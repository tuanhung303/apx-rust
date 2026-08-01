#![forbid(unsafe_code)]
//! Minimal hand-rolled MCP (Model Context Protocol) stdio server exposing two
//! tools, `apx` (apply APX edit scripts) and `peek` (read-only region reads),
//! both through the local engine.
//! Transport is newline-delimited JSON-RPC 2.0 on stdin/stdout; everything
//! else (logs, diagnostics) goes to stderr. The apply flow mirrors
//! `apx-cli`'s `run()` with `Mode::Apply` exactly.

use apx_core::{evaluate, evaluate_peek, parse};
use apx_local::{FsBaseline, apply, canonicalize_root, resolve_paths};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Tool description exposed to the model through `tools/list`.
const APX_MCP_DESCRIPTION: &str = "ONE atomic script, ALL edits; rejection changes nothing; fix per diagnostic, retry. No whole-file rewrite for local edits. `in PATH` existing, `new PATH` creates; paths from `cwd` (default `.`), inside `root`. `tsel FROM_LINE \"TEXT\" [N]` first N exact 1-line matches, `bsel \"START\" \"END\"` two-anchor span: FRAGMENT-only, replaces match, never line; `rsel S:E` COMPLETE LINES. `type \"TEXT\"` or `type <<PATCH` heredoc, PATCH after last content line. `mv DEST` needs prior `in`; `rm`/`del`/`commit`. Selectors double-quoted (\\\" escaped); single quotes invalid. Line numbers frozen baseline; never adjust for earlier edits; inserts unselectable. Worked example (batch, 2 files + heredoc): `in a.go\ntsel 4 \"func oldName()\"\ntype \"func newName()\"\nin b.go\nrsel 12:14\ntype <<PATCH\nnew body\nPATCH\ncommit`. Use the `peek` tool for one-based line numbers; never hand-count lines.";

const PEEK_MCP_DESCRIPTION: &str = "Read-only file viewing through the same selectors as the apx tool. Script may contain only in, sel, tsel, bsel, rsel; after each selector it prints just the selected lines, one-based numbered. Never modifies files — use it to read exactly the regions you plan to edit instead of printing whole files. Output line numbers are the coordinates for `tsel`/`rsel`/`sel` — copy them straight into an apx script; cheaper than `nl -ba` and never drift from the baseline. Multi-file reads in ONE script replace `nl -ba` batches: `in a.go\nrsel 1:30\nin b.go\nrsel 1:40`.";

fn main() {
    let root_default = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("apx-mcp: reading stdin: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(&line, &root_default) {
            if let Err(error) = writeln!(stdout, "{response}") {
                eprintln!("apx-mcp: writing stdout: {error}");
                break;
            }
            if let Err(error) = stdout.flush() {
                eprintln!("apx-mcp: flushing stdout: {error}");
                break;
            }
        }
    }
}

/// Handle one newline-delimited JSON-RPC 2.0 line and return the response
/// JSON line, or `None` for notifications (which never get a response).
fn handle_line(line: &str, root_default: &Path) -> Option<String> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(_) => return Some(rpc_error_response(Value::Null, -32700, "parse error")),
    };
    let Some(request) = message.as_object() else {
        return Some(rpc_error_response(Value::Null, -32600, "invalid request"));
    };
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(rpc_error_response(
            rpc_id(request),
            -32600,
            "invalid request",
        ));
    };
    let id = rpc_id(request);
    if request.get("id").is_none() || method == "notifications/initialized" {
        return None;
    }
    let result = match method {
        "initialize" => initialize_result(),
        "ping" => Value::Object(serde_json::Map::new()),
        "tools/list" => tools_list(),
        "tools/call" => match tools_call(request.get("params"), root_default) {
            Ok(result) => result,
            Err((code, message)) => {
                return Some(rpc_error_response(id, code, message));
            }
        },
        _ => return Some(rpc_error_response(id, -32601, "method not found")),
    };
    Some(rpc_response(id, result))
}

/// Extract the client-supplied request id, defaulting to `null`.
fn rpc_id(request: &serde_json::Map<String, Value>) -> Value {
    request.get("id").cloned().unwrap_or(Value::Null)
}

fn rpc_response(id: Value, result: Value) -> String {
    serde_json::to_string(&json!({"jsonrpc": "2.0", "id": id, "result": result})).unwrap()
}

fn rpc_error_response(id: Value, code: i64, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    }))
    .unwrap()
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-03-26",
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "apx-mcp", "version": env!("CARGO_PKG_VERSION")}
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "apx",
                "description": APX_MCP_DESCRIPTION,
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "script": {"type": "string"},
                        "root": {"type": "string"},
                        "cwd": {"type": "string"}
                    },
                    "required": ["script"]
                }
            },
            {
                "name": "peek",
                "description": PEEK_MCP_DESCRIPTION,
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "script": {"type": "string"},
                        "root": {"type": "string"},
                        "cwd": {"type": "string"}
                    },
                    "required": ["script"]
                }
            }
        ]
    })
}

/// Run the `apx` tool: apply `arguments.script` with the CLI's exact
/// `Mode::Apply` flow. Every outcome — success, parse rejection, evaluate
/// rejection, apply failure, path escape — returns a normal tool result so
/// the model sees diagnostics and can retry.
fn tools_call(params: Option<&Value>, root_default: &Path) -> Result<Value, (i64, &'static str)> {
    let params = params
        .and_then(Value::as_object)
        .ok_or((-32602, "invalid params"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "invalid params"))?;
    let arguments = params
        .get("arguments")
        .and_then(Value::as_object)
        .ok_or((-32602, "invalid params"))?;
    let script = arguments
        .get("script")
        .and_then(Value::as_str)
        .ok_or((-32602, "invalid params"))?;
    let root = match arguments.get("root") {
        Some(value) => Some(value.as_str().ok_or((-32602, "invalid params"))?.to_owned()),
        None => Some(root_default.to_string_lossy().into_owned()),
    };
    let cwd = match arguments.get("cwd") {
        Some(value) => Some(value.as_str().ok_or((-32602, "invalid params"))?.to_owned()),
        None => None,
    };
    let (text, success) = match name {
        "apx" => run_script(script, root.as_deref(), cwd.as_deref()),
        "peek" => run_peek_script(script, root.as_deref(), cwd.as_deref()),
        _ => return Err((-32602, "unknown tool")),
    };
    Ok(json!({
        "content": [{"type": "text", "text": text}],
        "isError": !success
    }))
}

/// Apply `script` under `root` (default: the current directory) with working
/// directory `cwd` (default: `.`). Returns `(output_text, ok)`: the report
/// or diagnostic text, and whether the script applied successfully.
fn run_script(script: &str, root: Option<&str>, cwd: Option<&str>) -> (String, bool) {
    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_arg = match root {
        Some(root) => root.to_owned(),
        None => current.to_string_lossy().into_owned(),
    };
    let root_path = PathBuf::from(&root_arg);
    if !root_path.is_absolute() {
        return failure("apx: workspace root must be absolute".to_owned());
    }
    let (canonical_root, alias) = match canonicalize_root(&root_path) {
        Ok(pair) => pair,
        Err(message) => return failure(format!("apx: {message}")),
    };
    let cwd = match resolve_cwd(&canonical_root, &alias, cwd.unwrap_or(".")) {
        Ok(cwd) => cwd,
        Err(message) => return (format!("apx: {message}"), false),
    };
    let program = match parse(script) {
        Ok(program) => program,
        Err(errors) => {
            let text: String = errors
                .commands
                .iter()
                .map(|error| error.diagnostic())
                .collect();
            return failure(text);
        }
    };
    let program = match resolve_paths(program, &canonical_root, &alias, &cwd) {
        Ok(program) => program,
        Err(message) => return (format!("apx: {message}"), false),
    };
    let baseline = FsBaseline::new(canonical_root.clone());
    let evaluation = match evaluate(&baseline, &program) {
        Ok(evaluation) => evaluation,
        Err(error) => return failure(error.diagnostic()),
    };
    if evaluation.changes.changes.is_empty() {
        return (evaluation.report, true);
    }
    match apply(&canonical_root, &evaluation.changes) {
        Ok(()) => (evaluation.report, true),
        Err(message) => failure(format!("apx: {message}")),
    }
}

fn failure(text: String) -> (String, bool) {
    (
        format!("{}\nno changes applied (atomic)", text.trim_end()),
        false,
    )
}

/// Validate `cwd` against the canonical root exactly like the CLI: clean the
/// path, reject escapes above the root, and require an existing directory.
fn run_peek_script(script: &str, root: Option<&str>, cwd: Option<&str>) -> (String, bool) {
    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_arg = match root {
        Some(root) => root.to_owned(),
        None => current.to_string_lossy().into_owned(),
    };
    let root_path = PathBuf::from(&root_arg);
    if !root_path.is_absolute() {
        return (
            "apx peek: workspace root must be absolute".to_owned(),
            false,
        );
    }
    let (canonical_root, alias) = match canonicalize_root(&root_path) {
        Ok(pair) => pair,
        Err(message) => return (format!("apx peek: {message}"), false),
    };
    let cwd = match resolve_cwd(&canonical_root, &alias, cwd.unwrap_or(".")) {
        Ok(cwd) => cwd,
        Err(message) => return (format!("apx peek: {message}"), false),
    };
    let program = match parse(script) {
        Ok(program) => program,
        Err(errors) => {
            let text: String = errors
                .commands
                .iter()
                .map(|error| error.diagnostic())
                .collect();
            return (text, false);
        }
    };
    let program = match resolve_paths(program, &canonical_root, &alias, &cwd) {
        Ok(program) => program,
        Err(message) => return (format!("apx peek: {message}"), false),
    };
    let baseline = FsBaseline::new(canonical_root.clone());
    match evaluate_peek(&baseline, &program) {
        Ok(text) => (text, true),
        Err(error) => (error.diagnostic(), false),
    }
}

fn resolve_cwd(canonical_root: &Path, alias: &Path, cwd: &str) -> Result<String, String> {
    let relative = if cwd.starts_with('/') {
        let stripped = apx_local::strip_root_prefix(cwd, canonical_root)
            .or_else(|| apx_local::strip_root_prefix(cwd, alias));
        match stripped {
            Some(relative) if !relative.is_empty() => relative,
            Some(_) => ".".to_owned(),
            None => return Err("workspace cwd must resolve within root".to_owned()),
        }
    } else {
        let cleaned = apx_core::clean_path(cwd);
        if cleaned == ".." || cleaned.starts_with("../") {
            return Err("workspace cwd must resolve within root".to_owned());
        }
        cleaned
    };
    let target = canonical_root.join(&relative);
    let metadata = std::fs::metadata(&target)
        .map_err(|error| format!("canonicalizing workspace cwd: {error}"))?;
    if !metadata.is_dir() {
        return Err("workspace cwd must be a directory".to_owned());
    }
    Ok(if relative.is_empty() {
        ".".to_owned()
    } else {
        relative
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apx-mcp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn initialize_response_shape() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
            Path::new("."),
        )
        .expect("initialize must produce a response");
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(
            value["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert_eq!(value["result"]["serverInfo"]["name"], "apx-mcp");
        assert_eq!(value["result"]["serverInfo"]["version"], "0.1.0");
        assert!(value["error"].is_null());
    }

    #[test]
    fn tools_list_declares_apx_tool_with_required_script() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":"t1","method":"tools/list"}"#,
            Path::new("."),
        )
        .expect("tools/list must produce a response");
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], "t1");
        let tools = value["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 2);
        let tool = &tools[0];
        assert_eq!(tool["name"], "apx");
        assert_eq!(tool["description"], APX_MCP_DESCRIPTION);
        assert_eq!(tool["inputSchema"]["required"], json!(["script"]));
        assert_eq!(
            tool["inputSchema"]["properties"]["script"]["type"],
            "string"
        );
        assert_eq!(tool["inputSchema"]["properties"]["root"]["type"], "string");
        assert_eq!(tool["inputSchema"]["properties"]["cwd"]["type"], "string");
        let peek = &tools[1];
        assert_eq!(peek["name"], "peek");
        assert_eq!(peek["description"], PEEK_MCP_DESCRIPTION);
        assert_eq!(peek["inputSchema"]["required"], json!(["script"]));
    }

    #[test]
    fn run_script_success_edits_scratch_file_and_reports() {
        let dir = scratch("success");
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let (text, ok) = run_script(
            "in a.txt\ntsel 2 \"two\"\ntype \"TWO\"\n",
            Some(dir.to_str().unwrap()),
            None,
        );
        assert!(ok, "{text}");
        assert!(text.contains("1 file change"), "{text}");
        assert!(text.contains("update a.txt (+1/-1)"), "{text}");
        assert!(text.contains("changed lines:"), "{text}");
        assert!(text.contains("- two"), "{text}");
        assert!(text.contains("+ TWO"), "{text}");

        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\nTWO\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_script_invalid_script_returns_diagnostic_and_leaves_file_unchanged() {
        let dir = scratch("invalid");
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let (text, ok) = run_script(
            "in a.txt\ntsel 2 \"missing\"\ntype \"X\"\n",
            Some(dir.to_str().unwrap()),
            None,
        );
        assert!(!ok, "{text}");
        assert!(!text.is_empty(), "diagnostic must be non-empty");
        assert!(text.contains("no changes applied (atomic)"), "{text}");

        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\ntwo\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_script_path_escape_fails_closed() {
        let dir = scratch("escape");
        let (text, ok) = run_script("in ../../etc/passwd\n", Some(dir.to_str().unwrap()), None);
        assert!(!ok, "{text}");
        assert!(text.contains("outside workspace root"), "{text}");
        let (cwd_text, cwd_ok) =
            run_script("in a.txt\n", Some(dir.to_str().unwrap()), Some("../../"));
        assert!(!cwd_ok, "{cwd_text}");
        assert!(
            cwd_text.contains("workspace cwd must resolve within root"),
            "{cwd_text}"
        );
        // The escape was rejected before any filesystem access; /etc/passwd is untouched.
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_script_accepts_absolute_cwd_equal_to_root() {
        let dir = scratch("abs-cwd-eq-root");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        let root = dir.to_string_lossy().into_owned();
        let (text, ok) = run_script(
            "in a.txt\ntsel 1 \"one\"\ntype \"ONE\"\ncommit",
            Some(&root),
            Some(&root),
        );
        assert!(ok, "expected success, got: {text}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "ONE\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_script_accepts_absolute_cwd_subdir() {
        let dir = scratch("abs-cwd-subdir");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.txt"), "one\n").unwrap();
        let root = dir.to_string_lossy().into_owned();
        let cwd = format!("{root}/sub");
        let (text, ok) = run_script(
            "in a.txt\ntsel 1 \"one\"\ntype \"ONE\"\ncommit",
            Some(&root),
            Some(&cwd),
        );
        assert!(ok, "expected success, got: {text}");
        assert_eq!(
            std::fs::read_to_string(dir.join("sub/a.txt")).unwrap(),
            "ONE\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tools_call_applies_script_end_to_end() {
        let dir = scratch("call");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        let params = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "apx",
                "arguments": {
                    "script": "in a.txt\ntsel 1 \"one\"\ntype \"ONE\"\n",
                    "root": dir.to_str().unwrap()
                }
            }
        });
        let response =
            handle_line(&serde_json::to_string(&params).unwrap(), Path::new(".")).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 9);
        assert_eq!(value["result"]["isError"], false);
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("update a.txt"), "{text}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "ONE\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tools_call_missing_script_is_invalid_params() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"apx","arguments":{}}}"#,
            Path::new("."),
        )
        .expect("tools/call must produce a response");
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["error"]["code"], -32602);
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/other"}"#,
            Path::new("."),
        )
        .expect("unknown method must produce an error response");
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 2);
        assert_eq!(value["error"]["code"], -32601);
    }

    #[test]
    fn run_peek_script_reads_region_and_never_writes() {
        let dir = scratch("peek");
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        let (text, ok) = run_peek_script("in a.txt\nrsel 2:3\n", Some(dir.to_str().unwrap()), None);
        assert!(ok, "{text}");
        assert!(text.contains("\ttwo"), "{text}");
        assert!(text.contains("\tthree"), "{text}");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
        let (bad, bad_ok) = run_peek_script(
            "in a.txt\ntsel 2 \"two\"\ntype \"TWO\"\n",
            Some(dir.to_str().unwrap()),
            None,
        );
        assert!(!bad_ok, "{bad}");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tools_call_peek_reads_region() {
        let dir = scratch("call-peek");
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let params = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "peek",
                "arguments": {
                    "script": "in a.txt\nrsel 2:2\n",
                    "root": dir.to_str().unwrap()
                }
            }
        });
        let response =
            handle_line(&serde_json::to_string(&params).unwrap(), Path::new(".")).unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 10);
        assert_eq!(value["result"]["isError"], false);
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\ttwo"), "{text}");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\ntwo\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn notifications_and_ping_are_handled() {
        assert!(
            handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                Path::new(".")
            )
            .is_none()
        );
        assert!(
            handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#,
                Path::new(".")
            )
            .is_none()
        );
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
            Path::new("."),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["result"], json!({}));
        assert_eq!(value["id"], 5);
    }
}
