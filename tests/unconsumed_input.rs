//! A grammar that stops before the end of the input is an error, not a silently truncated result.

use rustine::exec::execute_tree;
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

fn error(gel: &str, input: &str) -> Option<String> {
    let mut doc = parse_gel_document(&lex(gel).unwrap()).unwrap();
    doc.compile_regexes();
    execute_tree(&doc, "input", input).unwrap().error
}

const FOO: &str = "grammar input:\n    match /foo\\n/:\n        out.add('found')\n";

#[test]
fn input_without_a_match_is_an_error() {
    assert_eq!(
        error(FOO, "foo\nfoo\nbar\nfoo\n").as_deref(),
        Some("grammar 'input' stopped at line 3 before the end of the input: \"bar\\nfoo\\n\"")
    );
}

#[test]
fn consuming_the_whole_input_is_no_error() {
    assert_eq!(error(FOO, "foo\nfoo\n"), None);
    assert_eq!(error(FOO, ""), None);
}

#[test]
fn returning_before_the_end_is_an_error() {
    let gel =
        "grammar input:\n    match /stop\\n/:\n        do.return()\n    match /\\w+\\n/:\n        out.add('word')\n";
    let err = error(gel, "a\nstop\nb\n").unwrap();
    assert!(err.starts_with("grammar 'input' stopped at line 3 "), "{err}");
}

#[test]
fn a_sub_grammar_may_stop_where_its_caller_continues() {
    let gel = "grammar digits:\n    match /\\d+\\n/:\n        out.add('digit')\n\ngrammar input:\n    match /start\\n/:\n        digits()\n    match /\\w+\\n/:\n        out.add('word')\n";
    assert_eq!(error(gel, "start\n1\n2\nend\n"), None);
}

#[test]
fn do_fail_keeps_its_message() {
    let gel = "grammar input:\n    match /x/:\n        do.fail('bad x')\n";
    assert_eq!(error(gel, "xyz").as_deref(), Some("bad x"));
}

#[cfg(feature = "cli")]
#[test]
fn rgel_exits_with_an_error() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (grammar, input) = (dir.join("unconsumed.gel"), dir.join("unconsumed.txt"));
    std::fs::write(&grammar, FOO).unwrap();
    std::fs::write(&input, "foo\nbar\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rgel"))
        .arg("-s")
        .arg(&grammar)
        .arg(&input)
        .output()
        .expect("run rgel");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no output expected");
    assert!(String::from_utf8_lossy(&out.stderr).contains("stopped at line 2"));
}
