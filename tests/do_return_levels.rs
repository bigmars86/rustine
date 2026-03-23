use rustine::parse_and_run;

/// `do.return()` without an argument exits the current grammar level.
#[test]
fn do_return_default_exits_one_level() {
    let src = "\
grammar main:
    match /foo/:
        out.create(\"root/found\")
        do.return()
    match /bar/:
        out.create(\"root/bar\")
";
    let json = parse_and_run(src, "main", "foobar").expect("exec");
    assert!(json.contains("\"found\""), "found should exist: {json}");
    assert!(
        !json.contains("\"bar\""),
        "bar should not be reached after do.return: {json}"
    );
    assert!(json.contains("\"consumed\": 3"), "consumed should be 3: {json}");
}

/// In Python Gelatin, `Function.parse()` ignores the grammar's return value.
/// It only checks whether `context.start` moved. So `do.return(2)` inside
/// a sub-grammar exits ONLY that grammar; the extra level is lost.
/// The caller continues normally and can match subsequent input.
#[test]
fn do_return_levels_lost_through_subgrammar() {
    let src = "\
grammar sub:
    match /inner/:
        out.create(\"root/inner\")
        do.return(2)

grammar main:
    match /start/:
        out.create(\"root/start\")
        sub()
    match /after/:
        out.create(\"root/after\")
";
    let json = parse_and_run(src, "main", "startinnerafter").expect("exec");
    assert!(json.contains("\"start\""), "start should exist: {json}");
    assert!(json.contains("\"inner\""), "inner should exist: {json}");
    // Python semantics: the return level is lost when crossing a grammar call boundary.
    // The caller continues and matches "after".
    assert!(
        json.contains("\"after\""),
        "after SHOULD be reached (Python semantics — return levels don't propagate through grammar calls): {json}"
    );
}
