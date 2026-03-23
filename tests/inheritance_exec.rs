/// Tests for grammar inheritance at execution time.
///
/// Covers:
///   1. Single-level inheritance (child extends parent)
///   2. Multi-level / chain inheritance (C → B → A)
///   3. Cycle detection (A → B → A does not loop)
///   4. Sub-grammar call into an inheriting grammar
use rustine::parse_and_run;

// ── 1. Single-level: child inherits parent's skip rule ──────────────

#[test]
fn single_level_inheritance() {
    // `base` has a skip rule for whitespace.
    // `child(base)` only has a match rule.
    // Because child inherits base, whitespace is skipped before "hello" is matched.
    let gel = r#"
grammar base:
    skip /\s+/
grammar child(base):
    match /hello/:
        out.create("root")
        out.add("greeting", "$0")
"#;
    let json = parse_and_run(gel, "child", "   hello").expect("single-level inheritance");
    assert!(
        json.contains("greeting"),
        "child should match via inherited skip: {json}"
    );
    assert!(json.contains("hello"), "captured value: {json}");
}

// ── 2. Multi-level: C(B), B(A) – three-level chain ─────────────────

#[test]
fn multi_level_inheritance() {
    // grammar A: skip whitespace
    // grammar B(A): match numbers → create "num" node
    // grammar C(B): match words  → create "word" node
    //
    // Running C should have A-skip + B-match + C-match available.
    let gel = r#"
grammar A:
    skip /\s+/
grammar B(A):
    match /(\d+)/:
        out.create("root")
        out.add("num", "$1")
grammar C(B):
    match /([a-z]+)/:
        out.add("word", "$1")
"#;
    // Input: whitespace then a number then whitespace then a word.
    // A's skip rule eats whitespace, B's rule matches "42", C's rule matches "abc".
    let json = parse_and_run(gel, "C", "  42   abc").expect("multi-level inheritance");
    assert!(json.contains("num"), "B-rule should fire via A→B→C chain: {json}");
    assert!(json.contains("42"), "number captured: {json}");
    assert!(json.contains("word"), "C-rule should fire: {json}");
    assert!(json.contains("abc"), "word captured: {json}");
}

// ── 3. Cycle detection: A(B) + B(A) must not loop ───────────────────

#[test]
fn cycle_detection_does_not_hang() {
    // Both grammars point at each other. The engine should break the cycle
    // and still be able to match.
    let gel = r#"
grammar A(B):
    match /hello/:
        out.create("root")
        out.add("msg", "$0")
grammar B(A):
    skip /\s+/
"#;
    // Even with a cycle, running A should work (it will see its own statements,
    // the cycle just prevents revisiting B→A again).
    let json = parse_and_run(gel, "A", "  hello").expect("cycle must not hang");
    assert!(json.contains("msg"), "A should still match: {json}");
}

// ── 4. Sub-grammar call dispatches to inheriting grammar ────────────

#[test]
fn subgrammar_call_with_inheritance() {
    // `inner(base)` inherits base-skip and has its own match.
    // `main` calls `inner` as a bare grammar name action.
    let gel = r#"
grammar main:
    match /START/:
        inner
grammar base:
    skip /\s+/
grammar inner(base):
    match /hello/:
        out.create("root")
        out.add("val", "$0")
        do.return()
"#;
    let json = parse_and_run(gel, "main", "START   hello").expect("sub-grammar with inheritance");
    assert!(json.contains("val"), "inner should execute with inherited skip: {json}");
    assert!(json.contains("hello"), "captured: {json}");
}

// ── 5. Deep chain (4 levels) ────────────────────────────────────────

#[test]
fn deep_chain_four_levels() {
    let gel = r#"
grammar L1:
    skip /\s+/
grammar L2(L1):
    match /A/:
        out.create("root")
        out.add("a", "$0")
grammar L3(L2):
    match /B/:
        out.add("b", "$0")
grammar L4(L3):
    match /C/:
        out.add("c", "$0")
"#;
    let json = parse_and_run(gel, "L4", " A B C").expect("4-level chain");
    assert!(json.contains("\"a\""), "L2 rule via L1→L2→L3→L4: {json}");
    assert!(json.contains("\"b\""), "L3 rule: {json}");
    assert!(json.contains("\"c\""), "L4 rule: {json}");
}
