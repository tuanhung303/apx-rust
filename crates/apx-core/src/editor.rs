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
        let visible = visible_line_count(&self.baseline, &lines);
        let selected = lines.get(line - 1).ok_or_else(|| {
            format!(
                "line {line} is outside the file; file has {} lines",
                visible
            )
        })?;
        let content = &self.baseline[selected.0..selected.1];
        let offsets: Vec<usize> = content
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(content.len()))
            .collect();
        if start == 0 || end == 0 || start > offsets.len() || end > offsets.len() {
            return Err(format!(
                "columns {start}:{end} are outside line {line} (line has {} characters)",
                offsets.len() - 1
            ));
        }
        self.set_selections(vec![Selection {
            start: selected.0 + offsets[start - 1],
            end: selected.0 + offsets[end],
            linewise: false,
        }])
    }

    pub fn select_matches(&mut self, line: usize, text: &str, count: usize) -> Result<(), String> {
        let lines = logical_lines(&self.baseline);
        let visible = visible_line_count(&self.baseline, &lines);
        let start = lines.get(line - 1).map(|item| item.0).ok_or_else(|| {
            format!(
                "line {line} is outside the file; file has {} lines",
                visible
            )
        })?;
        let offsets = non_overlapping_offsets(&self.baseline[start..], text, count);
        if offsets.len() != count {
            let from = match offsets.last() {
                Some(last) => lines.partition_point(|item| item.0 < start + last + text.len()),
                None => line - 1,
            };
            let hint = closest_line_hint(&lines, &self.baseline, text, from, lines.len())
                .or_else(|| {
                    if offsets.is_empty() && line > 1 {
                        closest_line_hint(&lines, &self.baseline, text, 0, line - 1)
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            return Err(format!(
                "found {} of {count} requested matches of {text:?} at or after line {line}; file has {} lines{hint}",
                offsets.len(),
                visible
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
            let hint = if starts.is_empty() {
                let lines = logical_lines(&self.baseline);
                closest_line_hint(&lines, &self.baseline, start, 0, lines.len())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return Err(format!(
                "start literal {start:?} occurs {} times in the active file baseline; want exactly once{hint}",
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
            let hint = if ends.is_empty() {
                let lines = logical_lines(&self.baseline);
                let from = lines.partition_point(|item| item.2 <= tail);
                closest_line_hint(&lines, &self.baseline, end, from, lines.len())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return Err(format!(
                "end literal {end:?} occurs {} times after start in the active file baseline; want exactly once{hint}",
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
        let visible = visible_line_count(&self.baseline, &lines);
        if start == 0 || end > lines.len() {
            return Err(format!(
                "line range {start}:{end} is outside the file; file has {} lines",
                visible
            ));
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

    pub fn selected_spans(&self) -> Vec<(usize, usize)> {
        self.selections
            .iter()
            .map(|selection| (selection.start, selection.end))
            .collect()
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

pub(crate) fn logical_lines(text: &str) -> Vec<(usize, usize, usize)> {
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
/// Count of lines the agent can see when peeking: logical lines minus the
/// synthetic trailing empty segment after a final newline (empty file = 0).
pub(crate) fn visible_line_count(text: &str, lines: &[(usize, usize, usize)]) -> usize {
    if text.is_empty() {
        return 0;
    }
    lines.len() - usize::from(text.ends_with('\n'))
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

/// Maximum candidate lines scanned per closest-match hint search.
const HINT_CANDIDATE_LINES: usize = 400;
/// Minimum bigram similarity for a hint to be shown; below this it is noise.
const HINT_MIN_SIMILARITY: f64 = 0.5;
/// Maximum snippet characters quoted in a hint.
const HINT_SNIPPET_CHARS: usize = 80;

/// Best fuzzy candidate line for a missed anchor, as a message suffix like
/// `; closest line K: "snippet"`. Compares whole logical lines in
/// `lines[from..to]` (at most HINT_CANDIDATE_LINES of them) against the needle
/// using a character-bigram Dice ratio, so near-miss anchors and
/// substring-shaped anchors both surface; unrelated files yield no hint.
fn closest_line_hint(
    lines: &[(usize, usize, usize)],
    baseline: &str,
    needle: &str,
    from: usize,
    to: usize,
) -> Option<String> {
    if needle.chars().count() < 2 {
        return None;
    }
    let to = to.min(from.saturating_add(HINT_CANDIDATE_LINES)).min(lines.len());
    let mut best: Option<(f64, usize)> = None;
    for index in from..to {
        let text = &baseline[lines[index].0..lines[index].1];
        if text.is_empty() {
            continue;
        }
        let score = bigram_similarity(needle, text);
        if score >= HINT_MIN_SIMILARITY && best.is_none_or(|(current, _)| score > current) {
            best = Some((score, index));
        }
    }
    let (_, index) = best?;
    let text = &baseline[lines[index].0..lines[index].1];
    let mut snippet: String = text.chars().take(HINT_SNIPPET_CHARS).collect();
    if text.chars().count() > HINT_SNIPPET_CHARS {
        snippet.push('…');
    }
    Some(format!("; closest line {}: {snippet:?}", index + 1))
}

/// Character-bigram Dice similarity in [0, 1]; 1.0 for identical strings.
/// Unicode-safe (operates on chars) and O(n) in the compared lengths.
fn bigram_similarity(needle: &str, line: &str) -> f64 {
    let needle_chars: Vec<char> = needle.chars().take(HINT_SNIPPET_CHARS + 1).collect();
    let line_chars: Vec<char> = line.chars().take(HINT_SNIPPET_CHARS + 1).collect();
    if needle_chars.len() < 2 || line_chars.len() < 2 {
        return usize::from(needle_chars == line_chars) as f64;
    }
    let mut counts: std::collections::HashMap<(char, char), usize> =
        std::collections::HashMap::new();
    for pair in needle_chars.windows(2) {
        *counts.entry((pair[0], pair[1])).or_default() += 1;
    }
    let mut shared = 0usize;
    for pair in line_chars.windows(2) {
        if let Some(count) = counts.get_mut(&(pair[0], pair[1])) {
            if *count > 0 {
                *count -= 1;
                shared += 1;
            }
        }
    }
    2.0 * shared as f64 / (needle_chars.len() + line_chars.len() - 2) as f64
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsel_miss_reports_closest_line() {
        let mut editor = Editor::new("fn handleRequst(ctx) {\n}\n".to_owned());
        let error = editor
            .select_matches(1, "handleRequest", 1)
            .expect_err("tsel must miss");
        assert!(
            error.contains("found 0 of 1 requested matches"),
            "{error}"
        );
        assert!(
            error.contains("; closest line 1: \"fn handleRequst(ctx) {\""),
            "{error}"
        );
    }

    #[test]
    fn tsel_far_apart_text_yields_no_hint() {
        let mut editor = Editor::new("alpha\nbeta\ngamma\n".to_owned());
        let error = editor
            .select_matches(1, "zzqqxxyy nonsense", 1)
            .expect_err("tsel must miss");
        assert!(!error.contains("closest line"), "{error}");
    }

    #[test]
    fn tsel_unicode_emoji_lines_still_hint() {
        let mut editor = Editor::new("let emoji = \"\u{1F680}\u{1F389} launch\";\nlet other = 1;\n".to_owned());
        let error = editor
            .select_matches(1, "let emoji = \"\u{1F680} launch\";", 1)
            .expect_err("tsel must miss");
        assert!(
            error.contains("; closest line 1: \"let emoji = \\\"\u{1F680}\u{1F389} launch\\\";\""),
            "{error}"
        );
    }

    #[test]
    fn tsel_partial_match_hints_at_remainder() {
        let mut editor = Editor::new("alpha beta\nalpha bet\n".to_owned());
        let error = editor
            .select_matches(1, "alpha beta", 2)
            .expect_err("tsel must miss");
        assert!(
            error.contains("found 1 of 2 requested matches"),
            "{error}"
        );
        assert!(error.contains("; closest line 2: \"alpha bet\""), "{error}");
    }

    #[test]
    fn bsel_missing_start_reports_closest_line() {
        let mut editor = Editor::new("fn main() -> Result<()> {\n}\n".to_owned());
        let error = editor
            .select_block("fn main() {", "}")
            .expect_err("bsel must miss");
        assert!(error.contains("occurs 0 times"), "{error}");
        assert!(
            error.contains("; closest line 1: \"fn main() -> Result<()> {\""),
            "{error}"
        );
    }

    #[test]
    fn bsel_missing_end_reports_closest_line_after_start() {
        let mut editor = Editor::new("START\nkeep\ngoin\n".to_owned());
        let error = editor
            .select_block("START", "going")
            .expect_err("bsel must miss");
        assert!(error.contains("occurs 0 times"), "{error}");
        assert!(error.contains("; closest line 3: \"goin\""), "{error}");
    }
}
