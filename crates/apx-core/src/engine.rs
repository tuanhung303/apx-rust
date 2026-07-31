use crate::editor::Editor;
use crate::{Change, ChangeKind, ChangeSet, CommandError, Evaluation, Operation, Program};
use std::collections::{BTreeMap, BTreeSet};

pub trait Baseline {
    fn read(&self, path: &str) -> Result<Option<String>, String>;
    fn exists(&self, path: &str) -> Result<bool, String> {
        self.read(path).map(|value| value.is_some())
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryBaseline {
    files: BTreeMap<String, String>,
}

impl MemoryBaseline {
    pub fn new(files: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            files: files.into_iter().collect(),
        }
    }
}

impl Baseline for MemoryBaseline {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        Ok(self.files.get(path).cloned())
    }
}

struct FileState {
    original_path: Option<String>,
    path: String,
    original: String,
    created: bool,
    deleted: bool,
    editor: Editor,
}

struct Workspace<'a, B: Baseline> {
    baseline: &'a B,
    files: Vec<FileState>,
    paths: BTreeMap<String, usize>,
    reserved: BTreeSet<String>,
    active: Option<usize>,
    clipboard: Option<(String, bool)>,
}

pub fn evaluate<B: Baseline>(baseline: &B, program: &Program) -> Result<Evaluation, CommandError> {
    Workspace {
        baseline,
        files: Vec::new(),
        paths: BTreeMap::new(),
        reserved: BTreeSet::new(),
        active: None,
        clipboard: None,
    }
    .evaluate(program)
}

impl<B: Baseline> Workspace<'_, B> {
    fn evaluate(mut self, program: &Program) -> Result<Evaluation, CommandError> {
        for (index, instruction) in program.instructions.iter().enumerate() {
            if let Err(message) = self.execute(&instruction.operation, index + 1, instruction.line)
            {
                return Err(CommandError {
                    command: index + 1,
                    line: instruction.line,
                    operation: instruction.operation.name().to_owned(),
                    category: category(&instruction.operation).to_owned(),
                    message,
                });
            }
        }
        let changes = self.changes();
        let report = self.report(&changes);
        Ok(Evaluation { changes, report })
    }

    fn execute(
        &mut self,
        operation: &Operation,
        command: usize,
        line: usize,
    ) -> Result<(), String> {
        match operation {
            Operation::In { path } => return self.select(path),
            Operation::New { path } => return self.create(path),
            Operation::Move { path } => return self.move_active(path),
            Operation::Remove { path } => return self.remove(path.as_deref()),
            Operation::Commit => {
                for file in &mut self.files {
                    if !file.deleted {
                        file.editor.commit();
                    }
                }
                self.reserved.clear();
                return Ok(());
            }
            _ => {}
        }
        let active = self
            .active
            .ok_or_else(|| format!("{} requires an active file", operation.name()))?;
        let file = &mut self.files[active];
        match operation {
            Operation::Select { line, start, end } => {
                file.editor.select_columns(*line, *start, *end)
            }
            Operation::TextSelect {
                line, text, count, ..
            } => file.editor.select_matches(*line, text, *count),
            Operation::BlockSelect { start, end } => file.editor.select_block(start, end),
            Operation::RangeSelect { start, end } => file.editor.select_lines(*start, *end),
            Operation::Type { text } => file.editor.type_text(text, command, line),
            Operation::Delete => file.editor.delete(command, line),
            Operation::Copy | Operation::Cut => {
                let selected = file
                    .editor
                    .selected_clipboard()
                    .ok_or_else(|| format!("{} requires a selection", operation.name()))?;
                if matches!(operation, Operation::Cut) {
                    file.editor.delete(command, line)?;
                }
                self.clipboard = Some(selected);
                Ok(())
            }
            Operation::Paste => {
                let (text, linewise) = self.clipboard.clone().ok_or_else(|| {
                    "paste requires a preceding copy or cut in the same script".to_owned()
                })?;
                file.editor.paste(&text, linewise, command, line)
            }
            _ => unreachable!(),
        }
    }

    fn select(&mut self, path: &str) -> Result<(), String> {
        if let Some(index) = self.paths.get(path).copied() {
            self.active = Some(index);
            self.files[index].editor.reset();
            return Ok(());
        }
        let content = self
            .baseline
            .read(path)?
            .ok_or_else(|| format!("{path} does not exist"))?;
        let index = self.files.len();
        self.files.push(FileState {
            original_path: Some(path.to_owned()),
            path: path.to_owned(),
            original: content.clone(),
            created: false,
            deleted: false,
            editor: Editor::new(content),
        });
        self.paths.insert(path.to_owned(), index);
        self.active = Some(index);
        Ok(())
    }

    fn create(&mut self, path: &str) -> Result<(), String> {
        self.ensure_free(path)?;
        let index = self.files.len();
        self.files.push(FileState {
            original_path: None,
            path: path.to_owned(),
            original: String::new(),
            created: true,
            deleted: false,
            editor: Editor::default(),
        });
        self.paths.insert(path.to_owned(), index);
        self.active = Some(index);
        Ok(())
    }

    fn move_active(&mut self, path: &str) -> Result<(), String> {
        let index = self.active.ok_or_else(|| {
            "mv requires an active file; select one with in PATH first".to_owned()
        })?;
        self.ensure_free(path)?;
        let old = self.files[index].path.clone();
        self.paths.remove(&old);
        self.reserved.insert(old);
        self.files[index].path = path.to_owned();
        self.paths.insert(path.to_owned(), index);
        Ok(())
    }

    fn remove(&mut self, path: Option<&str>) -> Result<(), String> {
        let index = match path {
            None => self
                .active
                .ok_or_else(|| "rm requires an active file".to_owned())?,
            Some(path) => {
                if let Some(index) = self.paths.get(path).copied() {
                    index
                } else {
                    let content = self
                        .baseline
                        .read(path)?
                        .ok_or_else(|| format!("{path} does not exist"))?;
                    let index = self.files.len();
                    self.files.push(FileState {
                        original_path: Some(path.to_owned()),
                        path: path.to_owned(),
                        original: content.clone(),
                        created: false,
                        deleted: false,
                        editor: Editor::new(content),
                    });
                    self.paths.insert(path.to_owned(), index);
                    index
                }
            }
        };
        if !self.files[index].created && self.files[index].editor.has_edits() {
            return Err("cannot remove a baseline file after content edit".to_owned());
        }
        let path = self.files[index].path.clone();
        self.paths.remove(&path);
        self.reserved.insert(path);
        self.files[index].deleted = true;
        if self.active == Some(index) {
            self.active = None;
        }
        Ok(())
    }

    fn ensure_free(&self, path: &str) -> Result<(), String> {
        if self.paths.contains_key(path)
            || self.reserved.contains(path)
            || self.baseline.exists(path)?
        {
            Err(format!("destination {path} already exists"))
        } else {
            Ok(())
        }
    }

    fn changes(&self) -> ChangeSet {
        let mut changes = Vec::new();
        for file in &self.files {
            let content = file.editor.content();
            let change = match (
                file.created,
                file.deleted,
                file.original_path.as_deref() != Some(&file.path),
                file.original != content,
            ) {
                (true, true, _, _) => None,
                (true, false, _, _) => Some(Change {
                    kind: ChangeKind::Add,
                    original_path: None,
                    path: Some(file.path.clone()),
                    original: String::new(),
                    content,
                }),
                (false, true, _, _) => Some(Change {
                    kind: ChangeKind::Delete,
                    original_path: file.original_path.clone(),
                    path: None,
                    original: file.original.clone(),
                    content: String::new(),
                }),
                (false, false, true, false) => Some(Change {
                    kind: ChangeKind::Move,
                    original_path: file.original_path.clone(),
                    path: Some(file.path.clone()),
                    original: file.original.clone(),
                    content,
                }),
                (false, false, moved, true) => Some(Change {
                    kind: if moved {
                        ChangeKind::Move
                    } else {
                        ChangeKind::Update
                    },
                    original_path: file.original_path.clone(),
                    path: Some(file.path.clone()),
                    original: file.original.clone(),
                    content,
                }),
                _ => None,
            };
            if let Some(change) = change {
                changes.push(change);
            }
        }
        ChangeSet { changes }
    }

    fn report(&self, changes: &ChangeSet) -> String {
        let mut report = match self.active {
            Some(index) => format!("in {}\n", self.files[index].path),
            None => "no active file\n".to_owned(),
        };
        report.push_str(&format!(
            "{} file change{}\n",
            changes.changes.len(),
            if changes.changes.len() == 1 { "" } else { "s" }
        ));
        for change in &changes.changes {
            match change.kind {
                ChangeKind::Add => report.push_str(&format!(
                    "add {}\n",
                    change.path.as_deref().unwrap_or_default()
                )),
                ChangeKind::Delete => report.push_str(&format!(
                    "delete {}\n",
                    change.original_path.as_deref().unwrap_or_default()
                )),
                ChangeKind::Update => report.push_str(&format!(
                    "update {}\n",
                    change.path.as_deref().unwrap_or_default()
                )),
                ChangeKind::Move => report.push_str(&format!(
                    "move {} -> {}\n",
                    change.original_path.as_deref().unwrap_or_default(),
                    change.path.as_deref().unwrap_or_default()
                )),
            }
        }
        report
    }
}

fn category(operation: &Operation) -> &'static str {
    match operation {
        Operation::In { .. }
        | Operation::New { .. }
        | Operation::Move { .. }
        | Operation::Remove { .. } => "file",
        Operation::Select { .. }
        | Operation::TextSelect { .. }
        | Operation::BlockSelect { .. }
        | Operation::RangeSelect { .. } => "selection",
        Operation::Type { .. }
        | Operation::Delete
        | Operation::Copy
        | Operation::Cut
        | Operation::Paste => "edit",
        Operation::Commit => "state",
    }
}
