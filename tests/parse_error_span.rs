use rustine::GelError;

/// A parse error (e.g. missing colon) yields GelError::Parse with a span.
#[test]
fn parse_error_has_span() {
    // Missing colon after grammar name
    let src = "grammar main\n    match /foo/:";
    let result = rustine::parse_and_run(src, "main", "x");
    let err = result.expect_err("should fail on missing colon");
    match &err {
        GelError::Parse { span, message, .. } => {
            assert!(!message.is_empty(), "message should be non-empty");
            assert!(span.line > 0, "line should be set: {span}");
        }
        other => panic!("expected Parse error, got: {other:?}"),
    }
}
