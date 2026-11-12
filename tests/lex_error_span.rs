use rustine::GelError;

/// Lexer error on unexpected character includes line and column.
#[test]
fn lex_error_has_span() {
    let src = "grammar main:\n    match /foo/:\n        @badchar";
    let result = rustine::parse_and_run(src, "main", "foo");
    let err = result.expect_err("should fail on @badchar");
    match &err {
        GelError::Lex { span, message, .. } => {
            assert!(message.contains("Unexpected character"), "msg: {message}");
            assert!(span.line > 0, "line should be set: {span}");
            assert!(span.col > 0, "col should be set: {span}");
        }
        other => panic!("expected Lex error, got: {other:?}"),
    }
}

/// Lexer error on single-line input reports line 1.
#[test]
fn lex_error_single_line_position() {
    let result = rustine::parse_and_run("@", "main", "x");
    let err = result.expect_err("@ is not valid");
    match &err {
        GelError::Lex { span, .. } => {
            assert_eq!(span.line, 1);
            assert_eq!(span.col, 1);
        }
        other => panic!("expected Lex, got: {other:?}"),
    }
}

/// A non-ASCII character is reported whole, with its byte length as span.
#[test]
fn lex_error_non_ascii_character() {
    for (src, ch) in [("grammar input: ä", 'ä'), ("\u{1F642}", '\u{1F642}')] {
        match rustine::parse_and_run(src, "input", "x").expect_err("not valid Gel") {
            GelError::Lex { span, message } => {
                assert_eq!(message, format!("Unexpected character '{ch}'"));
                assert_eq!(span.len, ch.len_utf8());
            }
            other => panic!("expected Lex, got: {other:?}"),
        }
    }
}
