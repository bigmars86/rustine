use rustine::parse_and_run;

#[test]
fn test_do_fail_stops_execution() {
    let grammar = "grammar g:\n    match /foo/:\n        out.create(\"root\")\n        do.fail(\"fatal\")\n        out.add(\"root/after_fail\")\n    match /bar/:\n        out.add(\"root/bar\")\n";
    let input = "foobar";
    let json = parse_and_run(grammar, "g", input).expect("run");
    // do.fail records error and stops processing; bar should not be reached
    assert!(json.contains("\"error\": \"fatal\""), "expected error field: {json}");
    assert!(json.contains("\"root\""), "expected root node: {json}");
    assert!(!json.contains("after_fail"), "after_fail should not appear: {json}");
    assert!(!json.contains("\"bar\""), "bar should not appear after fail: {json}");
}
