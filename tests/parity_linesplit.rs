//! Parity test: linesplit demo
//! Verifies that the Rust engine produces the same structural output as
//! Python Gelatin for the linesplit grammar (simple line-by-line splitting).

use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const SYNTAX: &str = include_str!("../fixtures/parity/linesplit/syntax1.gel");
const INPUT: &str = include_str!("../fixtures/parity/linesplit/input1.txt");

fn run_linesplit(format: RuntimeFormat) -> String {
    let tokens = lex(SYNTAX).expect("lex syntax");
    let mut doc = parse_gel_document(&tokens).expect("parse syntax");
    let exec = execute(&mut doc, "input", INPUT).expect("execute");
    assert!(exec.error.is_none(), "unexpected error: {:?}", exec.error);
    serialize_execution(&exec, format)
}

#[test]
fn parity_linesplit_json_line_count() {
    let json = run_linesplit(RuntimeFormat::Json);
    // The input has 10 lines (including 2 blank lines).
    // Python output has 10 "line" entries — 8 with #text, 2 empty {}.
    // Rust output has 10 "line" entries — each with #text (empty string for blank lines).
    // Count the "line" entries by counting occurrences of the "line" key in JSON.
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| panic!("valid JSON: {e}\n{json}"));
    let output = parsed
        .get("output")
        .unwrap_or_else(|| panic!("missing 'output' key\n{json}"));
    let lines = output
        .get("line")
        .unwrap_or_else(|| panic!("missing 'line' key\n{json}"));
    let lines = lines
        .as_array()
        .unwrap_or_else(|| panic!("line is not array: {lines}\nfull: {json}"));
    assert_eq!(lines.len(), 10, "expected 10 lines, got {}\n{}", lines.len(), json);
    // 8 non-empty #text values
    let non_empty = lines
        .iter()
        .filter(|l| l.get("#text").and_then(|t| t.as_str()).is_some_and(|s| !s.is_empty()))
        .count();
    assert_eq!(non_empty, 8, "expected 8 non-empty lines, got {non_empty}\n{json}");
}

#[test]
fn parity_linesplit_json_content() {
    let json = run_linesplit(RuntimeFormat::Json);
    // Verify specific content values from the Python expected output
    assert!(json.contains("a nice line"), "missing 'a nice line'\n{json}");
    assert!(json.contains("another & line"), "missing 'another & line'\n{json}");
    assert!(json.contains("something else"), "missing 'something else'\n{json}");
    assert!(json.contains("my name is foo"), "missing 'my name is foo'\n{json}");
    assert!(json.contains("your name is bar"), "missing 'your name is bar'\n{json}");
    assert!(json.contains("yet a nice line"), "missing 'yet a nice line'\n{json}");
    assert!(json.contains("yet another line"), "missing 'yet another line'\n{json}");
    assert!(
        json.contains("yet something else"),
        "missing 'yet something else'\n{json}"
    );
}

#[test]
fn parity_linesplit_json_structure() {
    let json = run_linesplit(RuntimeFormat::Json);
    // The output node must have "line" children (array) within "output"
    assert!(json.contains("\"line\""), "missing 'line' key\n{json}");
}

#[test]
fn parity_linesplit_xml() {
    let xml = run_linesplit(RuntimeFormat::Xml);
    // Verify XML contains the expected node structure
    assert!(xml.contains("a nice line"), "missing 'a nice line'\n{xml}");
    assert!(xml.contains("another &amp; line"), "missing escaped ampersand\n{xml}");
    assert!(
        xml.contains("yet something else"),
        "missing 'yet something else'\n{xml}"
    );
}

#[test]
fn parity_linesplit_yaml() {
    let yaml = run_linesplit(RuntimeFormat::Yaml);
    assert!(yaml.contains("a nice line"), "missing 'a nice line'\n{yaml}");
    assert!(
        yaml.contains("yet something else"),
        "missing 'yet something else'\n{yaml}"
    );
}
