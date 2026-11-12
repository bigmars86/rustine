//! Byte offsets used while matching (search window, back-references) must stay on UTF-8 char boundaries.

use rustine::exec::{execute, serialize_tree, ExecutionResult, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

/// 6-byte lines: byte 1000 of the input falls inside the 2-byte `ę`.
fn unicode_lines() -> String {
    "aaaę\n".repeat(200)
}

fn run(grammar: &str, input: &str) -> ExecutionResult {
    let tokens = lex(grammar).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    execute(&mut doc, "input", input).expect("execute")
}

#[test]
fn combined_regex_window_on_multibyte_boundary() {
    let input = unicode_lines();
    assert!(!input.is_char_boundary(1000));
    let grammar = "grammar input:\n    match /[^\\n]+/ /\\n/:\n        out.create('line', '$0')\n";
    let xml = serialize_tree(&run(grammar, &input), RuntimeFormat::Xml).unwrap();
    assert_eq!(xml.matches("<line>aaaę</line>").count(), 200, "{xml}");
}

#[test]
fn single_regex_window_on_multibyte_boundary() {
    // The back-reference disables the combined regex, so each expression is matched on its own.
    let grammar = "grammar input:\n    match /a/ $0 /[^\\n]*/ /\\n/:\n        out.create('line', '$2')\n";
    let xml = serialize_tree(&run(grammar, &unicode_lines()), RuntimeFormat::Xml).unwrap();
    assert_eq!(xml.matches("<line>aę</line>").count(), 200, "{xml}");
}

#[test]
fn skip_window_on_multibyte_boundary() {
    let input = unicode_lines();
    let result = run("grammar input:\n    skip /[a-zę]+\\n/\n", &input);
    assert_eq!(result.consumed, input.len());
}

#[test]
fn back_reference_against_multibyte_char() {
    let grammar = "grammar input:\n    match /a(b)/ $1:\n        out.create('pair', '$1')\n";
    let xml = serialize_tree(&run(grammar, "abé"), RuntimeFormat::Xml).unwrap();
    assert!(
        !xml.contains("<pair>"),
        "back-reference must not match half a char: {xml}"
    );
    let xml = serialize_tree(&run(grammar, "abb"), RuntimeFormat::Xml).unwrap();
    assert!(xml.contains("<pair>b</pair>"), "{xml}");
}
