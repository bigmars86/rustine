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

/// A grammar call ignores the callee's return level: `do.return(2)` inside a
/// sub-grammar leaves only that grammar, and the caller matches on.
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
    assert!(
        json.contains("\"after\""),
        "after should be reached (return levels don't propagate through grammar calls): {json}"
    );
}
