use crate::{CommandError, CommandGroupError, Instruction, Operation, Program};
use std::path::{Component, Path, PathBuf};

const MAX_HEREDOC_BODY_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalLine {
    pub text: String,
    pub terminator: String,
}

pub fn split_physical_lines(source: &str) -> Vec<PhysicalLine> {
    let raw: Vec<&str> = source.split('\n').collect();
    let count = raw.len();
    raw.into_iter()
        .enumerate()
        .map(|(index, raw_line)| {
            let mut text = raw_line;
            let terminator = if index < count - 1 {
                if let Some(trimmed) = text.strip_suffix('\r') {
                    text = trimmed;
                    "\r\n"
                } else {
                    "\n"
                }
            } else if let Some(trimmed) = text.strip_suffix('\r') {
                text = trimmed;
                "\r"
            } else {
                ""
            };
            PhysicalLine {
                text: text.to_owned(),
                terminator: terminator.to_owned(),
            }
        })
        .collect()
}

pub fn parse(source: &str) -> Result<Program, CommandGroupError> {
    let lines = split_physical_lines(source);
    let mut instructions = Vec::new();
    let mut failures = Vec::new();
    let mut index = 0;
    let mut command = 0;
    while index < lines.len() {
        let header = index;
        let line = &lines[index].text;
        index += 1;
        if line.trim().is_empty() {
            continue;
        }
        command += 1;
        let source_line = header + 1;
        let operation_name = line.split_whitespace().next().unwrap_or("").to_owned();
        let parsed = if is_quoted_command(line) && scan_quote(line, false) {
            let mut open = true;
            while index < lines.len() {
                open = scan_quote(&lines[index].text, open);
                index += 1;
                if !open {
                    break;
                }
            }
            Err(
                "physical newline inside quoted operand; encode line terminators as \\n or \\r"
                    .to_owned(),
            )
        } else if let Some(delimiter) = line.strip_prefix("type <<") {
            match parse_delimiter(delimiter) {
                Ok(delimiter) => {
                    let mut body = String::new();
                    let mut closed = false;
                    while index < lines.len() {
                        if lines[index].text == delimiter {
                            index += 1;
                            closed = true;
                            break;
                        }
                        if body.len() + lines[index].text.len() + lines[index].terminator.len()
                            <= MAX_HEREDOC_BODY_BYTES
                        {
                            body.push_str(&lines[index].text);
                            body.push_str(&lines[index].terminator);
                        }
                        index += 1;
                    }
                    if !closed {
                        Err(format!(
                            "unterminated heredoc; expected closing delimiter {delimiter}"
                        ))
                    } else if body.len() > MAX_HEREDOC_BODY_BYTES {
                        Err(format!(
                            "heredoc body exceeds {MAX_HEREDOC_BODY_BYTES} bytes"
                        ))
                    } else {
                        Ok(Operation::Type { text: body })
                    }
                }
                Err(error) => Err(error),
            }
        } else {
            parse_instruction(line)
        };
        match parsed {
            Ok(operation) => instructions.push(Instruction {
                line: source_line,
                operation,
            }),
            Err(message) => failures.push(CommandError {
                command,
                line: source_line,
                operation: operation_name,
                category: "syntax".to_owned(),
                message,
            }),
        }
    }
    if failures.is_empty() {
        Ok(Program { instructions })
    } else {
        Err(CommandGroupError { commands: failures })
    }
}

fn is_quoted_command(line: &str) -> bool {
    line.starts_with("type ") || line.starts_with("tsel ") || line.starts_with("bsel ")
}

fn scan_quote(line: &str, mut open: bool) -> bool {
    let mut escaped = false;
    for character in line.chars() {
        if !open {
            if character == '"' {
                open = true;
            }
        } else if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            open = false;
        }
    }
    open
}

fn parse_instruction(line: &str) -> Result<Operation, String> {
    for name in ["in", "new", "mv", "rm"] {
        if let Some(path) = line.strip_prefix(&format!("{name} ")) {
            if path.is_empty() {
                return Err("path must not be empty".to_owned());
            }
            let path = clean_path(path);
            return Ok(match name {
                "in" => Operation::In { path },
                "new" => Operation::New { path },
                "mv" => Operation::Move { path },
                "rm" => Operation::Remove { path: Some(path) },
                _ => unreachable!(),
            });
        }
    }
    match line {
        "rm" => return Ok(Operation::Remove { path: None }),
        "del" => return Ok(Operation::Delete),
        "copy" => return Ok(Operation::Copy),
        "cut" => return Ok(Operation::Cut),
        "paste" => return Ok(Operation::Paste),
        "commit" => return Ok(Operation::Commit),
        _ => {}
    }
    if let Some(rest) = line.strip_prefix("sel ") {
        let (line_number, columns) = rest
            .split_once(' ')
            .ok_or_else(|| "unknown or malformed command".to_owned())?;
        let (start, end) = columns
            .split_once(':')
            .ok_or_else(|| "unknown or malformed command".to_owned())?;
        let line = positive(line_number, "line reference")?;
        let start = positive(start, "number")?;
        let end = positive(end, "number")?;
        if start > end {
            return Err("selection start exceeds end".to_owned());
        }
        return Ok(Operation::Select { line, start, end });
    }
    if let Some(rest) = line.strip_prefix("tsel ") {
        let (line_number, encoded) = rest
            .split_once(' ')
            .ok_or_else(|| "unknown or malformed command".to_owned())?;
        let line = positive(line_number, "line reference")?;
        let (text, trailing) = decode_quoted(encoded)
            .map_err(|error| format!("invalid quoted string for tsel: {error}"))?;
        if text.is_empty() {
            return Err("tsel text must not be empty".to_owned());
        }
        if text.contains(['\r', '\n']) {
            return Err("tsel text must stay on one line".to_owned());
        }
        let count = if trailing.trim().is_empty() {
            1
        } else {
            positive(trailing.trim(), "tsel count").map_err(|_| "invalid tsel count".to_owned())?
        };
        return Ok(Operation::TextSelect { line, text, count });
    }
    if let Some(rest) = line.strip_prefix("bsel ") {
        let (start, trailing) =
            decode_quoted(rest).map_err(|error| format!("invalid bsel quoted strings: {error}"))?;
        if trailing.is_empty() || !trailing.as_bytes()[0].is_ascii_whitespace() {
            return Err(
                "invalid bsel quoted strings: quoted operands must be separated by whitespace"
                    .to_owned(),
            );
        }
        let (end, remainder) = decode_quoted(trailing.trim_start())
            .map_err(|error| format!("invalid bsel quoted strings: {error}"))?;
        if !remainder.trim().is_empty() {
            return Err(
                "invalid bsel quoted strings: trailing text after bsel literals".to_owned(),
            );
        }
        if start.is_empty() || end.is_empty() {
            return Err("bsel literals must not be empty".to_owned());
        }
        if start == end {
            return Err("bsel literals must differ".to_owned());
        }
        return Ok(Operation::BlockSelect { start, end });
    }
    if let Some(rest) = line.strip_prefix("rsel ") {
        let (start, end) = rest
            .split_once(':')
            .ok_or_else(|| "unknown or malformed command".to_owned())?;
        let start = positive(start, "line reference")?;
        let end = positive(end, "line reference")?;
        if start > end {
            return Err("line range start exceeds end".to_owned());
        }
        return Ok(Operation::RangeSelect { start, end });
    }
    if let Some(rest) = line.strip_prefix("type ") {
        let (text, trailing) = decode_quoted(rest)
            .map_err(|error| format!("invalid quoted string for type: {error}"))?;
        if !trailing.trim().is_empty() {
            return Err("trailing text after type string".to_owned());
        }
        return Ok(Operation::Type { text });
    }
    Err("unknown or malformed command".to_owned())
}

fn parse_delimiter(source: &str) -> Result<&str, String> {
    let delimiter = if source.len() >= 2
        && ((source.starts_with('"') && source.ends_with('"'))
            || (source.starts_with('\'') && source.ends_with('\'')))
    {
        &source[1..source.len() - 1]
    } else {
        source
    };
    if delimiter.is_empty()
        || delimiter.len() > 64
        || !delimiter
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || b"_.-".contains(&value))
    {
        Err("invalid heredoc delimiter; expected 1-64 ASCII letters, digits, underscores, dots, or hyphens, optionally enclosed in matching single or double quotes".to_owned())
    } else {
        Ok(delimiter)
    }
}

fn decode_quoted(source: &str) -> Result<(String, &str), String> {
    let source = source.trim_start_matches([' ', '\t', '\r', '\n']);
    if !source.starts_with('"') {
        return Err("quoted operand must begin with a double quote".to_owned());
    }
    let mut escaped = false;
    for (index, character) in source.char_indices().skip(1) {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => {
                let end = index + 1;
                let encoded = source[..end].replace('\t', "\\t");
                let value: String =
                    serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
                return Ok((value, &source[end..]));
            }
            '\n' | '\r' => {
                return Err(
                    "physical newline inside quoted operand; encode line terminators as \\n or \\r"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
    Err("unexpected end of JSON input".to_owned())
}

fn positive(value: &str, description: &str) -> Result<usize, String> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|character| character.is_ascii_digit())
    {
        return Err(format!("invalid {description} {value:?}"));
    }
    value
        .parse()
        .map_err(|_| format!("{description} is out of range"))
}

fn clean_path(path: &str) -> String {
    let mut result = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if result.file_name().is_some() => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    if result.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        result.to_string_lossy().into_owned()
    }
}
