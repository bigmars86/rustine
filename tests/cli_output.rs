//! `rgel` streams its output and stops quietly when the reader goes away.
#![cfg(feature = "cli")]

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn rgel_stops_quietly_when_stdout_closes() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (grammar, input) = (dir.join("lines.gel"), dir.join("lines.txt"));
    std::fs::write(
        &grammar,
        "grammar input:\n    match /(\\w+)\\n/:\n        out.create('line', '$1')\n",
    )
    .unwrap();
    // Several MB of output, far more than a pipe buffer holds.
    std::fs::write(&input, "word\n".repeat(200_000)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rgel"))
        .args(["-f", "json", "-s"])
        .arg(&grammar)
        .arg(&input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run rgel");
    let mut head = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut head).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(&head[..6], b"{\n    ");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
}
