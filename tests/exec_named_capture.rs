use rustine::parser::{OutputFormat, Parser};

#[test]
fn test_named_capture_pattern_and_action() {
    let gel = r#"
grammar main:
match /(?P<foo>foo)(?P<bar>\d{2})/ $foo $bar:
  act($foo,$bar)
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "foo42foo42").expect("exec failed");
    assert!(out.contains("act"), "{}", out);
    assert!(out.contains("foo"), "{}", out);
    assert!(out.contains("42"), "{}", out);
    assert!(out.contains("capture_names_history"), "{}", out);
}
