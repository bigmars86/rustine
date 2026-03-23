use rustine::parse_and_run;

#[test]
fn do_skip_stops_later_actions() {
    let grammar = "grammar g:\n    match /foo/:\n        out.create(\"root\")\n        do.skip()\n        out.add(\"root/after\")\n";
    let input = "foo";
    let json = parse_and_run(grammar, "g", input).expect("run");
    // Expect root node created; 'after' should not appear in the output tree
    assert!(json.contains("\"root\""), "expected root node: {json}");
    // The output section should not contain an 'after' node
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(
        !output_section.contains("\"after\""),
        "after should not be in output tree: {json}"
    );
}
