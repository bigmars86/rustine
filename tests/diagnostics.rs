use rustine::parse_and_run;

/// Execution result JSON contains a diagnostics array with severity levels.
#[test]
fn diagnostics_array_in_json() {
    // A no-match scenario emits a warning diagnostic.
    let src = "\
grammar main:
    match /foo/:
        out.create(\"root/found\")
";
    let json = parse_and_run(src, "main", "fooBAR").expect("exec");
    assert!(json.contains("\"diagnostics\""), "should have diagnostics key: {json}");
    assert!(
        json.contains("\"severity\": \"warning\""),
        "no-match should be warning: {json}"
    );
    assert!(
        json.contains("no match found"),
        "should contain no-match message: {json}"
    );
}

/// Normal execution with full match has info-level diagnostics.
#[test]
fn diagnostics_info_on_match() {
    let src = "\
grammar main:
    match /hello/:
        out.create(\"root/greeting\")
";
    let json = parse_and_run(src, "main", "hello").expect("exec");
    assert!(json.contains("\"diagnostics\""), "should have diagnostics: {json}");
    assert!(
        json.contains("\"severity\": \"info\""),
        "match should emit info diagnostic: {json}"
    );
    assert!(json.contains("match consumed"), "should contain match trace: {json}");
}

/// Diagnostics carry both old-style traces and new structured diagnostics.
#[test]
fn traces_and_diagnostics_parallel() {
    let src = "\
grammar main:
    match /a/:
        out.create(\"root/a\")
";
    let json = parse_and_run(src, "main", "a").expect("exec");
    // Both traces and diagnostics should contain the match event
    assert!(json.contains("\"traces\""), "traces array present: {json}");
    assert!(json.contains("\"diagnostics\""), "diagnostics array present: {json}");
    let trace_count = json.matches("match consumed").count();
    assert!(
        trace_count >= 2,
        "match consumed should appear in both traces and diagnostics: {json}"
    );
}
