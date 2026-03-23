use rustine::parse_and_run;

/// `out.open` / `out.enter` should auto-leave the tree when the action block ends.
#[test]
fn auto_leave_on_statement_exit() {
    let src = "\
grammar main:
    match /a/:
        out.open(\"root/container\")
        out.create(\"inside\")
    match /b/:
        out.create(\"root/outside\")
";
    let json = parse_and_run(src, "main", "ab").expect("exec");
    assert!(json.contains("\"container\""), "container should exist: {json}");
    assert!(
        json.contains("\"inside\""),
        "inside should exist within container: {json}"
    );
    assert!(json.contains("\"outside\""), "outside node should exist: {json}");
}

/// An explicit `out.leave()` cancels the pending auto-leave.
#[test]
fn explicit_leave_cancels_auto_leave() {
    let src = "\
grammar main:
    match /a/:
        out.open(\"root/container\")
        out.create(\"inside\")
        out.leave()
    match /b/:
        out.create(\"root/sibling\")
";
    let json = parse_and_run(src, "main", "ab").expect("exec");
    assert!(json.contains("\"container\""), "container should exist: {json}");
    assert!(json.contains("\"sibling\""), "sibling should exist: {json}");
}
