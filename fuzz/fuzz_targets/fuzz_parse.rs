#![no_main]
use libfuzzer_sys::fuzz_target;

/// Fuzz the full lex → parse pipeline.
/// Goal: no panics, no UB — only Ok/Err returns.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(tokens) = rustine::parser::lexer::lex(s) {
            let _ = rustine::parser::syntax::parse_gel_document(&tokens);
        }
    }
});
