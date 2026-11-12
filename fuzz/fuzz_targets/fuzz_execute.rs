#![no_main]
use libfuzzer_sys::fuzz_target;
use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

// Fuzz the lex → parse → execute → serialize pipeline: the grammar comes
// before the first NUL byte, the input text after it.
//
// Goal: no panics, no UB, no infinite loops.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let (grammar_src, input_text) = s.split_once('\0').unwrap_or((s, ""));
        if let Ok(tokens) = lex(grammar_src) {
            if let Ok(mut doc) = parse_gel_document(&tokens) {
                if let Ok(result) = execute(&mut doc, "input", input_text) {
                    for format in [RuntimeFormat::Json, RuntimeFormat::Xml, RuntimeFormat::Yaml] {
                        let _ = serialize_tree(&result, format);
                    }
                }
            }
        }
    }
});
