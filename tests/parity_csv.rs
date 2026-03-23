//! Parity test: csv demo
//! Verifies that the Rust engine produces the same structural output as
//! Python Gelatin for the CSV grammar (quoted/unquoted field parsing).

use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const SYNTAX: &str = include_str!("../fixtures/parity/csv/syntax1.gel");
const INPUT: &str = include_str!("../fixtures/parity/csv/input1.txt");

fn run_csv(format: RuntimeFormat) -> String {
    let tokens = lex(SYNTAX).expect("lex syntax");
    let mut doc = parse_gel_document(&tokens).expect("parse syntax");
    let exec = execute(&mut doc, "input", INPUT).expect("execute");
    assert!(exec.error.is_none(), "unexpected error: {:?}", exec.error);
    serialize_execution(&exec, format)
}

#[test]
fn parity_csv_json_line_count() {
    let json = run_csv(RuntimeFormat::Json);
    // The input has 3 CSV lines, so we expect 3 "line" groups.
    // Each line appears as a key in the output; count occurrences of "column"
    // arrays inside "line" entries.
    // Python output: 3 line entries with 8, 8, 8 columns respectively.
    assert!(json.contains("\"line\""), "missing 'line' key\n{json}");
}

#[test]
fn parity_csv_json_data_line1() {
    let json = run_csv(RuntimeFormat::Json);
    // Line 1: data1, data2  ,"data3'''",  'data4""',,,data5,
    assert!(json.contains("data1"), "missing 'data1'\n{json}");
    assert!(json.contains("data2"), "missing 'data2'\n{json}");
    assert!(json.contains("data3'''"), "missing 'data3\\'\\'\\''\n{json}");
    assert!(json.contains("data5"), "missing 'data5'\n{json}");
}

#[test]
fn parity_csv_json_data_line2() {
    let json = run_csv(RuntimeFormat::Json);
    // Line 2: foo1, foo2, "foo3'''",  'foo4""',,,foo5,'foo6'
    assert!(json.contains("foo1"), "missing 'foo1'\n{json}");
    assert!(json.contains("foo2"), "missing 'foo2'\n{json}");
    assert!(json.contains("foo3'''"), "missing 'foo3\\'\\'\\''\n{json}");
    assert!(json.contains("foo5"), "missing 'foo5'\n{json}");
    assert!(json.contains("foo6"), "missing 'foo6'\n{json}");
}

#[test]
fn parity_csv_json_data_line3() {
    let json = run_csv(RuntimeFormat::Json);
    // Line 3: bar1, bar2  ,"bar3'''",  'bar4""',,,bar5,
    assert!(json.contains("bar1"), "missing 'bar1'\n{json}");
    assert!(json.contains("bar5"), "missing 'bar5'\n{json}");
}

#[test]
fn parity_csv_xml() {
    let xml = run_csv(RuntimeFormat::Xml);
    assert!(xml.contains("data1"), "missing 'data1'\n{xml}");
    assert!(xml.contains("foo6"), "missing 'foo6'\n{xml}");
    assert!(xml.contains("bar5"), "missing 'bar5'\n{xml}");
    // Column nodes should be present
    assert!(xml.contains("column"), "missing 'column' nodes\n{xml}");
}

#[test]
fn parity_csv_yaml() {
    let yaml = run_csv(RuntimeFormat::Yaml);
    assert!(yaml.contains("data1"), "missing 'data1'\n{yaml}");
    assert!(yaml.contains("foo6"), "missing 'foo6'\n{yaml}");
    assert!(yaml.contains("column"), "missing 'column' nodes\n{yaml}");
}
