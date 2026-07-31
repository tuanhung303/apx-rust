use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    In {
        path: String,
    },
    New {
        path: String,
    },
    Move {
        path: String,
    },
    Remove {
        path: Option<String>,
    },
    Select {
        line: usize,
        start: usize,
        end: usize,
    },
    TextSelect {
        line: usize,
        text: String,
        count: usize,
    },
    BlockSelect {
        start: String,
        end: String,
    },
    RangeSelect {
        start: usize,
        end: usize,
    },
    Type {
        text: String,
    },
    Delete,
    Copy,
    Cut,
    Paste,
    Commit,
}

impl Operation {
    pub fn name(&self) -> &'static str {
        match self {
            Self::In { .. } => "in",
            Self::New { .. } => "new",
            Self::Move { .. } => "mv",
            Self::Remove { .. } => "rm",
            Self::Select { .. } => "sel",
            Self::TextSelect { .. } => "tsel",
            Self::BlockSelect { .. } => "bsel",
            Self::RangeSelect { .. } => "rsel",
            Self::Type { .. } => "type",
            Self::Delete => "del",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::Commit => "commit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instruction {
    pub line: usize,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Program {
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandError {
    pub command: usize,
    pub line: usize,
    pub operation: String,
    pub category: String,
    pub message: String,
}

impl CommandError {
    pub fn diagnostic(&self) -> String {
        let error = self.to_string();
        let sanitized: String = error
            .chars()
            .flat_map(|character| match character {
                '\n' => "; ".chars().collect::<Vec<_>>(),
                value if value.is_control() => value.escape_default().collect(),
                value => vec![value],
            })
            .collect();
        format!("apx: {sanitized}\n")
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "command {}, source line {}, operation {:?}, category {}: {}",
            self.command, self.line, self.operation, self.category, self.message
        )
    }
}

impl std::error::Error for CommandError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandGroupError {
    pub commands: Vec<CommandError>,
}

impl std::fmt::Display for CommandGroupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, error) in self.commands.iter().enumerate() {
            if index != 0 {
                writeln!(formatter)?;
            }
            write!(formatter, "{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandGroupError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeKind {
    Add,
    Update,
    Delete,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    pub original_path: Option<String>,
    pub path: Option<String>,
    pub original: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeSet {
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evaluation {
    pub changes: ChangeSet,
    pub report: String,
}
