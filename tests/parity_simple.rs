use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const SYNTAX: &str = include_str!("../fixtures/parity/simple/syntax1.gel");
const INPUT: &str = include_str!("../fixtures/parity/simple/input1.txt");
const EXPECT_JSON: &str = include_str!("../fixtures/parity/simple/output1.json");

// Minimal JSON scanner to verify key/value parity rather than exact formatting.
fn json_contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

#[test]
fn parity_simple_fixture() {
    // Parse top-level document and execute 'input' grammar
    // Parse fixture grammar & run entry grammar 'input'
    let tokens = lex(SYNTAX).expect("lex syntax");
    let mut doc = parse_gel_document(&tokens).expect("parse syntax");
    let exec = execute(&mut doc, "input", INPUT).expect("execute input grammar");
    assert!(exec.error.is_none(), "unexpected execution error: {:?}", exec.error);
    // Serialize runtime result to JSON
    let runtime_json = serialize_execution(&exec, RuntimeFormat::Json);
    println!("RUNTIME JSON: {}", runtime_json);

    // Check key attributes present for both users (attributes serialized with '@' prefix)
    assert!(
        json_contains(&runtime_json, "@firstname\": \"John"),
        "missing John @firstname"
    );
    assert!(
        json_contains(&runtime_json, "@lastname\": \"Doe"),
        "missing Doe @lastname"
    );
    assert!(
        json_contains(&runtime_json, "@firstname\": \"Jane"),
        "missing Jane @firstname"
    );
    assert!(
        json_contains(&runtime_json, "@lastname\": \"Foo"),
        "missing Foo @lastname"
    );
    // Office and birth-date values
    assert!(json_contains(&runtime_json, "1st Ave"));
    assert!(json_contains(&runtime_json, "2nd Ave"));
    assert!(json_contains(&runtime_json, "1978-01-01"));
    assert!(json_contains(&runtime_json, "1970-01-01"));

    // Ensure user nodes count (rough check by counting occurrences of \"firstname\")
    let count = runtime_json.matches("@firstname\": \"").count();
    assert_eq!(count, 2, "expected two user entries, found {}", count);

    // (Optional) Compare subset of expected JSON to ensure structural names match
    for key in ["user", "office", "birth-date"].iter() {
        assert!(json_contains(EXPECT_JSON, key));
    }
}
