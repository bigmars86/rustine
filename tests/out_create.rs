use rustine::parse_and_run;

#[test]
fn test_out_create_creates_nodes() {
    let gel =
        "grammar main:\n    match 'foo':\n        out.create('a/b/c','value')\n        out.create('a/b/c','value2')\n";
    let res = parse_and_run(gel, "main", "foo").expect("run");
    // Both values should appear as #text entries
    assert!(res.contains("\"#text\": \"value\""), "value missing: {res}");
    assert!(res.contains("\"#text\": \"value2\""), "value2 missing: {res}");
    // Output tree should have node keys for 'a', 'b' (nested)
    let output_idx = res.find("\"output\"").expect("output missing");
    let output_section = &res[output_idx..];
    assert!(output_section.contains("\"a\""), "node a missing: {res}");
    assert!(output_section.contains("\"b\""), "node b missing: {res}");
    assert!(output_section.contains("\"c\""), "node c missing: {res}");
}
