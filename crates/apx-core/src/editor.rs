#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub start: usize,
    pub end: usize,
    pub linewise: bool,
}

#[derive(Debug, Clone)]
struct Edit {
    command: usize,
    line: usize,
    operation: &'static str,
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Editor {
    baseline: String,
    cursor: usize,
    selections: Vec<Selection>,
    edits: Vec<Edit>,
}

impl Editor {
    pub fn new(baseline: String) -> Self {
        Self {
            baseline,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
        self.selections.clear();
    }

    pub fn commit(&mut self) {
        self.baseline = self.content();
        self.edits.clear();
        self.reset();
    }

    pub fn has_edits(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn content(&self) -> String {
        let mut edits = self.edits.clone();
        edits.sort_by_key(|edit| (edit.start, edit.end));
        let mut result = String::new();
        let mut cursor = 0;
        for edit in edits {
            result.push_str(&self.baseline[cursor..edit.start]);
            result.push_str(&edit.replacement);
            cursor = cursor.max(edit.end);
        }
        result.push_str(&self.baseline[cursor..]);
        result
    }

    pub fn select_columns(&mut self, line: usize, start: usize, end: usize) -> Result<(), String> {
        let lines = logical_lines(&self.baseline);
        let selected = lines
            .get(line - 1)
            .ok_or_else(|| format!("line {line} is outside the file"))?;
        let content = &self.baseline[selected.0..selected.1];
        let offsets: Vec<usize> = content
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(content.len()))
            .collect();
        if start == 0 || end == 0 || start > offsets.len() || end > offsets.len() {
            return Err(format!("columns {start}:{end} are outside line {line}"));
        }
        self.set_selections(vec![Selection {
            start: selected.0 + offsets[start - 1],
            end: selected.0 + offsets[end],
            linewise: false,
        }])
    }

    pub fn select_matches(&mut self, line: usize, text: &str, count: usize) -> Result<(), String> {
        let lines = logical_lines(&self.baseline);
        let start = lines
            .get(line - 1)
            .map(|item| item.0)
            .ok_or_else(|| format!("line {line} is outside the file"))?;
        let offsets = non_overlapping_offsets(&self.baseline[start..], text, count);
        if offsets.len() != count {
            return Err(format!(
                "found {} of {count} requested matches of {text:?} at or after line {line}",
                offsets.len()
            ));
        }
        self.set_selections(
            offsets
                .into_iter()
                .map(|offset| Selection {
                    start: start + offset,
                    end: start + offset + text.len(),
                    linewise: false,
                })
                .collect(),
        )
    }

    pub fn select_block(&mut self, start: &str, end: &str) -> Result<(), String> {
        let starts: Vec<usize> = self
            .baseline
            .match_indices(start)
            .map(|item| item.0)
            .collect();
        if starts.len() != 1 {
            return Err(format!(
                "start literal {start:?} occurs {} times in the active file baseline; want exactly once",
                starts.len()
            ));
        }
        let start_offset = starts[0];
        let tail = start_offset + start.len();
        let ends: Vec<usize> = self.baseline[tail..]
            .match_indices(end)
            .map(|item| item.0)
            .collect();
        if ends.len() != 1 {
            return Err(format!(
                "end literal {end:?} occurs {} times after start in the active file baseline; want exactly once",
                ends.len()
            ));
        }
        self.set_selections(vec![Selection {
            start: start_offset,
            end: tail + ends[0] + end.len(),
            linewise: false,
        }])
    }

    pub fn select_lines(&mut self, start: usize, end: usize) -> Result<(), String> {
        let lines = logical_lines(&self.baseline);
        if start == 0 || end > lines.len() {
            return Err(format!("line range {start}:{end} is outside the file"));
        }
        self.set_selections(vec![Selection {
            start: lines[start - 1].0,
            end: lines[end - 1].2,
            linewise: true,
        }])
    }

    fn set_selections(&mut self, selections: Vec<Selection>) -> Result<(), String> {
        for selection in &selections {
            for edit in &self.edits {
                if edit.start != edit.end
                    && selection.start.max(edit.start) < selection.end.min(edit.end)
                {
                    return Err(format!(
                        "selection conflicts with edit from command {} (source line {}, operation {:?})",
                        edit.command, edit.line, edit.operation
                    ));
                }
            }
        }
        self.selections = selections;
        Ok(())
    }

    pub fn selected_clipboard(&self) -> Option<(String, bool)> {
        self.selections.first().map(|selected| {
            (
                self.baseline[selected.start..selected.end].to_owned(),
                selected.linewise,
            )
        })
    }

    pub fn type_text(
        &mut self,
        replacement: &str,
        command: usize,
        line: usize,
    ) -> Result<(), String> {
        if self.selections.is_empty() {
            self.record(vec![Edit {
                command,
                line,
                operation: "type",
                start: self.cursor,
                end: self.cursor,
                replacement: replacement.to_owned(),
            }])
        } else {
            let edits = self
                .selections
                .iter()
                .map(|selected| {
                    let mut replacement = replacement.to_owned();
                    if selected.linewise && !ends_with_terminator(&replacement) {
                        replacement.push_str(line_terminator(
                            &self.baseline[selected.start..selected.end],
                        ));
                    }
                    Edit {
                        command,
                        line,
                        operation: "type",
                        start: selected.start,
                        end: selected.end,
                        replacement,
                    }
                })
                .collect();
            self.cursor = self.selections.last().expect("nonempty").end;
            self.selections.clear();
            self.record(edits)
        }
    }

    pub fn delete(&mut self, command: usize, line: usize) -> Result<(), String> {
        if self.selections.is_empty() {
            return Err("del requires a selection".to_owned());
        }
        let edits = self
            .selections
            .iter()
            .map(|selected| Edit {
                command,
                line,
                operation: "del",
                start: selected.start,
                end: selected.end,
                replacement: String::new(),
            })
            .collect();
        self.cursor = self.selections.last().expect("nonempty").start;
        self.selections.clear();
        self.record(edits)
    }

    pub fn paste(
        &mut self,
        text: &str,
        linewise: bool,
        command: usize,
        line: usize,
    ) -> Result<(), String> {
        let positions: Vec<usize> = if self.selections.is_empty() {
            vec![self.cursor]
        } else {
            self.selections
                .iter()
                .map(|selected| selected.end)
                .collect()
        };
        let terminator = line_terminator(&self.baseline).to_owned();
        let edits = positions
            .iter()
            .map(|position| {
                let mut replacement = text.to_owned();
                if linewise {
                    if *position > 0 && !ends_with_terminator(&self.baseline[..*position]) {
                        replacement.insert_str(0, &terminator);
                    }
                    if *position < self.baseline.len()
                        && !starts_with_terminator(&self.baseline[*position..])
                        && !ends_with_terminator(&replacement)
                    {
                        replacement.push_str(&terminator);
                    }
                }
                Edit {
                    command,
                    line,
                    operation: "paste",
                    start: *position,
                    end: *position,
                    replacement,
                }
            })
            .collect();
        self.cursor = *positions.last().expect("nonempty");
        self.selections.clear();
        self.record(edits)
    }

    fn record(&mut self, candidates: Vec<Edit>) -> Result<(), String> {
        let mut pending = self.edits.clone();
        for candidate in candidates {
            if candidate.start == candidate.end && candidate.replacement.is_empty() {
                continue;
            }
            for existing in &pending {
                if conflicts(existing, &candidate) {
                    return Err(format!(
                        "conflicts with edit from command {} (source line {}, operation {:?})",
                        existing.command, existing.line, existing.operation
                    ));
                }
            }
            pending.push(candidate.clone());
            self.edits.push(candidate);
        }
        Ok(())
    }
}

fn conflicts(first: &Edit, second: &Edit) -> bool {
    match (first.start == first.end, second.start == second.end) {
        (true, true) => first.start == second.start,
        (true, false) => first.start > second.start && first.start < second.end,
        (false, true) => second.start > first.start && second.start < first.end,
        (false, false) => first.start.max(second.start) < first.end.min(second.end),
    }
}

fn logical_lines(text: &str) -> Vec<(usize, usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0, 0)];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    for segment in text.split_inclusive('\n') {
        let full_end = start + segment.len();
        let content_end = if segment.ends_with("\r\n") {
            full_end - 2
        } else if segment.ends_with('\n') {
            full_end - 1
        } else {
            full_end
        };
        lines.push((start, content_end, full_end));
        start = full_end;
    }
    if text.ends_with('\n') {
        lines.push((start, start, start));
    }
    lines
}

fn non_overlapping_offsets(text: &str, literal: &str, limit: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while result.len() < limit {
        let Some(relative) = text[cursor..].find(literal) else {
            break;
        };
        let offset = cursor + relative;
        result.push(offset);
        cursor = offset + literal.len();
    }
    result
}

fn line_terminator(text: &str) -> &str {
    if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

fn starts_with_terminator(text: &str) -> bool {
    text.starts_with('\n') || text.starts_with('\r')
}

fn ends_with_terminator(text: &str) -> bool {
    text.ends_with('\n') || text.ends_with('\r')
}
