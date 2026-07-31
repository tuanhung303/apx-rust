mod common;

use apx_core::parse;
use common::serialize_program;

#[test]
fn accepted_corpus_scripts_round_trip() {
    let corpus = common::load_corpus();
    let mut failures: Vec<String> = Vec::new();
    for case in corpus
        .cases
        .iter()
        .filter(|case| case.expected.errors.is_empty())
    {
        let program = match parse(&case.script) {
            Ok(program) => program,
            Err(errors) => {
                failures.push(format!(
                    "{}: Go accepts but Rust rejected before round-trip: {}",
                    case.name, errors
                ));
                continue;
            }
        };
        let serialized = serialize_program(&program);
        let reparsed = match parse(&serialized) {
            Ok(program) => program,
            Err(errors) => {
                failures.push(format!(
                    "{}: serialized script rejected: {errors:?}\n  script: {:?}",
                    case.name, serialized
                ));
                continue;
            }
        };
        if reparsed != program {
            failures.push(format!(
                "{}: AST changed after round-trip\n  original:   {program:?}\n  round-trip: {reparsed:?}\n  script: {:?}",
                case.name, serialized
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "round-trip failures:\n{}",
        failures.join("\n")
    );
}
