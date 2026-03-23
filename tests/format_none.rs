//! Test that RuntimeFormat::None produces an empty string (Dummy generator).

use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const SYNTAX: &str = r#"
grammar input(default):
    match /(\S+)/ /\s+/ /(\S+)/ /\n/:
        out.add('item?key="$1"', '$3')
    skip /[^\n]*\n/
"#;

const INPUT: &str = "hello world\n";

#[test]
fn format_none_returns_empty_string() {
    let tokens = lex(SYNTAX).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    let exec = execute(&mut doc, "input", INPUT).expect("execute");
    assert!(exec.error.is_none());

    // format=None should produce empty string
    let output = serialize_tree(&exec, RuntimeFormat::None);
    assert!(output.is_empty(), "expected empty string, got: {output:?}");

    // but JSON should produce actual output (sanity check)
    let json = serialize_tree(&exec, RuntimeFormat::Json);
    assert!(!json.is_empty(), "JSON should not be empty");
    assert!(json.contains("item"), "JSON should contain 'item'");
}
