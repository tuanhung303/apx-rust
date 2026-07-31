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
