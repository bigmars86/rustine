use rustine::parser::{OutputFormat, Parser};

#[test]
fn test_scope_resets_between_statements() {
    let gel = r#"
grammar main:
match /A(x)(y)/:
  act1($1,$2)
when $1:
  act2($1)
match /xy/:
  act3("done")
"#;
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Axyxy").expect("exec failed");
    // act1 should have substituted captures
    assert!(out.contains("act1"), "{}", out);
    // act2 should NOT fire because $1 not available in a new statement scope (pattern $1 fails)
    assert!(!out.contains("act2"), "{}", out);
    // act3 should fire based on literal match
    assert!(out.contains("act3"), "{}", out);
}
