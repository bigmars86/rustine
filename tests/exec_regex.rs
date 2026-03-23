use rustine::parser::{OutputFormat, Parser};

#[test]
fn test_regex_basic() {
    let gel = r#"
grammar main:
match /Hel.+o/:
  act("A")
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Hello").expect("exec failed");
    assert!(out.contains("act"), "output: {}", out);
}

#[test]
fn test_regex_case_insensitive() {
    let gel = r#"
grammar main:
imatch /heL.+o/:
  act("B")
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Hello").expect("exec failed");
    assert!(out.contains("act"), "output: {}", out);
}
