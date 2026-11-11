//! `rgel` lowercases tag names in `out.*` paths unless `--preserve-case` is given.
#![cfg(feature = "cli")]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn rgel(name: &str, extra_args: &[&str]) -> String {
    let grammar = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.gel"));
    std::fs::write(
        &grammar,
        "grammar input:\n    match /(\\w+)\\n/:\n        out.add('MixedCase', '$1')\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rgel"))
        .arg("-s")
        .arg(&grammar)
        .args(extra_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rgel");
    child.stdin.take().unwrap().write_all(b"value\n").unwrap();
    let out = child.wait_with_output().expect("rgel output");
    assert!(
        out.status.success(),
        "rgel failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn rgel_lowercases_by_default() {
    let xml = rgel("rgel_default", &[]);
    assert!(xml.contains("<mixedcase>value</mixedcase>"), "{xml}");
}

#[test]
fn rgel_preserve_case_flag() {
    let xml = rgel("rgel_preserve_case", &["--preserve-case"]);
    assert!(xml.contains("<MixedCase>value</MixedCase>"), "{xml}");
}
