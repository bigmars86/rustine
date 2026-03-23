use rustine::parser::{OutputFormat, Parser};

#[test]
fn test_capture_groups_and_refs() {
    let gel = r#"
grammar main:
match /He(l)(l)o/:
  act($1,$2)
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Hello").expect("exec failed");
    // Expect substituted captures
    assert!(out.contains("\"l\""), "{}", out);
    let count_l = out.matches("\"l\"").count();
    assert!(count_l >= 2, "{}", out);
    assert!(out.contains("capture_history"), "{}", out);
}

#[test]
fn test_capture_reference_in_when_scoped() {
    // Under scoped semantics, $1 in a new statement should not resolve, so second action won't appear.
    let gel = r#"
grammar main:
match /He(l+)o/:
  act($1)
when $1:
  act2($1)
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Helllo").expect("exec failed");
    assert!(out.contains("act"), "{}", out);
    assert!(!out.contains("act2"), "{}", out);
}
