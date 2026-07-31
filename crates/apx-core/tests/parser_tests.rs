use apx_core::{Operation, parse};

#[test]
fn parses_complete_command_grammar() {
    let script = concat!(
        "in source.rs\n",
        "sel 1 1:2\n",
        "tsel 2 \"old\" 2\n",
        "bsel \"start\" \"end\"\n",
        "rsel 2:4\n",
        "type \"new\\ntext\"\n",
        "del\ncopy\ncut\npaste\ncommit\n",
        "mv moved.rs\nrm\nnew fresh.rs\n",
        "type <<'BODY'\r\n",
        "one\r\n",
        "two\n",
        "BODY\r\n",
    );
    let program = parse(script).expect("complete grammar parses");
    assert_eq!(program.instructions.len(), 15);
    assert!(matches!(
        &program.instructions[1].operation,
        Operation::Select {
            line: 1,
            start: 1,
            end: 2
        }
    ));
    assert!(matches!(
        &program.instructions[14].operation,
        Operation::Type { text } if text == "one\r\ntwo\n"
    ));
}

#[test]
fn aggregates_syntax_errors_by_physical_command() {
    let error = parse("tsel 0 \"x\"\n\nbsel \"\" \"x\"\ntype <<!\n").unwrap_err();
    assert_eq!(error.commands.len(), 3);
    assert_eq!(error.commands[0].command, 1);
    assert_eq!(error.commands[1].line, 3);
    assert!(
        error.commands[2]
            .message
            .contains("invalid heredoc delimiter")
    );
}

#[test]
fn preserves_crlf_heredoc_and_rejects_physical_quoted_newline() {
    let program = parse("type <<PATCH\r\na\r\nb\nPATCH\r\n").unwrap();
    assert!(matches!(
        &program.instructions[0].operation,
        Operation::Type { text } if text == "a\r\nb\n"
    ));
    let error = parse("type \"a\nb\"\n").unwrap_err();
    assert!(error.commands[0].message.contains("physical newline"));
}

#[test]
fn rejects_ambiguous_or_invalid_selector_syntax() {
    for (script, expected) in [
        ("tsel 1 \"\" 1\n", "must not be empty"),
        ("tsel 1 \"x\" 0\n", "invalid tsel count"),
        ("bsel \"x\" \"x\"\n", "must differ"),
        ("rsel 3:1\n", "start exceeds end"),
        ("sel 1 4:2\n", "start exceeds end"),
    ] {
        let error = parse(script).unwrap_err();
        assert!(
            error.commands[0].message.contains(expected),
            "{script:?}: {}",
            error.commands[0].message
        );
    }
}
