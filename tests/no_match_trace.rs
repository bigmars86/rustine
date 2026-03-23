use rustine::parse_and_run;

/// When no statement matches the remaining input, a diagnostic trace is emitted.
#[test]
fn no_match_trace_on_stuck_grammar() {
    let src = "\
grammar main:
    match /foo/:
        out.create(\"root/found\")
";
    let json = parse_and_run(src, "main", "fooBAR").expect("exec");
    assert!(json.contains("\"found\""), "foo should match: {json}");
    assert!(
        json.contains("no match found"),
        "should have no-match trace for stuck input: {json}"
    );
}
