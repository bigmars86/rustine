use rustine::parser::{OutputFormat, Parser};

#[test]
fn test_intra_pattern_backreference() {
    let gel = r#"
grammar main:
match /A([a-z]{2})(\d+)/ $1 $2:
  act($1,$2)
"#;
    // "Aab42" matches the regex, then $1 ("ab") and $2 ("42") match the rest.
    let parser = Parser::new(OutputFormat::Json);
    let out = parser.parse_and_run(gel, "main", "Aab42ab42").expect("exec failed");
    assert!(out.contains("act"), "{}", out);
    assert!(out.contains("\"ab\""), "{}", out);
    assert!(out.contains("\"42\""), "{}", out);
}
