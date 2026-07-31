use apx_core::{Instruction, Operation, Program};
use serde::Deserialize;
use std::path::PathBuf;

pub const REQUIRED_ORACLE_REVISION: &str = "bcd85fc2f817e7c405f8b92953cd3ad4db165759";

#[derive(Deserialize)]
pub struct Corpus {
    pub go_oracle_revision: String,
    pub cases: Vec<Case>,
}

#[derive(Deserialize)]
pub struct Case {
    pub name: String,
    pub script: String,
    #[allow(dead_code)] // read only by round-trip tests; unused by fuzz binaries
    pub expected: Expected,
}

#[derive(Deserialize)]
pub struct Expected {
    #[serde(default, deserialize_with = "de_null_default")]
    #[allow(dead_code)] // read only by round-trip tests; unused by fuzz binaries
    pub errors: Vec<serde_json::Value>,
}

pub fn de_null_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

pub fn load_corpus() -> Corpus {
    let corpus_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus/parser.json");
    let raw = std::fs::read_to_string(&corpus_path).expect("corpus fixture must exist");
    let corpus: Corpus = serde_json::from_str(&raw).expect("corpus must be valid JSON");
    assert_eq!(
        corpus.go_oracle_revision, REQUIRED_ORACLE_REVISION,
        "corpus must come from the frozen oracle revision"
    );
    corpus
}

pub fn serialize_program(program: &Program) -> String {
    let mut script = String::new();
    let mut last_line = 0;
    for instruction in &program.instructions {
        for _ in last_line + 1..instruction.line {
            script.push('\n');
        }
        script.push_str(&serialize_instruction(instruction));
        script.push('\n');
        last_line = instruction.line;
    }
    script
}

pub fn serialize_instruction(instruction: &Instruction) -> String {
    match &instruction.operation {
        Operation::In { path } => format!("in {path}"),
        Operation::New { path } => format!("new {path}"),
        Operation::Move { path } => format!("mv {path}"),
        Operation::Remove { path: Some(path) } => format!("rm {path}"),
        Operation::Remove { path: None } => "rm".to_owned(),
        Operation::Select { line, start, end } => format!("sel {line} {start}:{end}"),
        Operation::TextSelect { line, text, count } => {
            let mut line = format!("tsel {line} {}", quote(text));
            if *count != 1 {
                line.push(' ');
                line.push_str(&count.to_string());
            }
            line
        }
        Operation::BlockSelect { start, end } => format!("bsel {} {}", quote(start), quote(end)),
        Operation::RangeSelect { start, end } => format!("rsel {start}:{end}"),
        Operation::Type { text } => format!("type {}", quote(text)),
        Operation::Delete => "del".to_owned(),
        Operation::Copy => "copy".to_owned(),
        Operation::Cut => "cut".to_owned(),
        Operation::Paste => "paste".to_owned(),
        Operation::Commit => "commit".to_owned(),
    }
}

pub fn quote(text: &str) -> String {
    serde_json::to_string(text).expect("JSON escaping of an instruction string cannot fail")
}
