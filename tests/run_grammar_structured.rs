use rustine::parse_and_run;

#[test]
fn run_grammar_returns_structured_json() {
    let src = "grammar main:\n    match /abc/:\n        do.say(\"hello\")\n";
    let json = parse_and_run(src, "main", "abc").expect("run grammar");
    assert!(json.contains("\"consumed\": 3"), "consumed mismatch: {json}");
    assert!(json.contains("\"actions\""), "actions missing: {json}");
    assert!(json.contains("\"output\""), "output missing: {json}");
    assert!(json.contains("do.say"), "do.say missing: {json}");
    assert!(json.contains("hello"), "hello missing: {json}");
}
