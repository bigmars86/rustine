//! Deeply nested grammar calls end the run with an error instead of overflowing the stack.

use rustine::exec::execute_tree;
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

const NESTED: &str = "grammar input:\n    match /\\{/:\n        out.enter('x')\n        input()\n    match /\\}/:\n        do.return()\n";

/// The error of `depth` nested calls, on a stack that also fits a debug build.
fn error_for(depth: usize) -> Option<String> {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut doc = parse_gel_document(&lex(NESTED).unwrap()).unwrap();
            doc.compile_regexes();
            let input = "{".repeat(depth) + &"}".repeat(depth);
            execute_tree(&doc, "input", &input).unwrap().error
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn nesting_up_to_the_limit_works() {
    assert_eq!(error_for(250), None);
}

#[test]
fn deeper_nesting_is_an_error() {
    assert_eq!(
        error_for(251).as_deref(),
        Some("grammar calls nested more than 250 levels deep (calling 'input' at line 1)")
    );
    assert!(error_for(100_000).is_some());
}

#[cfg(feature = "cli")]
#[test]
fn rgel_reports_deep_nesting() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (grammar, input) = (dir.join("nested.gel"), dir.join("nested.txt"));
    std::fs::write(&grammar, NESTED).unwrap();
    std::fs::write(&input, "{".repeat(5000) + &"}".repeat(5000)).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rgel"))
        .arg("-s")
        .arg(&grammar)
        .arg(&input)
        .output()
        .expect("run rgel");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nested more than 250 levels"));
}
