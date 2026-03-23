use rustine::parser::{OutputFormat, Parser};

#[test]
fn yaml_basic_structure() {
    // Valid Gel syntax: define + grammar + match with action block
    let src = "define ws /\\s+/\ngrammar main: match /abc/ : say(\"hi\")";
    let parser = Parser::new(OutputFormat::Yaml);
    let yaml = parser.parse_str(src).expect("yaml generation");
    assert!(yaml.contains("gel_document:"));
    assert!(yaml.contains("defines:"));
    assert!(yaml.contains("grammars:"));
    assert!(yaml.contains("statements:"));
    assert!(yaml.contains("type: match"));
}
