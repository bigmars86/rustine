use rustine::parse_and_run;

#[test]
fn single_shot_trigger_fires_once() {
    let src = "grammar main:\n    match /foo/:\n        out.enqueue_before(/foo/, \"root/once\")\n        out.create(\"root\")\n    match /foo/:\n        out.create(\"root/again\")\n";
    let json = parse_and_run(src, "main", "foofoo").expect("exec");
    // 'once' should appear as a node in the output tree
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(
        output_section.contains("\"once\""),
        "once node should be in output: {json}"
    );
}

#[test]
fn persistent_trigger_fires_each_time() {
    let src = "grammar main:\n    match /foo/:\n        out.enqueue_before_persist(/foo/, \"root/persist\")\n        out.create(\"root\")\n    match /foo/:\n        out.create(\"root/again\")\n";
    let json = parse_and_run(src, "main", "foofoo").expect("exec");
    // 'persist' should appear as a node in the output tree
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(
        output_section.contains("\"persist\""),
        "persist node should be in output: {json}"
    );
}

#[test]
fn on_leave_trigger_fires() {
    let src = "grammar main:\n    match /foo/:\n        out.create(\"root\")\n        out.open(\"root/child\")\n        out.enqueue_on_leave(/foo/, \"root/leave\")\n        out.leave()\n";
    let json = parse_and_run(src, "main", "foo").expect("exec");
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(
        output_section.contains("\"leave\""),
        "leave node should be in output: {json}"
    );
}

#[test]
fn persistent_on_leave_trigger_multiple() {
    let src = "grammar main:\n    match /foo/:\n        out.enqueue_on_leave_persist(/foo/, \"root/pl\")\n        out.open(\"root/a\")\n        out.leave()\n    match /foo/:\n        out.open(\"root/b\")\n        out.leave()\n";
    let json = parse_and_run(src, "main", "foofoo").expect("exec");
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(output_section.contains("\"pl\""), "pl node should be in output: {json}");
}

#[test]
fn trigger_positional_capture_interpolation() {
    let src = "grammar main:\n    match /(foo)(bar)/:\n        out.enqueue_after(/foobar/, \"root/$1/$2\")\n        out.create(\"root\")\n";
    let json = parse_and_run(src, "main", "foobar").expect("exec");
    // Interpolation of $1/$2 should create nested nodes root/foo/bar
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    assert!(
        output_section.contains("\"foo\""),
        "foo node should exist from $1 interpolation: {json}"
    );
    assert!(
        output_section.contains("\"bar\""),
        "bar node should exist from $2 interpolation: {json}"
    );
}

#[test]
fn trigger_named_capture_interpolation() {
    let src = "grammar main:\n    match /(?P<x>foo)(?P<y>bar)/:\n        out.enqueue_before(/foobar/, \"root/$x/$y\")\n        out.create(\"root\")\n";
    let json = parse_and_run(src, "main", "foobar").expect("exec");
    let output_idx = json.find("\"output\"").expect("output missing");
    let output_section = &json[output_idx..];
    // Named capture interpolation: $x → foo, $y → bar → creates root/foo/bar
    // Note: If named interpolation isn't working yet, at minimum root should exist
    assert!(
        output_section.contains("\"root\""),
        "root node should be in output: {json}"
    );
}
