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
    assert!(json.contains("a+b"), "+ should NOT be decoded to space: {json}");
}

/// Attribute values decode `%XX` like text; a `+` written in the grammar stays `+`.
#[test]
fn plus_in_attribute_value_stays() {
    let src = "\
grammar main:
    match /.+/:
        out.add('node?name=\"a+b%20c\"')
";
    let json = parse_and_run(src, "main", "x").expect("exec");
    assert!(json.contains("\"a+b c\""), "{json}");
}

/// Tag names are decoded too, so a captured name keeps characters such as `:` and `/`.
#[test]
fn tag_names_are_percent_decoded() {
    let src = "\
grammar main:
    match /\\S+/:
        out.add('$0')
";
    let json = parse_and_run(src, "main", "Foo:Bar/Baz").expect("exec");
    assert!(json.contains("\"foo:bar/baz\""), "{json}");
}
