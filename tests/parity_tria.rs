//! Parity test: tria (triathlon) demo
//! Verifies that the Rust engine produces the same structural output as
//! Python Gelatin for the tria grammar (complex multi-level grammar with
//! inheritance, grammar cross-calls, alternation, out.open, out.add,
//! out.add_attribute, out.set_root_name, do.return, skip).

use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const SYNTAX: &str = include_str!("../fixtures/parity/tria/syntax1.gel");
const INPUT: &str = include_str!("../fixtures/parity/tria/input1.txt");

fn run_tria(format: RuntimeFormat) -> String {
    let tokens = lex(SYNTAX).expect("lex syntax");
    let mut doc = parse_gel_document(&tokens).expect("parse syntax");
    let exec = execute(&mut doc, "input", INPUT).expect("execute");
    assert!(exec.error.is_none(), "unexpected error: {:?}", exec.error);
    serialize_execution(&exec, format)
}

// --- JSON tests ---

#[test]
fn parity_tria_json_week_count() {
    let json = run_tria(RuntimeFormat::Json);
    // Two weeks in the input
    let count = json.matches("@number").count();
    assert_eq!(count, 2, "expected 2 weeks, got {count}\n{json}");
}

#[test]
fn parity_tria_json_week_numbers() {
    let json = run_tria(RuntimeFormat::Json);
    assert!(
        json.contains("@number\": \"1"),
        "missing week @number 1\n{json}"
    );
    assert!(
        json.contains("@number\": \"2"),
        "missing week @number 2\n{json}"
    );
}

#[test]
fn parity_tria_json_week1_days() {
    let json = run_tria(RuntimeFormat::Json);
    // Week 1 has 7 days: Mo, Di, Mi, Do, Fr, Sa, So
    for day in ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"] {
        assert!(json.contains(day), "missing weekday '{day}'\n{json}");
    }
    // Specific dates
    assert!(json.contains("04Jan10"), "missing 04Jan10\n{json}");
    assert!(json.contains("10Jan10"), "missing 10Jan10\n{json}");
}

#[test]
fn parity_tria_json_disciplines() {
    let json = run_tria(RuntimeFormat::Json);
    assert!(json.contains("Run"), "missing discipline 'Run'\n{json}");
    assert!(json.contains("Swim"), "missing discipline 'Swim'\n{json}");
    assert!(json.contains("Bike"), "missing discipline 'Bike'\n{json}");
}

#[test]
fn parity_tria_json_unit_details() {
    let json = run_tria(RuntimeFormat::Json);
    // Run unit: route + comment + time
    assert!(
        json.contains("Unterführung"),
        "missing route 'Unterführung'\n{json}"
    );
    assert!(
        json.contains("LS3AsicsTrabuco"),
        "missing Run comment\n{json}"
    );
    assert!(
        json.contains("15:34(15:34)"),
        "missing Run time\n{json}"
    );
    // Swim unit: place + distances
    assert!(
        json.contains("Hallenbad Markgröningen"),
        "missing Swim place\n{json}"
    );
    assert!(json.contains("400 ES"), "missing distance '400 ES'\n{json}");
    assert!(json.contains("100 AS"), "missing distance '100 AS'\n{json}");
}

#[test]
fn parity_tria_json_week2_bike() {
    let json = run_tria(RuntimeFormat::Json);
    // Bike unit in week 2
    assert!(json.contains("Centurion"), "missing material 'Centurion'\n{json}");
    assert!(json.contains("44.2 km"), "missing distance '44.2 km'\n{json}");
    assert!(json.contains("18.3 km/h"), "missing average '18.3 km/h'\n{json}");
}

#[test]
fn parity_tria_json_alternation_distance() {
    let json = run_tria(RuntimeFormat::Json);
    // The grammar uses alternation (|) for Länge/Laenge — both should produce distance nodes
    assert!(json.contains("6*50 Technik"), "missing distance '6*50 Technik'\n{json}");
    assert!(
        json.contains("8*50 GA1(25 Kraul"),
        "missing distance '8*50 GA1(25 Kraul...'\n{json}"
    );
}

// --- XML tests ---

#[test]
fn parity_tria_xml_structure() {
    let xml = run_tria(RuntimeFormat::Xml);
    // Key structural elements
    assert!(xml.contains("week"), "missing 'week' element\n{xml}");
    assert!(xml.contains("day"), "missing 'day' element\n{xml}");
    assert!(xml.contains("unit"), "missing 'unit' element\n{xml}");
    assert!(xml.contains("discipline"), "missing 'discipline' element\n{xml}");
    assert!(xml.contains("Run"), "missing 'Run' discipline\n{xml}");
    assert!(xml.contains("Swim"), "missing 'Swim' discipline\n{xml}");
    assert!(xml.contains("Bike"), "missing 'Bike' discipline\n{xml}");
}

#[test]
fn parity_tria_xml_week2() {
    let xml = run_tria(RuntimeFormat::Xml);
    assert!(xml.contains("Centurion"), "missing 'Centurion'\n{xml}");
    assert!(xml.contains("44.2 km"), "missing '44.2 km'\n{xml}");
}

// --- YAML tests ---

#[test]
fn parity_tria_yaml_structure() {
    let yaml = run_tria(RuntimeFormat::Yaml);
    assert!(yaml.contains("week"), "missing 'week'\n{yaml}");
    assert!(yaml.contains("day"), "missing 'day'\n{yaml}");
    assert!(yaml.contains("unit"), "missing 'unit'\n{yaml}");
    assert!(yaml.contains("discipline"), "missing 'discipline'\n{yaml}");
    assert!(yaml.contains("Run"), "missing 'Run'\n{yaml}");
    assert!(yaml.contains("Centurion"), "missing 'Centurion'\n{yaml}");
}
