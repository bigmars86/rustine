use rustine::parse_and_run;

#[test]
fn test_do_say_and_warn_traces() {
    let grammar = r#"grammar g:
match /foo/: 
  do.say("hello")
  do.warn("caution")
  out.create("root")
"#;
    let input = "foo";
    let json = parse_and_run(grammar, "g", input).expect("run");
    assert!(json.contains("say: hello"));
    assert!(json.contains("warn: caution"));
}
