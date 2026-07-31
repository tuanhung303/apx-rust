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
    assert_eq!(
        output,
        "==> a.txt <==\n     1\tone\n==> b.txt <==\n     1\ttwo\n"
    );
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

#[test]
fn rm_then_new_replaces_a_baseline_file_in_one_script() {
    let files = baseline(&[("events.py", "old content\n")]);
    let program = parse("in events.py\nrm\nnew events.py\ntype \"fresh content\\n\"\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    let kinds: Vec<_> = result
        .changes
        .changes
        .iter()
        .map(|change| change.kind.clone())
        .collect();
    assert_eq!(kinds, vec![ChangeKind::Delete, ChangeKind::Add]);
    assert_eq!(result.changes.changes[1].content, "fresh content\n");
}

#[test]
fn new_still_rejects_an_untouched_baseline_destination() {
    let files = baseline(&[("events.py", "old content\n")]);
    let program = parse("new events.py\ntype \"fresh content\\n\"\n").unwrap();
    let error = evaluate(&files, &program).unwrap_err();
    assert!(error.message.contains("already exists"));
    assert!(error.message.contains("rm it first"));
}

#[test]
fn comments_are_skipped_and_command_numbers_stay_executable() {
    let program = parse(concat!(
        "# step one\n",
        "in a.txt\n",
        "  # indented comment\n",
        "tsel 1 \"x\"\n",
        "type \"y\"\n",
    ))
    .unwrap();
    assert_eq!(program.instructions.len(), 3);
    assert_eq!(program.instructions[1].operation.name(), "tsel");

    let files = baseline(&[("a.txt", "x\n")]);
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "y\n");

    let bad = parse("# c\nin a.txt\ntsel 9 \"miss\"\n# tail\n").unwrap();
    let error = evaluate(&files, &bad).unwrap_err();
    assert_eq!(error.command, 2);
    assert!(error.message.contains("; in a.txt"));
}

#[test]
fn hash_inside_heredoc_and_quotes_is_content_not_comment() {
    let files = baseline(&[("a.txt", "line\n")]);
    let program = parse(concat!(
        "in a.txt\n",
        "rsel 1:1\n",
        "type <<PATCH\n",
        "# not a comment\n",
        "\"quoted\"\n",
        "PATCH\n",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(
        result.changes.changes[0].content,
        "# not a comment\n\"quoted\"\n"
    );

    let files = baseline(&[("b.txt", "#hash\n")]);
    let program = parse("in b.txt\ntsel 1 \"#hash\"\ntype \"#done\"\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "#done\n");
}

#[test]
fn unicode_emoji_and_alt_symbols_edit_roundtrip() {
    let files = baseline(&[(
        "utf8.txt",
        "name: Nguyễn Văn A\nmood: 🐍🔥\nprice: 100© ± 5°\n中文测试\n",
    )]);
    let program = parse(concat!(
        "in utf8.txt\n",
        "tsel 2 \"🐍🔥\"\n",
        "type \"🐍✨\"\n",
        "tsel 3 \"©\"\n",
        "type \"®\"\n",
        "tsel 4 \"中文测试\"\n",
        "type \"日本語\"\n",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(
        result.changes.changes[0].content,
        "name: Nguyễn Văn A\nmood: 🐍✨\nprice: 100® ± 5°\n日本語\n"
    );
    assert!(result.report.contains("- mood: 🐍🔥"), "{}", result.report);
    assert!(result.report.contains("+ mood: 🐍✨"), "{}", result.report);
}

#[test]
fn regex_metacharacters_are_literal_in_selectors() {
    let files = baseline(&[("calc.txt", "cost = arr[0] * 2 + (1|2)\n")]);
    let program = parse("in calc.txt\ntsel 1 \"arr[0] * 2\"\ntype \"arr[0] * 3\"\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(
        result.changes.changes[0].content,
        "cost = arr[0] * 3 + (1|2)\n"
    );

    let files = baseline(&[("re.txt", "m = a^b$c.d*e\n")]);
    let program = parse("in re.txt\ntsel 1 \"a^b$c.d*e\"\ntype \"a^b$c.d*f\"\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "m = a^b$c.d*f\n");
}

#[test]
fn escaped_quotes_and_backslashes_roundtrip() {
    let files = baseline(&[("cfg.txt", "say \"hi\" path=C:\\tmp\\a\n")]);
    let program = parse(concat!(
        "in cfg.txt\n",
        "tsel 1 \"say \\\"hi\\\"\"\n",
        "type \"say \\\"bye\\\"\"\n",
        "tsel 1 \"C:\\\\tmp\\\\a\"\n",
        "type \"C:\\\\tmp\\\\b\"\n",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(
        result.changes.changes[0].content,
        "say \"bye\" path=C:\\tmp\\b\n"
    );
}

#[test]
fn json_unicode_escapes_in_selectors_decode() {
    let files = baseline(&[("cafe.txt", "café\n")]);
    let program = parse("in cafe.txt\ntsel 1 \"caf\\u00e9\"\ntype \"caf\\u00e8\"\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "cafè\n");
}

#[test]
fn heredoc_close_requires_exact_delimiter_line() {
    let files = baseline(&[("a.txt", "old\n")]);
    let program = parse(concat!(
        "in a.txt\n",
        "rsel 1:1\n",
        "type <<PATCH\n",
        "PATCH not the close\n",
        "PATCHX\n",
        " PATCH\n",
        "PATCH\n",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(
        result.changes.changes[0].content,
        "PATCH not the close\nPATCHX\n PATCH\n"
    );
}

#[test]
fn crlf_script_line_endings_parse() {
    let files = baseline(&[("a.txt", "A\n")]);
    let program = parse("in a.txt\r\nrsel 1:1\r\ntype \"B\"\r\n").unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "B\n");
}

#[test]
fn comment_only_script_is_a_clean_noop() {
    let files = baseline(&[("a.txt", "x\n")]);
    let program = parse("# nothing to do\n# still nothing\n").unwrap();
    assert!(program.instructions.is_empty());
    let result = evaluate(&files, &program).unwrap();
    assert!(result.changes.changes.is_empty());
    assert!(
        result
            .report
            .contains("0 file changes (no net edits; content already matches)")
    );
}

#[test]
fn bsel_anchors_with_escaped_quotes_and_unicode() {
    let files = baseline(&[("a.txt", "start \"x\"\nmiddle\nend 🎯\n")]);
    let program = parse(concat!(
        "in a.txt\n",
        "bsel \"start \\\"x\\\"\" \"end 🎯\"\n",
        "type \"REPLACED\"",
    ))
    .unwrap();
    let result = evaluate(&files, &program).unwrap();
    assert_eq!(result.changes.changes[0].content, "REPLACED\n");
}

#[test]
fn range_errors_report_the_real_file_size() {
    let files = baseline(&[("a.txt", "one\ntwo\nthree\n")]);
    let rsel = parse("in a.txt\nrsel 2:9\n").unwrap();
    let error = evaluate(&files, &rsel).unwrap_err();
    assert!(error.message.contains("line range 2:9 is outside the file"));
    assert!(
        error.message.contains("file has 3 lines"),
        "{}",
        error.message
    );

    let tsel = parse("in a.txt\ntsel 9 \"two\"\n").unwrap();
    let error = evaluate(&files, &tsel).unwrap_err();
    assert!(error.message.contains("line 9 is outside the file"));
    assert!(
        error.message.contains("file has 3 lines"),
        "{}",
        error.message
    );

    let sel = parse("in a.txt\nsel 1 1:99\n").unwrap();
    let error = evaluate(&files, &sel).unwrap_err();
    assert!(error.message.contains("columns 1:99 are outside line 1"));
    assert!(
        error.message.contains("line has 3 characters"),
        "{}",
        error.message
    );

    let miss = parse("in a.txt\ntsel 1 \"zzz\"\n").unwrap();
    let error = evaluate(&files, &miss).unwrap_err();
    assert!(error.message.contains("found 0 of 1 requested matches"));
    assert!(
        error.message.contains("file has 3 lines"),
        "{}",
        error.message
    );
}
