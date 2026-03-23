use rustine::parse_and_run;

/// `when` with pipe-separated alternatives must check ALL of them, not just the first.
#[test]
fn when_checks_all_alternatives() {
    // Input matches the SECOND alternative.
    // when doesn't consume input, so we use do.return() to prevent infinite loop.
    let src = "\
grammar main:
    when /nomatch/ | /hello/:
        out.create(\"root/matched\")
        do.return()
";
    let json = parse_and_run(src, "main", "hello").expect("exec");
    assert!(
        json.contains("\"matched\""),
        "when should match second alternative: {json}"
    );
}
