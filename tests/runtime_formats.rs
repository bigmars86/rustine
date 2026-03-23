use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parse_and_run;
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

const GEL_SOURCE: &str = "grammar main:\n    match /Hello/:\n        out.create(\"line\")\n";

#[test]
fn runtime_json_xml_yaml_emit() {
    let input = "Hello World";

    // JSON via convenience helper
    let json = parse_and_run(GEL_SOURCE, "main", input).expect("json run");
    assert!(json.contains("\"consumed\""), "json missing consumed: {json}");
    assert!(json.contains("\"line\""), "json missing line node: {json}");

    // XML via execute + serialize
    let tokens = lex(GEL_SOURCE).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    let exec = execute(&mut doc, "main", input).expect("execute");
    let xml = serialize_execution(&exec, RuntimeFormat::Xml);
    assert!(xml.contains("<execution"), "xml missing execution tag: {xml}");
    assert!(xml.contains("<output>"), "xml missing output: {xml}");

    let yaml = serialize_execution(&exec, RuntimeFormat::Yaml);
    assert!(yaml.contains("execution:"), "yaml missing execution: {yaml}");
    assert!(yaml.contains("actions:"), "yaml missing actions: {yaml}");
}
