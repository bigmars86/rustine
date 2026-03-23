#![no_main]
use libfuzzer_sys::fuzz_target;
use rustine::exec::execute;
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

/// Fuzz the lex → parse → execute pipeline with arbitrary input.
/// Uses a minimal valid grammar so most fuzzer effort goes to the executor
/// and serializer rather than being rejected at the parser stage.
///
/// Goal: no panics, no UB, no infinite loops.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Split the input: first half as grammar, second half as input text.
        let mid = s.len() / 2;
        let (grammar_src, input_text) = s.split_at(mid);

        if let Ok(tokens) = lex(grammar_src) {
            if let Ok(mut doc) = parse_gel_document(&tokens) {
                let _ = execute(&mut doc, "input", input_text);
            }
        }
    }
});
