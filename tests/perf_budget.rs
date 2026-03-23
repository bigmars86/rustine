//! Performance budget tests — catch catastrophic regressions in CI.
//!
//! These are NOT micro-benchmarks.  They run a known fixture multiple times
//! and assert the total wall-clock time stays within a generous budget
//! (typically 5× the expected time on a slow CI runner).
//!
//! A passing test means "no algorithmic regression".  If someone accidentally
//! introduces O(n²) behavior, these tests will fail even on slow hardware.
//!
//! For detailed performance analysis, use `cargo bench` (Criterion).

use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Fixture data (embedded at compile time)
// ---------------------------------------------------------------------------

const COMPLEX_SYNTAX: &str = include_str!("../fixtures/parity/complex/syntax1.gel");
const COMPLEX_INPUT: &str = include_str!("../fixtures/parity/complex/input1.txt");

const SIMPLE_SYNTAX: &str = include_str!("../fixtures/parity/simple/syntax1.gel");
const SIMPLE_INPUT: &str = include_str!("../fixtures/parity/simple/input1.txt");

// ---------------------------------------------------------------------------
// Budget: complex fixture — lex + parse + execute + serialize (JSON)
// ---------------------------------------------------------------------------

/// Run the complex fixture (30 KB grammar, 15 KB input) 10 times.
///
/// Expected: ~2 s release, ~10 s debug on CI.  Budget: 120 s.
/// This catches algorithmic regressions, not micro-regressions.
#[test]
fn perf_budget_complex_json() {
    let iterations = 10;
    let budget_secs = 120;

    let start = Instant::now();
    for _ in 0..iterations {
        let tokens = lex(COMPLEX_SYNTAX).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");
        let exec = execute(&mut doc, "input", COMPLEX_INPUT).expect("execute");
        let _json = serialize_tree(&exec, RuntimeFormat::Json);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < budget_secs,
        "PERFORMANCE BUDGET EXCEEDED: complex JSON × {iterations} took {elapsed:.2?} \
         (budget: {budget_secs}s). Possible algorithmic regression.",
    );
}

// ---------------------------------------------------------------------------
// Budget: simple fixture — high iteration count to test per-call overhead
// ---------------------------------------------------------------------------

/// Run the simple fixture 500 times.
///
/// Expected: < 1 s release, ~5 s debug on CI.  Budget: 30 s.
/// Catches per-call overhead regressions (e.g. leaked allocations,
/// accidental re-compilation of regexes on every call).
#[test]
fn perf_budget_simple_high_iter() {
    let iterations = 500;
    let budget_secs = 30;

    let start = Instant::now();
    for _ in 0..iterations {
        let tokens = lex(SIMPLE_SYNTAX).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");
        let exec = execute(&mut doc, "input", SIMPLE_INPUT).expect("execute");
        let _json = serialize_tree(&exec, RuntimeFormat::Json);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < budget_secs,
        "PERFORMANCE BUDGET EXCEEDED: simple JSON × {iterations} took {elapsed:.2?} \
         (budget: {budget_secs}s). Possible per-call overhead regression.",
    );
}

// ---------------------------------------------------------------------------
// Budget: XML serialization path
// ---------------------------------------------------------------------------

/// Run complex fixture → XML 10 times.
///
/// Ensures the XML serializer doesn't regress independently of JSON.
#[test]
fn perf_budget_complex_xml() {
    let iterations = 10;
    let budget_secs = 120;

    let start = Instant::now();
    for _ in 0..iterations {
        let tokens = lex(COMPLEX_SYNTAX).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");
        let exec = execute(&mut doc, "input", COMPLEX_INPUT).expect("execute");
        let _xml = serialize_tree(&exec, RuntimeFormat::Xml);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < budget_secs,
        "PERFORMANCE BUDGET EXCEEDED: complex XML × {iterations} took {elapsed:.2?} \
         (budget: {budget_secs}s). Possible algorithmic regression.",
    );
}
