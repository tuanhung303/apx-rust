#![forbid(unsafe_code)]
//! Compatibility CLI: apply/translate/gain, mirroring the frozen Go oracle's
//! command-line contract (`apx [--root ROOT] [--cwd CWD] < SCRIPT`).

use apx_core::{evaluate, parse, Instruction, Operation, Program};
use apx_local::{apply, canonicalize_root, resolve_paths, FsBaseline};
use std::io::Read;
use std::path::{Path, PathBuf};

mod gain;

const HELP_TEXT: &str = r#"Usage:
  apx [--root ROOT] [--cwd CWD] < SCRIPT
  apx translate [--root ROOT] [--cwd CWD] < SCRIPT
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
    let cwd = match resolve_cwd(&canonical_root, invocation.cwd.as_deref().unwrap_or(".")) {
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
    let evaluation = match evaluate(&baseline, &program) {
        Ok(evaluation) => evaluation,
        Err(error) => {
            eprint!("{}", error.diagnostic());
            return 1;
        }
    };

    if invocation.translate {
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

struct Invocation {
    translate: bool,
    root: Option<String>,
    cwd: Option<String>,
}

fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    let mut translate = false;
    let mut rest = args;
    if rest.first().map(String::as_str) == Some("translate") {
        translate = true;
        rest = &rest[1..];
    }
    let mut root = None;
    let mut cwd = None;
    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        if index + 1 >= rest.len() || (flag != "--root" && flag != "--cwd") {
            return Err(
                "expected no arguments or exactly: [--root ROOT] [--cwd CWD], translate [--root ROOT] [--cwd CWD], or gain"
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
    Ok(Invocation { translate, root, cwd })
}

fn resolve_cwd(canonical_root: &Path, cwd: &str) -> Result<String, String> {
    let relative = if cwd.starts_with('/') {
        let stripped = strip_absolute_prefix(cwd, canonical_root);
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

fn strip_absolute_prefix(path: &str, root: &Path) -> Option<String> {
    let cleaned = apx_core::clean_path(path);
    let mut components = cleaned.split('/').peekable();
    let mut root_components = root.components();
    loop {
        match (components.peek(), root_components.next()) {
            (Some(&part), Some(component)) => {
                if part != component.as_os_str().to_str().unwrap_or("") {
                    return None;
                }
                components.next();
            }
            (Some(_), None) => {
                return Some(components.collect::<Vec<_>>().join("/"));
            }
            (None, Some(_)) => return None,
            (None, None) => return Some(String::new()),
        }
    }
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
