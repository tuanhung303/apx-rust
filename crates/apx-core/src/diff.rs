//! Minimal line diff for edit reports.
//!
//! Computes a longest-common-subsequence line diff of a file's original and
//! final content so the tool report can show, compactly, exactly which lines
//! an edit script changed. Kept deliberately small: no dependencies, no
//! hunk grouping, hard safety cap on the DP table.

/// One changed line, in document order (removed lines precede added lines
/// within a hunk, like a unified diff).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// A line present in the original but not in the new content.
    Removed { old: usize, text: String },
    /// A line present in the new content but not in the original.
    Added { new: usize, text: String },
}

/// Result of a line diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffResult {
    /// Number of removed lines.
    pub removed: usize,
    /// Number of added lines.
    pub added: usize,
    /// Changed lines in document order. Empty when the inputs are too large
    /// for the DP table (counts are still exact).
    pub lines: Vec<DiffLine>,
    /// True when the diff was computed in count-only mode (inputs too large).
    pub truncated: bool,
}

/// Largest DP table (n * m cells) the diff will allocate. Files above this
/// fall back to count-only mode, which is exact for the +/- summary.
const MAX_DP_CELLS: usize = 2_000_000;

/// Line diff of `original` vs `content`.
pub fn diff_lines(original: &str, content: &str) -> DiffResult {
    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = content.lines().collect();
    let (n, m) = (a.len(), b.len());
    if n * m > MAX_DP_CELLS {
        // Count-only fallback: exact totals, no preview lines.
        let removed = a.iter().filter(|line| !b.contains(line)).count();
        let added = b.iter().filter(|line| !a.contains(line)).count();
        return DiffResult {
            removed,
            added,
            lines: Vec::new(),
            truncated: true,
        };
    }
    // dp[i][j] = LCS length of a[i..] and b[j..].
    let mut dp = vec![0usize; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * (m + 1) + j] = if a[i] == b[j] {
                dp[(i + 1) * (m + 1) + j + 1] + 1
            } else {
                dp[(i + 1) * (m + 1) + j].max(dp[i * (m + 1) + j + 1])
            };
        }
    }
    let mut lines = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[(i + 1) * (m + 1) + j] >= dp[i * (m + 1) + j + 1] {
            lines.push(DiffLine::Removed {
                old: i + 1,
                text: a[i].to_owned(),
            });
            i += 1;
        } else {
            lines.push(DiffLine::Added {
                new: j + 1,
                text: b[j].to_owned(),
            });
            j += 1;
        }
    }
    while i < n {
        lines.push(DiffLine::Removed {
            old: i + 1,
            text: a[i].to_owned(),
        });
        i += 1;
    }
    while j < m {
        lines.push(DiffLine::Added {
            new: j + 1,
            text: b[j].to_owned(),
        });
        j += 1;
    }
    let removed = lines
        .iter()
        .filter(|line| matches!(line, DiffLine::Removed { .. }))
        .count();
    let added = lines.len() - removed;
    DiffResult {
        removed,
        added,
        lines,
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_orders_single_line_change() {
        let result = diff_lines("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(result.removed, 1);
        assert_eq!(result.added, 1);
        assert_eq!(
            result.lines,
            vec![
                DiffLine::Removed {
                    old: 2,
                    text: "b".to_owned()
                },
                DiffLine::Added {
                    new: 2,
                    text: "B".to_owned()
                },
            ]
        );
    }

    #[test]
    fn counts_pure_insertion_and_deletion() {
        let inserted = diff_lines("", "x\ny\n");
        assert_eq!((inserted.removed, inserted.added), (0, 2));
        assert_eq!(
            inserted.lines,
            vec![
                DiffLine::Added {
                    new: 1,
                    text: "x".to_owned()
                },
                DiffLine::Added {
                    new: 2,
                    text: "y".to_owned()
                },
            ]
        );
        let deleted = diff_lines("x\ny\n", "");
        assert_eq!((deleted.removed, deleted.added), (2, 0));
        assert_eq!(
            deleted.lines,
            vec![
                DiffLine::Removed {
                    old: 1,
                    text: "x".to_owned()
                },
                DiffLine::Removed {
                    old: 2,
                    text: "y".to_owned()
                },
            ]
        );
    }

    #[test]
    fn identical_inputs_are_empty() {
        let result = diff_lines("one\ntwo\n", "one\ntwo\n");
        assert_eq!((result.removed, result.added), (0, 0));
        assert!(result.lines.is_empty());
        assert!(!result.truncated);
    }
}
