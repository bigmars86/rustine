//! `rgel` exits with status 1 when the grammar calls `do.fail`.
#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

#[test]
fn rgel_do_fail_exits_with_error() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let grammar = dir.join("rgel_do_fail.gel");
    let input = dir.join("rgel_do_fail.txt");
    std::fs::write(
        &grammar,
        "grammar sub:\n    match 'x':\n        do.fail('boom')\n\ngrammar input:\n    match 'a':\n        sub()\n",
    )
    .unwrap();
    std::fs::write(&input, "ax").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rgel"))
        .arg("-s")
        .arg(&grammar)
        .arg(&input)
        .output()
        .expect("run rgel");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no output expected");
    assert!(String::from_utf8_lossy(&out.stderr).contains("boom"));
}
