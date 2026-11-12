//! Names starting with digits (e.g. `7600_modules`) are identifiers; an
//! all-digit run like `7600` is a number.

use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

/// A grammar named with a leading digit, called from `input`.
const REPRO_SYNTAX: &str = "grammar default:\n    match /a/:\n        do.skip()\n\ngrammar 7600_modules(default):\n    match /x/:\n        out.add('x')\n\ngrammar input(default):\n    match /y/:\n        7600_modules()\n";

/// Input that drives `input` → `7600_modules` (matches `y` then `x`).
const REPRO_INPUT: &str = "yx";

fn run(syntax: &str, input: &str, format: RuntimeFormat) -> String {
    let tokens = lex(syntax).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    let exec = execute(&mut doc, "input", input).expect("execute");
    assert!(exec.error.is_none(), "execution error: {:?}", exec.error);
    serialize_tree(&exec, format).unwrap()
}

/// Strip trailing whitespace for byte-exact comparison (matches parity_byte_exact).
fn norm(s: &str) -> String {
    s.trim_end().to_string()
}

#[test]
fn repro_grammar_compiles_and_defines_digit_named_grammar() {
    let tokens = lex(REPRO_SYNTAX).expect("lex reproduction grammar");
    let doc = parse_gel_document(&tokens).expect("parse reproduction grammar");
    // All three grammars must be present, including the digit-leading one.
    assert!(doc.grammars.contains_key("default"), "missing 'default' grammar");
    assert!(
        doc.grammars.contains_key("7600_modules"),
        "missing '7600_modules' grammar; grammars: {:?}",
        doc.grammars.keys().collect::<Vec<_>>()
    );
    assert!(doc.grammars.contains_key("input"), "missing 'input' grammar");
    // The digit-leading grammar inherits `default`.
    assert_eq!(
        doc.grammars.get("7600_modules").unwrap().inherit.as_deref(),
        Some("default")
    );
}

#[test]
fn repro_grammar_runtime_output() {
    let json = run(REPRO_SYNTAX, REPRO_INPUT, RuntimeFormat::Json);
    assert_eq!(norm(&json), "{\n    \"x\": {}\n}", "JSON parity mismatch:\n{json}");

    let xml = run(REPRO_SYNTAX, REPRO_INPUT, RuntimeFormat::Xml);
    assert_eq!(norm(&xml), "<xml>\n  <x/>\n</xml>", "XML parity mismatch:\n{xml}");

    // YAML: checked structurally (`x: {}` vs `x:\n{}` whitespace is not pinned here).
    let yaml = run(REPRO_SYNTAX, REPRO_INPUT, RuntimeFormat::Yaml);
    assert!(yaml.contains("x:"), "missing 'x' node in YAML:\n{yaml}");
    assert!(yaml.contains("{}"), "missing empty-node marker in YAML:\n{yaml}");
}

#[test]
fn numeric_literal_argument_still_parses_and_runs() {
    // A bare numeric literal used as a function-call argument must continue to
    // lex as a Number and round-trip to its string form in the output.
    const SYNTAX: &str = "grammar input:\n    match /x/:\n        out.add('count', 42)\n";
    let json = run(SYNTAX, "x", RuntimeFormat::Json);
    assert!(json.contains("\"count\""), "missing 'count' node:\n{json}");
    assert!(json.contains("42"), "missing numeric literal value '42':\n{json}");
}
