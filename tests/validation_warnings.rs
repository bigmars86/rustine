use rustine::parse_and_run;

/// The validation pass warns about undefined grammar references (bare invocations).
#[test]
fn validation_warning_undefined_grammar_ref() {
    let src = "\
grammar main:
    match /foo/:
        nonexistent_sub()
";
    // Should run but include a warning diagnostic about undefined grammar.
    // Actually, nonexistent_sub() will be treated as grammar invocation at runtime
    // and fail.  The validator should warn.
    let json = parse_and_run(src, "main", "foo").expect("exec should succeed (warning only)");
    assert!(json.contains("\"diagnostics\""), "should have diagnostics: {json}");
    // The warning appears in diagnostics (not necessarily in traces)
    assert!(
        json.contains("undefined grammar reference"),
        "should warn about undefined grammar: {json}"
    );
}

/// Validation detects undefined variables and emits a warning.
#[test]
fn validation_warning_undefined_variable() {
    let src = "\
grammar main:
    match undefined_var:
        out.create(\"root/x\")
";
    let json = parse_and_run(src, "main", "hello").expect("exec");
    assert!(
        json.contains("undefined variable"),
        "should warn about undefined variable: {json}"
    );
}
