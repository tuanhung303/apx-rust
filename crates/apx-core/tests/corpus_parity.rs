use apx_core::{Instruction, Operation, parse};
use serde::Deserialize;
use std::path::PathBuf;

const REQUIRED_ORACLE_REVISION: &str = "bcd85fc2f817e7c405f8b92953cd3ad4db165759";

#[derive(Deserialize)]
struct Corpus {
    go_oracle_revision: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    script: String,
    expected: Expected,
}

#[derive(Deserialize, Default)]
struct Expected {
    #[serde(default, deserialize_with = "de_null_default")]
    instructions: Vec<ExpectedInstruction>,
    #[serde(default, deserialize_with = "de_null_default")]
    errors: Vec<ExpectedError>,
}

fn de_null_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ExpectedInstruction {
    line: usize,
    operation: String,
    path: String,
    line_number: usize,
    end_line: usize,
    start: usize,
    end: usize,
    count: usize,
    text: String,
    end_text: String,
}

#[derive(Deserialize, Debug)]
struct ExpectedError {
    line: usize,
    command: usize,
    operation: String,
    category: String,
    message: String,
    diagnostic: String,
}

#[derive(Debug, PartialEq, Eq)]
struct ActualInstruction {
    line: usize,
    operation: String,
    path: String,
    line_number: usize,
    end_line: usize,
    start: usize,
    end: usize,
    count: usize,
    text: String,
    end_text: String,
}

impl ActualInstruction {
    fn from_instruction(instruction: &Instruction) -> Self {
        let mut path = String::new();
        let mut line_number = 0;
        let mut end_line = 0;
        let mut start = 0;
        let mut end = 0;
        let mut count = 0;
        let mut text = String::new();
        let mut end_text = String::new();
        let operation = match &instruction.operation {
            Operation::In { path: value } => {
                path.clone_from(value);
                "in"
            }
            Operation::New { path: value } => {
                path.clone_from(value);
                "new"
            }
            Operation::Move { path: value } => {
                path.clone_from(value);
                "mv"
            }
            Operation::Remove { path: value } => {
                if let Some(value) = value {
                    path.clone_from(value);
                }
                "rm"
            }
            Operation::Select {
                line: value,
                start: s,
                end: e,
            } => {
                line_number = *value;
                start = *s;
                end = *e;
                "sel"
            }
            Operation::TextSelect {
                line: value,
                text: t,
                count: c,
            } => {
                line_number = *value;
                text.clone_from(t);
                count = *c;
                "tsel"
            }
            Operation::BlockSelect { start: s, end: e } => {
                text.clone_from(s);
                end_text.clone_from(e);
                "bsel"
            }
            Operation::RangeSelect { start: s, end: e } => {
                line_number = *s;
                end_line = *e;
                "rsel"
            }
            Operation::Type { text: value } => {
                text.clone_from(value);
                "type"
            }
            Operation::Delete => "del",
            Operation::Copy => "copy",
            Operation::Cut => "cut",
            Operation::Paste => "paste",
            Operation::Commit => "commit",
        };
        Self {
            line: instruction.line,
            operation: operation.to_owned(),
            path,
            line_number,
            end_line,
            start,
            end,
            count,
            text,
            end_text,
        }
    }
}

#[test]
fn corpus_exact_parity() {
    let corpus_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus/parser.json");
    let raw = std::fs::read_to_string(&corpus_path)
        .expect("corpus fixture must exist; run scripts/parity.py generate");
    let corpus: Corpus = serde_json::from_str(&raw).expect("corpus fixture must be valid JSON");

    assert_eq!(
        corpus.go_oracle_revision, REQUIRED_ORACLE_REVISION,
        "corpus must come from the frozen oracle revision; run scripts/parity.py generate"
    );

    let mut failures: Vec<String> = Vec::new();
    for case in &corpus.cases {
        match parse(&case.script) {
            Ok(program) => {
                if !case.expected.errors.is_empty() {
                    failures.push(format!(
                        "{}: Go reports errors but Rust accepted\n  {}",
                        case.name,
                        case.expected.errors[0].diagnostic.trim_end()
                    ));
                }
                let actual: Vec<ActualInstruction> = program
                    .instructions
                    .iter()
                    .map(ActualInstruction::from_instruction)
                    .collect();
                if actual.len() != case.expected.instructions.len() {
                    failures.push(format!(
                        "{}: instruction count Rust={} Go={}",
                        case.name,
                        actual.len(),
                        case.expected.instructions.len()
                    ));
                }
                for (index, (got, want)) in actual
                    .iter()
                    .zip(case.expected.instructions.iter())
                    .enumerate()
                {
                    let got = (
                        got.line,
                        &got.operation,
                        &got.path,
                        got.line_number,
                        got.end_line,
                        got.start,
                        got.end,
                        got.count,
                        &got.text,
                        &got.end_text,
                    );
                    let want = (
                        want.line,
                        &want.operation,
                        &want.path,
                        want.line_number,
                        want.end_line,
                        want.start,
                        want.end,
                        want.count,
                        &want.text,
                        &want.end_text,
                    );
                    if got != want {
                        failures.push(format!(
                            "{}: instruction {index} mismatch\n  got:  {got:?}\n  want: {want:?}",
                            case.name
                        ));
                    }
                }
            }
            Err(group) => {
                if !case.expected.instructions.is_empty() {
                    failures.push(format!(
                        "{}: Go accepts but Rust rejected ({} errors)",
                        case.name,
                        group.commands.len()
                    ));
                }
                let actual: Vec<(usize, usize, String, String, String, String)> = group
                    .commands
                    .iter()
                    .map(|error| {
                        (
                            error.command,
                            error.line,
                            error.operation.clone(),
                            error.category.clone(),
                            error.message.clone(),
                            error.diagnostic(),
                        )
                    })
                    .collect();
                let want: Vec<(usize, usize, String, String, String, String)> = case
                    .expected
                    .errors
                    .iter()
                    .map(|error| {
                        (
                            error.command,
                            error.line,
                            error.operation.clone(),
                            error.category.clone(),
                            error.message.clone(),
                            error.diagnostic.clone(),
                        )
                    })
                    .collect();
                if actual != want {
                    failures.push(format!(
                        "{}: error mismatch\n  got:  {actual:?}\n  want: {want:?}",
                        case.name
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "corpus parity failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
