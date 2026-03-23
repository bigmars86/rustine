use rustine::parse_and_run;

/// Text values passed to `out.create` / `out.add` must be percent-decoded.
#[test]
fn percent_encoded_space() {
    let src = "\
grammar main:
    match /.+/:
        out.create(\"root/item\", \"hello%20world\")
";
    let json = parse_and_run(src, "main", "x").expect("exec");
    assert!(
        json.contains("hello world"),
        "percent-encoded %20 should be decoded to space: {json}"
    );
}

#[test]
fn plus_decoded_as_space() {
    let src = "\
grammar main:
    match /.+/:
        out.create(\"root/item\", \"a+b\")
";
    let json = parse_and_run(src, "main", "x").expect("exec");
    // Python's urllib.parse.unquote does NOT convert + to space.
    assert!(json.contains("a+b"), "+ should NOT be decoded to space: {json}");
}
