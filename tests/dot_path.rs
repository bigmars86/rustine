use rustine::parse_and_run;

/// The path segment `.` means "current node" — it should not create a child.
#[test]
fn dot_path_add_sets_text_on_current_node() {
    let src = "\
grammar main:
    match /(.+)/:
        out.open(\"root/child\")
        out.add(\".\", $1)
        out.leave()
";
    let json = parse_and_run(src, "main", "hello").expect("exec");
    assert!(json.contains("\"child\""), "child node should exist: {json}");
    assert!(
        json.contains("hello"),
        "child should have text from dot-path add: {json}"
    );
}

#[test]
fn dot_path_attribute_on_current_node() {
    let src = "\
grammar main:
    match /(.+)/:
        out.open(\"root/item\")
        out.add_attribute(\".\", \"key\", $1)
        out.leave()
";
    let json = parse_and_run(src, "main", "val").expect("exec");
    assert!(json.contains("\"@key\""), "attribute key should appear: {json}");
    assert!(json.contains("val"), "attribute value should be present: {json}");
}
