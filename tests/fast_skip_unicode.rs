//! Skip fast paths on non-ASCII text behave like the regex they stand for.

use rustine::exec::{execute_tree, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

fn run(gel: &str, input: &str) -> String {
    let mut doc = parse_gel_document(&lex(gel).unwrap()).unwrap();
    doc.compile_regexes();
    serialize_tree(&execute_tree(&doc, "input", input).unwrap(), RuntimeFormat::Json).unwrap()
}

const SKIP_THEN_NEWLINE: &str = "grammar input:\n    skip /SKIP/\n    match /\\n/:\n        out.add('nl')\n";

fn skip_then_newline(skip: &str, input: &str) -> String {
    run(&SKIP_THEN_NEWLINE.replace("SKIP", skip), input)
}

#[test]
fn dot_steps_over_a_whole_character() {
    assert!(skip_then_newline(".", "é\n").contains("\"nl\""));
    assert!(skip_then_newline("[^\\r\\n]", "é\u{1F642}\n").contains("\"nl\""));
}

#[test]
fn dot_does_not_match_a_newline() {
    assert!(skip_then_newline(".", "x\n").contains("\"nl\""));
}

#[test]
fn unicode_word_and_digit_characters() {
    assert!(skip_then_newline("\\w+", "café\n").contains("\"nl\""));
    assert!(skip_then_newline("\\d+", "\u{661}\u{662}\u{663}\n").contains("\"nl\""));
}

#[test]
fn unicode_whitespace() {
    let gel = "grammar input:\n    skip /\\s+/\n    match /x/:\n        out.add('x')\n";
    assert!(run(gel, "\u{a0}x").contains("\"x\""), "no-break space");
    assert!(run(gel, "\u{0B}x").contains("\"x\""), "vertical tab");
    let gel = "grammar input:\n    match /\\x{a0}/:\n        out.add('nbsp')\n    skip /\\S+/\n";
    assert!(
        run(gel, "ab\u{a0}cd").contains("\"nbsp\""),
        "\\S+ stops at a no-break space"
    );
}
