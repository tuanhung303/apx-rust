mod common;

use apx_core::parse;
use common::serialize_program;

const ALPHABET: &[char] = &[
    'a',
    'b',
    'z',
    'A',
    'Z',
    '0',
    '1',
    '9',
    ' ',
    '\t',
    '\r',
    '\n',
    '"',
    '\'',
    '\\',
    '/',
    ':',
    ';',
    ',',
    '.',
    '-',
    '_',
    '{',
    '}',
    '[',
    ']',
    '(',
    ')',
    '=',
    '+',
    '*',
    '&',
    '|',
    '<',
    '>',
    '!',
    '?',
    '~',
    '^',
    '%',
    '#',
    '$',
    '@',
    '`',
    '\u{1}',
    '\u{1f}',
    '\u{7f}',
    '\u{a0}',
    '\u{85}',
    '\u{2028}',
    '\u{fffd}',
    '\u{00e9}',
    '\u{4e2d}',
    '\u{1f600}',
];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn random_string(rng: &mut Lcg, max_len: usize) -> String {
    let len = rng.below(max_len + 1);
    (0..len)
        .map(|_| ALPHABET[rng.below(ALPHABET.len())])
        .collect()
}

fn check_no_panic(script: &str, label: &str, failures: &mut Vec<String>) {
    match parse(script) {
        Ok(program) => {
            let serialized = serialize_program(&program);
            match parse(&serialized) {
                Ok(reparsed) if reparsed == program => {}
                Ok(reparsed) => failures.push(format!(
                    "{label}: accepted script changed after round-trip\n  in:   {script:?}\n  out:  {serialized:?}\n  ast:  {reparsed:?} != {program:?}"
                )),
                Err(errors) => failures.push(format!(
                    "{label}: serialized accepted script rejected: {errors:?}\n  script: {serialized:?}"
                )),
            }
        }
        Err(group) => {
            for error in &group.commands {
                let diagnostic = error.diagnostic();
                if diagnostic.chars().any(|c| c.is_control() && c != '\n')
                    || !diagnostic.ends_with('\n')
                {
                    failures.push(format!(
                        "{label}: diagnostic malformed: {diagnostic:?} (error {error:?})"
                    ));
                }
            }
        }
    }
}

#[test]
fn random_scripts_never_panic_and_accepted_round_trip() {
    let mut rng = Lcg(0x5eed_1234_5678_9abc);
    let mut failures: Vec<String> = Vec::new();
    for index in 0..20_000 {
        let script = random_string(&mut rng, 256);
        check_no_panic(&script, &format!("random#{index}"), &mut failures);
    }
    assert!(
        failures.is_empty(),
        "fuzz failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn corpus_derived_mutations_never_panic() {
    let corpus = common::load_corpus();
    let mut rng = Lcg(0xdec0_11c0_ffee_0001);
    let mut failures: Vec<String> = Vec::new();
    for (case_index, case) in corpus.cases.iter().enumerate() {
        for variant in 0..64 {
            let mut characters: Vec<char> = case.script.chars().collect();
            if characters.is_empty() {
                characters.push('\n');
            }
            let mutation = rng.below(4);
            let position = rng.below(characters.len());
            match mutation {
                0 => {
                    characters[position] = ALPHABET[rng.below(ALPHABET.len())];
                }
                1 => {
                    characters.remove(position);
                }
                2 => {
                    characters.insert(position, ALPHABET[rng.below(ALPHABET.len())]);
                }
                _ => {
                    if characters.len() > 1 {
                        characters.remove(position);
                        characters.insert(position, ALPHABET[rng.below(ALPHABET.len())]);
                    }
                }
            }
            let script: String = characters.into_iter().collect();
            check_no_panic(
                &script,
                &format!("case {}({})#{}", case.name, case_index, variant),
                &mut failures,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "fuzz failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn whole_corpus_never_panics() {
    let corpus = common::load_corpus();
    let mut failures: Vec<String> = Vec::new();
    for case in &corpus.cases {
        check_no_panic(&case.script, &case.name, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "fuzz failures:\n{}",
        failures.join("\n")
    );
}
