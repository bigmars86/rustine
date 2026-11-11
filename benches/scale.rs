//! Scale benchmark: 302-grammar × 25 MB generated input.
//!
//! Grammar and input are generated dynamically — no external files.
//! Benchmarks execution and serialisation to JSON, XML, YAML.
//!
//! Run with:  `cargo bench --bench scale --no-default-features`
//!
//! For 50 MB / 100 MB one-shot runs, use the `scale_oneshot` example instead.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use rustine::exec::{execute_precompiled, serialize_tree, RuntimeFormat};
use rustine::parser::ast::GelDocument;
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::time::Duration;

#[path = "support/scale_data.rs"]
mod scale_data;
use scale_data::{generate_grammar, generate_input};

fn compile_grammar(src: &str) -> GelDocument {
    let tokens = lexer::lex(src).expect("lex grammar");
    let mut doc = parse_gel_document(&tokens).expect("parse grammar");
    doc.compile_regexes();
    doc
}

fn print_peak_rss(label: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::mem::MaybeUninit;
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            _rest: [usize; 6],
        }
        extern "system" {
            fn K32GetProcessMemoryInfo(process: isize, ppsmemcounters: *mut ProcessMemoryCounters, cb: u32) -> i32;
            fn GetCurrentProcess() -> isize;
        }
        unsafe {
            let mut counters = MaybeUninit::<ProcessMemoryCounters>::zeroed().assume_init();
            counters.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
                let mb = counters.peak_working_set_size as f64 / 1_048_576.0;
                eprintln!("[{label}] Peak RSS: {mb:.1} MB");
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmHWM:") {
                    let kb: usize = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
                    eprintln!("[{label}] Peak RSS: {:.1} MB", kb as f64 / 1024.0);
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_scale(c: &mut Criterion) {
    let grammar_src = generate_grammar();
    let doc = compile_grammar(&grammar_src);
    let input = generate_input(25 * 1_048_576);

    let input_bytes = input.len() as u64;
    eprintln!(
        "[scale] Input: {:.1} MB, Grammar: {} bytes",
        input.len() as f64 / 1_048_576.0,
        grammar_src.len()
    );

    // Execute-only
    {
        let mut group = c.benchmark_group("scale");
        group.throughput(Throughput::Bytes(input_bytes));
        group.warm_up_time(Duration::from_secs(5));
        group.measurement_time(Duration::from_secs(60));
        group.sample_size(10);
        group.bench_function("execute_25MB", |b| {
            b.iter(|| {
                let result = execute_precompiled(&doc, "input", &input).expect("execute");
                std::hint::black_box(result.consumed);
            });
        });
        group.finish();
    }

    // Pre-execute for serialisation benchmarks
    let result = execute_precompiled(&doc, "input", &input).expect("execute");
    let node_count = result.flat.as_ref().map_or(0, |f| f.len());
    eprintln!("[scale] Tree nodes: {node_count}");

    let formats: [(&str, RuntimeFormat); 3] = [
        ("json", RuntimeFormat::Json),
        ("xml", RuntimeFormat::Xml),
        ("yaml", RuntimeFormat::Yaml),
    ];

    // Serialise-only per format
    for (fmt_name, fmt) in &formats {
        let mut group = c.benchmark_group("scale");
        group.throughput(Throughput::Elements(node_count as u64));
        group.warm_up_time(Duration::from_secs(3));
        group.measurement_time(Duration::from_secs(30));
        group.sample_size(10);
        group.bench_function(format!("serialize_{fmt_name}_25MB"), |b| {
            b.iter(|| {
                let out = serialize_tree(&result, *fmt);
                std::hint::black_box(out.len());
            });
        });
        group.finish();
    }

    // Total (execute + serialize) per format
    for (fmt_name, fmt) in &formats {
        let mut group = c.benchmark_group("scale");
        group.throughput(Throughput::Bytes(input_bytes));
        group.warm_up_time(Duration::from_secs(5));
        group.measurement_time(Duration::from_secs(60));
        group.sample_size(10);
        group.bench_function(format!("total_{fmt_name}_25MB"), |b| {
            b.iter(|| {
                let res = execute_precompiled(&doc, "input", &input).expect("execute");
                let out = serialize_tree(&res, *fmt);
                std::hint::black_box(out.len());
            });
        });
        group.finish();
    }

    print_peak_rss("scale_25MB");
}

criterion_group! {
    name = scale_benches;
    config = Criterion::default().plotting_backend(criterion::PlottingBackend::Plotters);
    targets = bench_scale
}
criterion_main!(scale_benches);
