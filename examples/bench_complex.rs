//! Benchmark: run the "complex" fixture for PGO training or quick perf checks.
//!
//! Usage:
//!   cargo run --release --no-default-features --example bench_complex
//!
//! This runs the complex fixture (14.9 KB input) to provide a representative
//! training workload for profile-guided optimisation (PGO).  See pgo-build.sh
//! and pgo-build.ps1 for automated PGO workflows.

use rustine::exec::execute;
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;

#[cfg(feature = "bench")]
use rustine::exec::{serialize_tree, RuntimeFormat};

use std::time::Instant;

fn main() {
    let syntax_path = "fixtures/parity/complex/syntax1.gel";
    let input_path = "fixtures/parity/complex/input1.txt";
    let iterations = 50;

    #[cfg(feature = "bench")]
    {
        use rustine::exec::{execute_precompiled, serialize_tree, RuntimeFormat};
        let grammar_src = std::fs::read_to_string(syntax_path).expect("read grammar");
        let input = std::fs::read_to_string(input_path).expect("read input");
        let tokens = lexer::lex(&grammar_src).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");
        doc.compile_regexes();

        let t0 = Instant::now();
        for _ in 0..iterations {
            let result = execute_precompiled(&doc, "input", &input).expect("exec");
            let _json = serialize_tree(&result, RuntimeFormat::Json);
        }
        let elapsed = t0.elapsed();
        eprintln!(
            "bench_complex: {iterations} iterations in {:.3}s ({:.1} iter/s)",
            elapsed.as_secs_f64(),
            iterations as f64 / elapsed.as_secs_f64()
        );
        return;
    }

    // Fallback: non-bench build uses execute() which takes &mut doc
    let grammar_src = std::fs::read_to_string(syntax_path).expect("read grammar");
    let input = std::fs::read_to_string(input_path).expect("read input");
    let tokens = lexer::lex(&grammar_src).expect("lex");
    let doc = parse_gel_document(&tokens).expect("parse");

    let t0 = Instant::now();
    for _ in 0..iterations {
        let mut doc_clone = doc.clone();
        let _result = execute(&mut doc_clone, "input", &input).expect("exec");
    }
    let elapsed = t0.elapsed();
    eprintln!(
        "bench_complex: {iterations} iterations in {:.3}s ({:.1} iter/s)",
        elapsed.as_secs_f64(),
        iterations as f64 / elapsed.as_secs_f64()
    );
}
