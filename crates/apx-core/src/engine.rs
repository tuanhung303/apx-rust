use crate::diff::{DiffLine, DiffResult, diff_lines};
use crate::editor::{Editor, logical_lines};
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
            if let Err(mut message) =
                self.execute(&instruction.operation, index + 1, instruction.line)
            {
                if let Some(active) = self.active {
                    message = format!("{message}; in {}", self.files[active].path);
                }
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
        if self.paths.contains_key(path) {
            return Err(format!("destination {path} already exists"));
        }
        let freed = self
            .files
            .iter()
            .any(|file| file.deleted && file.path == path && !file.created);
        if !freed && (self.reserved.contains(path) || self.baseline.exists(path)?) {
            return Err(format!(
                "destination {path} already exists; rm it first in this script to replace"
            ));
        }
        Ok(())
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
        let mut report = String::new();
        if changes.changes.is_empty() {
            report.push_str("0 file changes (no net edits; content already matches)\n");
            return report;
        }
        report.push_str(&format!(
            "{} file change{}\n",
            changes.changes.len(),
            if changes.changes.len() == 1 { "" } else { "s" }
        ));
        let mut diffs = Vec::with_capacity(changes.changes.len());
        for change in &changes.changes {
            let diff = if matches!(change.kind, ChangeKind::Add | ChangeKind::Delete) {
                DiffResult::default()
            } else {
                diff_lines(&change.original, &change.content)
            };
            diffs.push(diff);
        }
        for (change, diff) in changes.changes.iter().zip(&diffs) {
            match change.kind {
                ChangeKind::Add => report.push_str(&format!(
                    "add {} ({})\n",
                    change.path.as_deref().unwrap_or_default(),
                    line_label(change.content.lines().count())
                )),
                ChangeKind::Delete => report.push_str(&format!(
                    "delete {} ({})\n",
                    change.original_path.as_deref().unwrap_or_default(),
                    line_label(change.original.lines().count())
                )),
                ChangeKind::Update => report.push_str(&format!(
                    "update {} (+{}/-{})\n",
                    change.path.as_deref().unwrap_or_default(),
                    diff.added,
                    diff.removed
                )),
                ChangeKind::Move => report.push_str(&format!(
                    "move {} -> {} (+{}/-{})\n",
                    change.original_path.as_deref().unwrap_or_default(),
                    change.path.as_deref().unwrap_or_default(),
                    diff.added,
                    diff.removed
                )),
            }
        }
        let mut preview: Vec<String> = Vec::new();
        let mut omitted = 0usize;
        let mut truncated_diff = false;
        let mut preview_bytes = 0usize;
        for (change, diff) in changes.changes.iter().zip(&diffs) {
            if matches!(change.kind, ChangeKind::Add | ChangeKind::Delete) {
                continue;
            }
            truncated_diff |= diff.truncated;
            let rendered = render_preview(diff);
            let mut per_file_lines = 0usize;
            let mut per_file_bytes = 0usize;
            for line in rendered {
                if per_file_lines >= REPORT_PREVIEW_LINES
                    || per_file_bytes >= REPORT_PREVIEW_BYTES
                    || preview_bytes >= REPORT_TOTAL_BYTES
                {
                    omitted += 1;
                    continue;
                }
                per_file_lines += 1;
                per_file_bytes += line.len() + 1;
                preview_bytes += line.len() + 1;
                preview.push(line);
            }
        }
        if !preview.is_empty() || truncated_diff {
            report.push_str("changed lines:\n");
            for line in &preview {
                report.push_str(line);
                report.push('\n');
            }
            if omitted > 0 {
                report.push_str(&format!("... {omitted} more changed lines omitted\n"));
            } else if truncated_diff {
                report
                    .push_str("... line diff truncated (file too large); counts above are exact\n");
            }
        }
        report
    }
}

/// Maximum preview lines per changed file.
const REPORT_PREVIEW_LINES: usize = 60;
/// Maximum length of one previewed line; longer lines are truncated with a marker.
const REPORT_LINE_CAP: usize = 160;
/// Maximum preview bytes per changed file.
const REPORT_PREVIEW_BYTES: usize = 4096;
/// Maximum total report size; the preview is cut first.
const REPORT_TOTAL_BYTES: usize = 8192;

fn line_label(count: usize) -> String {
    format!("{} line{}", count, if count == 1 { "" } else { "s" })
}

fn cap_line(line: &str) -> String {
    let chars = line.chars().count();
    if chars <= REPORT_LINE_CAP {
        return line.to_owned();
    }
    let mut capped: String = line.chars().take(REPORT_LINE_CAP).collect();
    capped.push_str(&format!("... [{} chars total]", chars));
    capped
}

fn line_text(line: &DiffLine) -> String {
    match line {
        DiffLine::Removed { text, .. } | DiffLine::Added { text, .. } => cap_line(text),
    }
}

/// Render a diff's changed lines as `- old` / `+ new` pairs (unified-diff
/// hunk order), so a replaced line's old and new text stay adjacent.
fn render_preview(diff: &DiffResult) -> Vec<String> {
    let mut out = Vec::new();
    let lines = &diff.lines;
    let mut i = 0;
    while i < lines.len() {
        if let DiffLine::Added { .. } = &lines[i] {
            out.push(format!("+ {}", line_text(&lines[i])));
            i += 1;
            continue;
        }
        let mut j = i;
        while j < lines.len() && matches!(lines[j], DiffLine::Removed { .. }) {
            j += 1;
        }
        let mut k = j;
        while k < lines.len() && matches!(lines[k], DiffLine::Added { .. }) {
            k += 1;
        }
        let (removed, added) = (&lines[i..j], &lines[j..k]);
        for (old, new) in removed.iter().zip(added) {
            out.push(format!("- {}", line_text(old)));
            out.push(format!("+ {}", line_text(new)));
        }
        for old in &removed[added.len().min(removed.len())..] {
            out.push(format!("- {}", line_text(old)));
        }
        for new in &added[removed.len().min(added.len())..] {
            out.push(format!("+ {}", line_text(new)));
        }
        i = k;
    }
    out
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

/// Read-only evaluation for `apx peek`: executes only `in` and the selector
/// commands, printing the selected lines with one-based line numbers. Any
/// edit or file-mutation command is rejected; nothing is ever written.
pub fn evaluate_peek<B: Baseline>(baseline: &B, program: &Program) -> Result<String, CommandError> {
    let mut distinct_paths: Vec<&str> = Vec::new();
    for instruction in &program.instructions {
        if let Operation::In { path } = &instruction.operation
            && !distinct_paths.contains(&path.as_str())
        {
            distinct_paths.push(path);
        }
    }
    let headers = distinct_paths.len() > 1;

    struct Active {
        content: String,
        editor: Editor,
    }

    let mut output = String::new();
    let mut active: Option<Active> = None;
    for (index, instruction) in program.instructions.iter().enumerate() {
        let failure = |message: String| CommandError {
            command: index + 1,
            line: instruction.line,
            operation: instruction.operation.name().to_owned(),
            category: category(&instruction.operation).to_owned(),
            message,
        };
        match &instruction.operation {
            Operation::In { path } => {
                let content = baseline
                    .read(path)
                    .map_err(failure)?
                    .ok_or_else(|| failure(format!("{path} does not exist")))?;
                if headers {
                    output.push_str(&format!("==> {path} <==\n"));
                }
                active = Some(Active {
                    editor: Editor::new(content.clone()),
                    content,
                });
            }
            Operation::Select { line, start, end } => {
                let active = active
                    .as_mut()
                    .ok_or_else(|| failure("sel requires an active file".to_owned()))?;
                active
                    .editor
                    .select_columns(*line, *start, *end)
                    .map_err(failure)?;
                render_selection(
                    &mut output,
                    &active.content,
                    &active.editor.selected_spans(),
                );
            }
            Operation::TextSelect { line, text, count } => {
                let active = active
                    .as_mut()
                    .ok_or_else(|| failure("tsel requires an active file".to_owned()))?;
                active
                    .editor
                    .select_matches(*line, text, *count)
                    .map_err(failure)?;
                render_selection(
                    &mut output,
                    &active.content,
                    &active.editor.selected_spans(),
                );
            }
            Operation::BlockSelect { start, end } => {
                let active = active
                    .as_mut()
                    .ok_or_else(|| failure("bsel requires an active file".to_owned()))?;
                active.editor.select_block(start, end).map_err(failure)?;
                render_selection(
                    &mut output,
                    &active.content,
                    &active.editor.selected_spans(),
                );
            }
            Operation::RangeSelect { start, end } => {
                let active = active
                    .as_mut()
                    .ok_or_else(|| failure("rsel requires an active file".to_owned()))?;
                active.editor.select_lines(*start, *end).map_err(failure)?;
                render_selection(
                    &mut output,
                    &active.content,
                    &active.editor.selected_spans(),
                );
            }
            other => {
                return Err(failure(format!(
                    "peek is read-only and supports only in, sel, tsel, bsel, and rsel; got {}",
                    other.name()
                )));
            }
        }
    }
    Ok(output)
}

fn render_selection(output: &mut String, content: &str, spans: &[(usize, usize)]) {
    let lines = logical_lines(content);
    let mut rendered: Vec<usize> = Vec::new();
    for &(start, end) in spans {
        for (index, &(line_start, content_end, full_end)) in lines.iter().enumerate() {
            if line_start < end && full_end > start && !rendered.contains(&index) {
                rendered.push(index);
                output.push_str(&format!(
                    "{:>6}\t{}\n",
                    index + 1,
                    &content[line_start..content_end]
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryBaseline, parse};

    fn evaluate_script(files: &[(&str, &str)], script: &str) -> Evaluation {
        let baseline = MemoryBaseline::new(
            files
                .iter()
                .map(|(path, content)| ((*path).to_owned(), (*content).to_owned())),
        );
        let program = parse(script).expect("script must parse");
        evaluate(&baseline, &program).expect("script must evaluate")
    }

    #[test]
    fn report_shows_update_counts_and_preview() {
        let evaluation = evaluate_script(
            &[("a.txt", "one\ntwo\nthree\n")],
            "in a.txt\ntsel 2 \"two\"\ntype \"TWO\"\n",
        );
        let report = evaluation.report;
        assert!(report.contains("1 file change"), "{report}");
        assert!(report.contains("update a.txt (+1/-1)"), "{report}");
        assert!(report.contains("changed lines:"), "{report}");
        assert!(report.contains("- two"), "{report}");
        assert!(report.contains("+ TWO"), "{report}");
    }

    #[test]
    fn report_labels_add_delete_and_moved_create() {
        let evaluation = evaluate_script(
            &[("a.txt", "hello\n")],
            "new b.txt\ntype <<PATCH\nx\ny\nPATCH\nrm a.txt\nin b.txt\nmv c.txt\n",
        );
        let report = evaluation.report;
        assert!(report.contains("2 file changes"), "{report}");
        assert!(report.contains("add c.txt (2 lines)"), "{report}");
        assert!(report.contains("delete a.txt (1 line)"), "{report}");
    }

    #[test]
    fn report_labels_move_of_existing_file() {
        let evaluation = evaluate_script(&[("a.txt", "hello\n")], "in a.txt\nmv b.txt\n");
        assert!(
            evaluation.report.contains("move a.txt -> b.txt (+0/-0)"),
            "{}",
            evaluation.report
        );
    }

    #[test]
    fn report_noop_script_is_explicit() {
        let evaluation = evaluate_script(
            &[("a.txt", "one\ntwo\n")],
            "in a.txt\ntsel 2 \"two\"\ntype \"two\"\n",
        );
        assert!(
            evaluation
                .report
                .contains("0 file changes (no net edits; content already matches)"),
            "{}",
            evaluation.report
        );
    }

    #[test]
    fn report_truncates_preview_at_cap() {
        let mut original = String::new();
        let mut updated = String::new();
        for i in 1..=70 {
            original.push_str(&format!("line {i}\n"));
            updated.push_str(&format!("LINE {i}\n"));
        }
        let mut script = String::from("in a.txt\nrsel 1:70\ntype <<PATCH\n");
        script.push_str(&updated);
        script.push_str("PATCH\n");
        let evaluation = evaluate_script(&[("a.txt", &original)], &script);
        let report = evaluation.report;
        assert!(report.contains("update a.txt (+70/-70)"), "{report}");
        assert!(report.contains("- line 1"), "{report}");
        assert!(report.contains("+ LINE 1"), "{report}");
        assert!(
            report.contains("... 80 more changed lines omitted"),
            "{report}"
        );
    }

    #[test]
    fn failure_diagnostic_includes_active_file() {
        let baseline = MemoryBaseline::new(vec![("a.txt".to_owned(), "one\ntwo\n".to_owned())]);
        let program = parse("in a.txt\ntsel 9 \"missing\"\n").unwrap();
        let error = evaluate(&baseline, &program).expect_err("tsel must fail");
        assert!(
            error.diagnostic().contains("; in a.txt"),
            "{}",
            error.diagnostic()
        );
    }
}
