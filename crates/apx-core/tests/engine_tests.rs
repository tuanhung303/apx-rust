use apx_core::{ChangeKind, MemoryBaseline, evaluate, parse, translate_apply_patch};

fn baseline(files: &[(&str, &str)]) -> MemoryBaseline {
    MemoryBaseline::new(
        files
            .iter()
            .map(|(path, content)| ((*path).to_owned(), (*content).to_owned())),
    )
}

#[test]
fn edits_use_immutable_baseline_and_reject_overlap() {
    let files = baseline(&[("file.txt", "one two two\nthree\n")]);
    let program =
        parse("in file.txt\ntsel 1 \"two\"\ntype \"A\"\nsel 1 5:7\ntype \"B\"\n").unwrap();
    let error = evaluate(&files, &program).unwrap_err();
    assert_eq!(error.command, 4);
    assert!(error.message.contains("conflicts with edit"));
}

#[test]
fn commit_advances_generation_and_allows_introduced_text() {
    let files = baseline(&[("file.txt", "old\n")]);
    let program = parse(
        "in file.txt\ntsel 1 \"old\"\ntype \"new\"\ncommit\ntsel 1 \"new\"\ntype \"final\"\n",
    )
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "final\n");
}

#[test]
fn copy_cut_and_linewise_paste_match_oracle_shape() {
    let files = baseline(&[("source.txt", "a\nb\n"), ("target.txt", "x\n")]);
    let program = parse("in source.txt\nrsel 1:1\ncopy\nin target.txt\nrsel 1:1\npaste\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "x\na\n");
}

#[test]
fn produces_typed_add_update_delete_and_move_changes() {
    let files = baseline(&[
        ("update.txt", "old\n"),
        ("delete.txt", "gone\n"),
        ("move.txt", "stay\n"),
    ]);
    let program = parse(concat!(
        "in update.txt\ntsel 1 \"old\"\ntype \"new\"\n",
        "rm delete.txt\n",
        "in move.txt\nmv moved.txt\n",
        "new add.txt\ntype \"added\\n\"\n",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    let kinds: Vec<_> = result
        .changes
        .changes
        .iter()
        .map(|change| change.kind.clone())
        .collect();
    assert_eq!(
        kinds,
        vec![
            ChangeKind::Update,
            ChangeKind::Delete,
            ChangeKind::Move,
            ChangeKind::Add
        ]
    );
}

#[test]
fn translation_is_deterministic_and_normalizes_line_endings() {
    let files = baseline(&[("old.txt", "old\r\n")]);
    let program = parse(
        "in old.txt\ntsel 1 \"old\"\ntype \"new\"\nmv moved.txt\nnew add.txt\ntype \"x\\r\\n\"\n",
    )
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    let first = translate_apply_patch(&result.changes).unwrap();
    let second = translate_apply_patch(&result.changes).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("*** Move to: moved.txt\n"));
    assert!(first.contains("*** Add File: add.txt\n+x\n"));
    assert!(!first.contains('\r'));
}

#[test]
fn translation_rejects_empty_or_unterminated_additions() {
    for text in ["", "body"] {
        let files = baseline(&[]);
        let escaped = serde_json::to_string(text).unwrap();
        let program = parse(&format!("new file.txt\ntype {escaped}\n")).unwrap();
        let result = evaluate(&files, &program).unwrap();
        assert!(translate_apply_patch(&result.changes).is_err());
    }
}

#[test]
fn peek_prints_selected_lines_with_one_based_numbers() {
    let files = baseline(&[("file.txt", "alpha\nbeta\ngamma\ndelta\n")]);
    let program = parse("in file.txt\nrsel 2:3\n").unwrap();
    let output = apx_core::evaluate_peek(&files, &program).unwrap();
    assert_eq!(output, "     2\tbeta\n     3\tgamma\n");
}

#[test]
fn peek_tsel_prints_each_match_line_once() {
    let files = baseline(&[("file.txt", "two\nx two\nother\n")]);
    let program = parse("in file.txt\ntsel 1 \"two\" 2\n").unwrap();
    let output = apx_core::evaluate_peek(&files, &program).unwrap();
    assert_eq!(output, "     1\ttwo\n     2\tx two\n");
}

#[test]
fn peek_bsel_and_sel_cover_block_and_column_spans() {
    let files = baseline(&[("file.txt", "head\nstart mid end\ntail\n")]);
    let program = parse("in file.txt\nbsel \"start\" \"end\"\nsel 3 1:4\n").unwrap();
    let output = apx_core::evaluate_peek(&files, &program).unwrap();
    assert_eq!(output, "     2\tstart mid end\n     3\ttail\n");
}

#[test]
fn peek_marks_multiple_files_with_headers() {
    let files = baseline(&[("a.txt", "one\n"), ("b.txt", "two\n")]);
    let program = parse("in a.txt\nrsel 1:1\nin b.txt\nrsel 1:1\n").unwrap();
    let output = apx_core::evaluate_peek(&files, &program).unwrap();
    assert_eq!(output, "==> a.txt <==\n     1\tone\n==> b.txt <==\n     1\ttwo\n");
}

#[test]
fn peek_rejects_edit_commands_and_missing_files() {
    let files = baseline(&[("file.txt", "one\n")]);
    let program = parse("in file.txt\nrsel 1:1\ntype \"x\"\n").unwrap();
    let error = apx_core::evaluate_peek(&files, &program).unwrap_err();
    assert_eq!(error.command, 3);
    assert!(error.message.contains("read-only"));

    let program = parse("in missing.txt\nrsel 1:1\n").unwrap();
    let error = apx_core::evaluate_peek(&files, &program).unwrap_err();
    assert!(error.message.contains("does not exist"));
}
