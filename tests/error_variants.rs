use rustine::GelError;

/// Referencing a non-existent grammar yields a Runtime error.
#[test]
fn runtime_error_missing_grammar() {
    let src = "\
grammar main:
    match /foo/:
        out.create(\"root/found\")
";
    let result = rustine::parse_and_run(src, "nonexistent", "foo");
    let err = result.expect_err("should fail on missing grammar");
    match &err {
        GelError::Runtime { message, .. } => {
            assert!(message.contains("Grammar not found"), "msg: {message}");
            assert!(message.contains("nonexistent"), "should name the grammar: {message}");
        }
        other => panic!("expected Runtime error, got: {other:?}"),
    }
}

/// Invalid regex in grammar source fails validation before execution.
#[test]
fn validation_error_invalid_regex() {
    let src = "\
grammar main:
    match /[unclosed/:
        out.create(\"root/x\")
";
    let result = rustine::parse_and_run(src, "main", "x");
    let err = result.expect_err("should fail on invalid regex");
    match &err {
        GelError::Validation { message, .. } => {
            assert!(message.contains("invalid regex"), "msg: {message}");
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }
}

/// Undefined grammar inheritance target fails validation.
#[test]
fn validation_error_undefined_parent() {
    let src = "\
grammar child(nonexistent):
    match /foo/:
        out.create(\"root/x\")
";
    let result = rustine::parse_and_run(src, "child", "foo");
    let err = result.expect_err("should fail on undefined parent");
    match &err {
        GelError::Validation { message, .. } => {
            assert!(message.contains("inherits from undefined grammar"), "msg: {message}");
            assert!(message.contains("nonexistent"), "should name the parent: {message}");
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }
}
