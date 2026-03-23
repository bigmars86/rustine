//! Benchmark: execution engine micro-benchmarks.
//!
//! Covers grammar scaling, serialization formats, capture-heavy workloads,
//! deep output trees, and arena allocation.
//!
//! Run with:  `cargo bench --bench exec --no-default-features`

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use rustine::exec::{execute, serialize_execution, RuntimeFormat};
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::fmt::Write;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Grammar + Input generators for scaling benchmarks
// ---------------------------------------------------------------------------

/// Generate a grammar with `num_rules` match rules.
///
/// Each rule matches `keyN= <value>` and creates a node via `out.add`.
fn generate_grammar(num_rules: usize) -> String {
    let mut s = String::with_capacity(num_rules * 100);
    s.push_str("define fs /[\\t ]+/\n");
    s.push_str("define ws /\\s+/\n");
    s.push_str("define nl /[\\r\\n]+/\n");
    s.push_str("define nows /\\S+/\n\n");
    s.push_str("grammar default:\n");
    s.push_str("    skip fs\n\n");
    s.push_str("grammar input(default):\n");
    for i in 0..num_rules {
        let _ = write!(
            s,
            "    match 'key{}=' fs nows nl:\n        out.add('key{}', '$2')\n",
            i, i,
        );
    }
    s
}

/// Generate input with `num_lines` lines cycling through `num_rules` keys.
fn generate_input(num_rules: usize, num_lines: usize) -> String {
    let mut s = String::with_capacity(num_lines * 30);
    for i in 0..num_lines {
        let key = i % num_rules;
        let _ = writeln!(s, "key{}= value{}", key, i);
    }
    s
}

// ---------------------------------------------------------------------------
// Scaling benchmarks
// ---------------------------------------------------------------------------

fn bench_scaling(c: &mut Criterion) {
    // Head-to-head configs (match PERFORMANCE_ANALYSIS.md)
    // plus scaling-characteristic configs (constant 1000 lines, varying rules).
    let configs: &[(usize, usize, &str)] = &[
        (5, 100, "5_rules_100_lines"),
        (5, 1_000, "5_rules_1000_lines"),
        (10, 1_000, "10_rules_1000_lines"),
        (20, 1_000, "20_rules_1000_lines"),
        (50, 1_000, "50_rules_1000_lines"),
        (50, 10_000, "50_rules_10000_lines"),
    ];

    for &(rules, lines, label) in configs {
        let grammar_src = generate_grammar(rules);
        let input = generate_input(rules, lines);
        let tokens = lexer::lex(&grammar_src).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");

        let mut group = c.benchmark_group("scaling");
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(5));

        group.bench_function(label, |b| {
            b.iter(|| {
                let result = execute(&mut doc, "input", &input).expect("execute");
                std::hint::black_box(result.consumed);
            });
        });
        group.finish();
    }
}

// ---------------------------------------------------------------------------
// Serialization benchmarks (1000-node tree)
// ---------------------------------------------------------------------------

fn bench_serialization(c: &mut Criterion) {
    let grammar_src = generate_grammar(20);
    let input = generate_input(20, 1_000);
    let tokens = lexer::lex(&grammar_src).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    let result = execute(&mut doc, "input", &input).expect("execute");

    let formats: &[(&str, RuntimeFormat)] = &[
        ("json", RuntimeFormat::Json),
        ("xml", RuntimeFormat::Xml),
        ("yaml", RuntimeFormat::Yaml),
    ];

    let mut group = c.benchmark_group("serialize");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));

    for &(name, fmt) in formats {
        group.bench_function(name, |b| {
            b.iter(|| {
                let out = serialize_execution(&result, fmt);
                std::hint::black_box(out.len());
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Capture-heavy benchmark (5000 lines with heavy regex captures)
// ---------------------------------------------------------------------------

fn bench_capture_heavy(c: &mut Criterion) {
    // Grammar with 3 key=value pairs per line → 12 capture fields per match
    let mut grammar_src = String::with_capacity(1_000);
    grammar_src.push_str("define fs /[\\t ]+/\n");
    grammar_src.push_str("define ws /\\s+/\n");
    grammar_src.push_str("define nl /[\\r\\n]+/\n");
    grammar_src.push_str("define nows /\\S+/\n");
    grammar_src.push_str("define word /\\w+/\n\n");
    grammar_src.push_str("grammar default:\n");
    grammar_src.push_str("    skip fs\n\n");
    grammar_src.push_str("grammar input(default):\n");
    grammar_src.push_str("    match word '=' nows fs word '=' nows fs word '=' nows nl:\n");
    grammar_src.push_str("        out.add('$0', '$2')\n");
    grammar_src.push_str("        out.add('$4', '$6')\n");
    grammar_src.push_str("        out.add('$8', '$10')\n");

    let mut input = String::with_capacity(5_000 * 60);
    for i in 0..5_000 {
        let _ = writeln!(input, "alpha=val{} beta=data{} gamma=info{}", i, i * 2, i * 3,);
    }

    let tokens = lexer::lex(&grammar_src).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");

    let mut group = c.benchmark_group("capture");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    group.bench_function("heavy_5000", |b| {
        b.iter(|| {
            let result = execute(&mut doc, "input", &input).expect("execute");
            std::hint::black_box(result.consumed);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Output tree depth benchmark
// ---------------------------------------------------------------------------

fn bench_output_tree_deep(c: &mut Criterion) {
    // Build a grammar that creates deeply nested output paths.
    // Each match writes to a 50-level-deep path, exercising the
    // output tree construction and path-traversal hot path.
    let depth = 50;
    let num_items = 200;

    let path_segments: Vec<String> = (0..depth).map(|d| format!("level{}", d)).collect();
    let deep_path = path_segments.join("/");

    let mut grammar_src = String::with_capacity(2_000);
    grammar_src.push_str("define fs /[\\t ]+/\n");
    grammar_src.push_str("define ws /\\s+/\n");
    grammar_src.push_str("define nl /[\\r\\n]+/\n");
    grammar_src.push_str("define nows /\\S+/\n\n");
    grammar_src.push_str("grammar default:\n");
    grammar_src.push_str("    skip fs\n\n");
    grammar_src.push_str("grammar input(default):\n");
    let _ = write!(
        grammar_src,
        "    match nows fs nows nl:\n        out.create('{}/item_$0', '$2')\n",
        deep_path,
    );

    let mut input = String::with_capacity(num_items * 30);
    for i in 0..num_items {
        let _ = writeln!(input, "key{} value{}", i, i);
    }

    let tokens = lexer::lex(&grammar_src).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");

    let mut group = c.benchmark_group("output_tree");
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("deep", |b| {
        b.iter(|| {
            let result = execute(&mut doc, "input", &input).expect("execute");
            std::hint::black_box(result.consumed);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Arena allocation benchmarks
// ---------------------------------------------------------------------------

use rustine::exec::arena::ExecArena;

fn bench_arena(c: &mut Criterion) {
    let mut group = c.benchmark_group("arena");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("alloc_str_1000", |b| {
        b.iter(|| {
            let mut arena = ExecArena::new();
            for i in 0..1_000 {
                let s = format!("some_temporary_string_{}", i);
                let allocated = arena.alloc_str(&s);
                std::hint::black_box(allocated);
            }
            arena.reset();
        });
    });

    group.bench_function("vs_vec_1000", |b| {
        b.iter(|| {
            let mut v: Vec<String> = Vec::with_capacity(1_000);
            for i in 0..1_000 {
                v.push(format!("some_temporary_string_{}", i));
            }
            std::hint::black_box(&v);
            drop(v);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_scaling,
    bench_serialization,
    bench_capture_heavy,
    bench_output_tree_deep,
    bench_arena,
);
criterion_main!(benches);
