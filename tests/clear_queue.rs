use rustine::parse_and_run;

/// `out.clear_queue` must preserve persistent triggers while removing non-persistent ones.
#[test]
fn clear_queue_preserves_persistent_triggers() {
    let src = "\
grammar main:
    match /setup/:
        out.enqueue_before_persist(/data/, \"root/persist\")
        out.clear_queue()
    match /data/:
        out.create(\"root/item\")
";
    let json = parse_and_run(src, "main", "setupdata").expect("exec");
    assert!(
        json.contains("\"persist\""),
        "persistent trigger should survive clear_queue: {json}"
    );
    assert!(
        json.contains("\"item\""),
        "normal match should still create item: {json}"
    );
}

#[test]
fn clear_queue_removes_non_persistent_triggers() {
    let src = "\
grammar main:
    match /setup/:
        out.enqueue_before(/data/, \"root/ephemeral\")
        out.clear_queue()
    match /data/:
        out.create(\"root/item\")
";
    let json = parse_and_run(src, "main", "setupdata").expect("exec");
    assert!(
        !json.contains("\"ephemeral\""),
        "non-persistent trigger should be cleared: {json}"
    );
    assert!(
        json.contains("\"item\""),
        "normal match should still create item: {json}"
    );
}
