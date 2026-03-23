use rustine::parse_and_run;

/// `out.create` / `out.add` / `out.replace` must fire enqueued `on_add` triggers.
#[test]
fn out_create_fires_on_add_trigger() {
    let src = "\
grammar main:
    match /setup/:
        out.enqueue_on_add(/data/, \"root/triggered\")
    match /data/:
        out.create(\"root/item\", \"val\")
";
    let json = parse_and_run(src, "main", "setupdata").expect("exec");
    assert!(
        json.contains("\"triggered\""),
        "on_add trigger should fire during out.create: {json}"
    );
    assert!(json.contains("\"item\""), "created item should exist: {json}");
}
