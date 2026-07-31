#![forbid(unsafe_code)]
//! Compatibility CLI: apply/translate/gain, mirroring the frozen Go oracle's
//! command-line contract (`apx [--root ROOT] [--cwd CWD] < SCRIPT`).

use apx_core::{evaluate, evaluate_peek, parse, Instruction, Operation, Program};
use apx_local::{apply, canonicalize_root, resolve_paths, FsBaseline};
use std::io::Read;
use std::path::{Path, PathBuf};

mod gain;

const HELP_TEXT: &str = r#"Usage:
  apx [--root ROOT] [--cwd CWD] < SCRIPT
  apx translate [--root ROOT] [--cwd CWD] < SCRIPT
  apx check [--root ROOT] [--cwd CWD] < SCRIPT
  apx peek [--root ROOT] [--cwd CWD] < SCRIPT
  apx gain
  apx --help
  apx translate --help
  apx --tool-help
  apx --version

Input and output:
  apx reads the complete editing script from standard input, validates and
  evaluates every command in memory, stages all changes, and only then commits.
  Normal-mode success writes the final-state report to stderr. translate never
  modifies files, writes one OpenAI apply_patch envelope to stdout, and then
  writes the pending final-state report to stderr. Failures use stderr and
  nonzero status.

Editing commands:
  in PATH                             select or reselect an existing file baseline
  new PATH                            select a pending empty file at cursor 0:0
  mv PATH                             move the active pending file without changing its baseline
  rm                                  remove the active file and clear editor state
  rm PATH                             remove a baseline file by path
  sel LINE START:END                  select inclusive one-based rune columns
  tsel FROM_LINE "TEXT" [N]           select the first N separate matches from FROM_LINE
  bsel "START" "END"                  select one whole-file uniquely anchored block
  rsel START:END                      select inclusive complete logical lines
  type "TEXT"                         record replacement or insertion at baseline coordinates
  type <<TAG                          record literal multiline replacement or insertion text
  del                                 record deletion of the selection
  copy                                store the baseline selection in the script clipboard
  cut                                 store and delete the baseline selection
  paste                               insert clipboard text after the selection or at the cursor
  commit                              advance to the next immutable in-memory baseline

Baseline editor state:
  The first in for an existing file captures an immutable baseline. Every
  selector for that file resolves against that baseline, regardless of prior
  edits or command order. Returning with in resets the baseline cursor to 0:0
  and clears the selection, but retains recorded edits. mv preserves baseline
  identity. Text introduced by an earlier command is not selectable. A selector
  that overlaps baseline content already replaced or deleted by an earlier edit
  is rejected.

  Disjoint baseline edits are applied together after complete validation.
  Replacements or deletions that overlap, insertions inside a replaced span,
  and multiple insertions at one baseline position are conflicts and reject the
  complete script. An insertion exactly at a replacement boundary is
  unambiguous and permitted. The script-local clipboard survives file changes,
  may be pasted repeatedly, and is discarded after the script completes.

  Never re-emit a whole file when the change is localized: if fewer than half
  of a file's lines change, keep the untouched lines out of the replacement
  (use tsel, bsel, or type), even though rsel over the whole file would also
  apply. Whole-file rewrites are only acceptable for small files or true
  rewrites, and they cost tokens.

Metrics:
  apx gain reads no script and reports the router/metrics view when available.

Read-only modes:
  apx check validates the complete script exactly like a normal run but never
  touches files and never prints the patch envelope: success is one short
  stdout line (ok N edits across M files), failures are the usual diagnostics.
  Use check, never translate, to validate a script before applying it.

  apx peek is read-only file viewing through selectors. Its script may contain
  only in/sel/tsel/bsel/rsel; after each selector it prints just the selected
  lines with one-based line numbers to stdout. Use it instead of cat/nl on
  whole files to read exactly the regions you plan to edit.

Agent workflow:
  1. Peek first: read only the regions you will edit (apx peek, or
     nl -ba FILE | sed -n 'A,Bp'), never whole files.
  2. Batch: group every edit for the task into one script, across as many
     files as needed; never split one file's edits across invocations. A
     script is atomic, so batching is safe.
  3. Validate cheaply with apx check, then apply once.

Paths:
  --root selects the trusted workspace boundary and defaults to apx's current
  directory. --cwd selects an existing directory within that root and defaults
  to ".". Relative script paths resolve from cwd. Absolute script paths may
  use the canonical root or a validated equivalent spelling. Paths that escape
  root, including through symlinks, are rejected. translate always emits
  root-relative paths.
"#;

const TRANSLATE_HELP: &str = r#"Usage:
  apx translate [--root ROOT] [--cwd CWD] < SCRIPT

Read and evaluate a complete editing script from standard input without
modifying files, then write one OpenAI apply_patch envelope to stdout and the
pending final-state report to stderr. Successful stdout is patch-only;
failures use stderr and nonzero status.

Attach SCRIPT through the execution interface's native non-PTY stdin field.
Do not use Python, printf, an encoding helper, a shell pipeline, or any
wrapper around apx translate. Run apx --help for the complete editing and
agent workflow.
"#;

const CHECK_HELP: &str = r#"Usage:
  apx check [--root ROOT] [--cwd CWD] < SCRIPT

Validate a complete editing script exactly like a normal run, without
modifying any file and without printing the apply_patch envelope. Success
writes one short line to stdout (ok N edits across M files); failures use
stderr and nonzero status. Prefer check over translate for validation.
"#;

const PEEK_HELP: &str = r#"Usage:
  apx peek [--root ROOT] [--cwd CWD] < SCRIPT

Read-only file viewing through selectors. The script may contain only
in/sel/tsel/bsel/rsel. After each selector command, peek prints just the
selected lines with one-based line numbers to stdout. It never modifies
files. Use it to read exactly the regions you plan to edit instead of
printing whole files.
"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = dispatch_informational(&args) {
        std::process::exit(code);
    }
    let mut script = String::new();
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    if let Err(error) = handle.read_to_string(&mut script) {
        eprintln!("apx: reading script: {error}");
        std::process::exit(1);
    }
    std::process::exit(run(&args, &script));
}

fn dispatch_informational(args: &[String]) -> Option<i32> {
    match args {
        [flag] if flag == "--help" => {
            print!("{HELP_TEXT}");
            Some(0)
        }
        [a, b] if a == "translate" && b == "--help" => {
            print!("{TRANSLATE_HELP}");
            Some(0)
        }
        [a, b] if a == "check" && b == "--help" => {
            print!("{CHECK_HELP}");
            Some(0)
        }
        [a, b] if a == "peek" && b == "--help" => {
            print!("{PEEK_HELP}");
            Some(0)
        }
        [flag] if flag == "--tool-help" => {
            print!("{}", apx_core::TOOL_DESCRIPTION_COMPACT);
            Some(0)
        }
        [flag] if flag == "--version" => {
            println!("apx {} (apx-rust {})", env!("CARGO_PKG_VERSION"), git_sha());
            Some(0)
        }
        [flag] if flag == "gain" => match gain::run_gain() {
            Ok(body) => {
                println!("{body}");
                Some(0)
            }
            Err(error) => {
                eprintln!("apx: {error}");
                Some(1)
            }
        },
        _ => None,
    }
}

fn run(args: &[String], script: &str) -> i32 {
    let invocation = match parse_invocation(args) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("apx: {message}");
            return 1;
        }
    };

    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_arg = invocation
        .root
        .clone()
        .unwrap_or_else(|| current.to_string_lossy().into_owned());
    let root_path = PathBuf::from(&root_arg);
    if !root_path.is_absolute() {
        eprintln!("apx: workspace root must be absolute");
        return 1;
    }
    let (canonical_root, alias) = match canonicalize_root(&root_path) {
        Ok(pair) => pair,
        Err(message) => {
            eprintln!("apx: {message}");
            return 1;
        }
    };
    let cwd = match resolve_cwd(&canonical_root, &alias, invocation.cwd.as_deref().unwrap_or(".")) {
        Ok(cwd) => cwd,
        Err(message) => {
            eprintln!("apx: {message}");
            return 1;
        }
    };

    let program = match parse(script) {
        Ok(program) => program,
        Err(errors) => {
            return fail_commands(&errors.commands);
        }
    };
    let program = match resolve_paths(program, &canonical_root, &alias, &cwd) {
        Ok(program) => program,
        Err(message) => {
            eprintln!("apx: {message}");
            return 1;
        }
    };

    let baseline = FsBaseline::new(canonical_root.clone());

    if invocation.mode == Mode::Peek {
        return match evaluate_peek(&baseline, &program) {
            Ok(output) => {
                print!("{output}");
                0
            }
            Err(error) => {
                eprint!("{}", error.diagnostic());
                1
            }
        };
    }

    let evaluation = match evaluate(&baseline, &program) {
        Ok(evaluation) => evaluation,
        Err(error) => {
            eprint!("{}", error.diagnostic());
            return 1;
        }
    };

    if invocation.mode == Mode::Check {
        let edits = program
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::Type { .. } | Operation::Delete | Operation::Cut | Operation::Paste
                )
            })
            .count();
        let files = evaluation.changes.changes.len();
        println!(
            "ok {edits} edit{} across {files} file{}",
            if edits == 1 { "" } else { "s" },
            if files == 1 { "" } else { "s" },
        );
        return 0;
    }

    if invocation.mode == Mode::Translate {
        return match apx_core::translate_apply_patch(&evaluation.changes) {
            Ok(patch) => {
                print!("{patch}");
                eprint!("{}", evaluation.report);
                0
            }
            Err(message) => {
                eprintln!("apx: {message}");
                1
            }
        };
    }

    if evaluation.changes.changes.is_empty() {
        eprint!("{}", evaluation.report);
        return 0;
    }
    match apply(&canonical_root, &evaluation.changes) {
        Ok(()) => {
            eprint!("{}", evaluation.report);
            0
        }
        Err(message) => {
            eprintln!("apx: {message}");
            1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Apply,
    Translate,
    Check,
    Peek,
}

struct Invocation {
    mode: Mode,
    root: Option<String>,
    cwd: Option<String>,
}

fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    let mut mode = Mode::Apply;
    let mut rest = args;
    if let Some(first) = rest.first().map(String::as_str) {
        match first {
            "translate" => {
                mode = Mode::Translate;
                rest = &rest[1..];
            }
            "check" => {
                mode = Mode::Check;
                rest = &rest[1..];
            }
            "peek" => {
                mode = Mode::Peek;
                rest = &rest[1..];
            }
            _ => {}
        }
    }
    let mut root = None;
    let mut cwd = None;
    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        if index + 1 >= rest.len() || (flag != "--root" && flag != "--cwd") {
            return Err(
                "expected no arguments or exactly: [--root ROOT] [--cwd CWD], translate [--root ROOT] [--cwd CWD], check [--root ROOT] [--cwd CWD], peek [--root ROOT] [--cwd CWD], or gain"
                    .to_owned(),
            );
        }
        let value = rest[index + 1].clone();
        if value.is_empty() {
            return Err(format!("{flag} requires a nonempty value"));
        }
        match flag {
            "--root" => {
                if root.is_some() {
                    return Err("--root may be specified only once".to_owned());
                }
                root = Some(value);
            }
            "--cwd" => {
                if cwd.is_some() {
                    return Err("--cwd may be specified only once".to_owned());
                }
                cwd = Some(value);
            }
            _ => unreachable!(),
        }
        index += 2;
    }
    Ok(Invocation { mode, root, cwd })
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
    Ok(if relative.is_empty() { ".".to_owned() } else { relative })
}


fn fail_commands(errors: &[apx_core::CommandError]) -> i32 {
    for error in errors {
        eprint!("{}", error.diagnostic());
    }
    1
}

fn git_sha() -> &'static str {
    option_env!("APX_GIT_SHA").unwrap_or("unknown")
}

#[allow(dead_code)]
fn _instruction_types(_program: &Program) {
    // Keeps Instruction/Operation imported for downstream tooling.
    let _ = std::mem::size_of::<Instruction>();
    let _ = std::mem::size_of::<Operation>();
}
