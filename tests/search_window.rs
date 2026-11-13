//! One regex match spans at most `search_window` bytes of the input (default 1000).

use rustine::exec::execute_tree;
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

const LINES: &str = "grammar input:\n    match /[a-z]+\\n/:\n        out.add('line')\n";

/// The error of a run on one line of `len` letters, with the default or the given window.
fn error_for(len: usize, window: Option<usize>) -> Option<String> {
    let mut doc = parse_gel_document(&lex(LINES).unwrap()).unwrap();
    if let Some(bytes) = window {
        doc.search_window = bytes;
    }
    doc.compile_regexes();
    execute_tree(&doc, "input", &("a".repeat(len) + "\n")).unwrap().error
}

#[test]
fn default_window_is_1000_bytes() {
    assert_eq!(error_for(999, None), None);
    let err = error_for(1000, None).expect("a 1001-byte match does not fit");
    assert!(err.contains("stopped at line 1"), "{err}");
}

#[test]
fn window_size_is_adjustable() {
    assert_eq!(error_for(1999, Some(2000)), None);
    assert!(error_for(2000, Some(2000)).is_some());
    assert_eq!(error_for(99, Some(100)), None);
    assert!(error_for(100, Some(100)).is_some());
}

#[cfg(feature = "cli")]
#[test]
fn rgel_search_window() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (grammar, input) = (dir.join("window.gel"), dir.join("window.txt"));
    std::fs::write(&grammar, LINES).unwrap();
    std::fs::write(&input, "a".repeat(1500) + "\n").unwrap();
    let rgel = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_rgel"))
            .args(args)
            .args(["-f", "json", "-s"])
            .arg(&grammar)
            .arg(&input)
            .output()
            .expect("run rgel")
    };
    let out = rgel(&[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stopped at line 1"));
    let out = rgel(&["--search-window", "2000"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"line\""));
    assert_eq!(rgel(&["--search-window", "0"]).status.code(), Some(2));
}
