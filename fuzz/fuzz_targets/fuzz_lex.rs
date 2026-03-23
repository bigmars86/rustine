#![no_main]
use libfuzzer_sys::fuzz_target;

/// Fuzz the lexer with arbitrary byte strings.
/// Goal: no panics, no UB — only Ok/Err returns.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = rustine::parser::lexer::lex(s);
    }
});
