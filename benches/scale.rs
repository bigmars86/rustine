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
use std::fmt::Write;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Constants (must match Python bench/bench_scale.py)
// ---------------------------------------------------------------------------

const NUM_SECTIONS: usize = 30;
const SUBS_PER_SECTION: usize = 9;
const ATTRS_PER_SUB: usize = 3;

// ---------------------------------------------------------------------------
// Grammar generator (deterministic, ~91 KB)
// ---------------------------------------------------------------------------

pub fn generate_grammar() -> String {
    let mut s = String::with_capacity(100_000);
    s.push_str("define fs /[\\t ]+/\n");
    s.push_str("define ws /\\s+/\n");
    s.push_str("define nl /[\\r\\n]+/\n");
    s.push_str("define nonl /[^\\r\\n]+/\n");
    s.push_str("define nows /\\S+/\n");
    s.push_str("define number /\\d+/\n");
    s.push_str("define word /\\w+/\n");
    s.push_str("define name /[\\w][\\w\\-]*/\n");
    s.push_str("define ipaddr /\\d+\\.\\d+\\.\\d+\\.\\d+/\n");
    s.push_str("define comment /[!#][^\\r\\n]*[\\r\\n]/\n\n");
    s.push_str("grammar default:\n");
    s.push_str("    skip fs\n");
    s.push_str("    skip comment\n\n");

    for sect in 0..NUM_SECTIONS {
        for sub in 0..SUBS_PER_SECTION {
            let pat = (sect * SUBS_PER_SECTION + sub) % 6;
            let _ = writeln!(s, "grammar s{sect:02}_{sub:02}(default):");
            match pat {
                0 => {
                    for a in 0..ATTRS_PER_SUB {
                        let _ = write!(s, "    match 'f{a}' fs nows nl:\n        out.add('f{a}', '$2')\n");
                    }
                }
                1 => {
                    s.push_str("    match 'f0' fs nows fs nows nl:\n        out.add('f0?type=\"$4\"', '$2')\n");
                    for a in 1..ATTRS_PER_SUB {
                        let _ = write!(s, "    match 'f{a}' fs nows nl:\n        out.add('f{a}', '$2')\n");
                    }
                }
                2 => {
                    for a in 0..ATTRS_PER_SUB {
                        let _ = write!(
                            s,
                            "    match 'f{a}' fs nows nl:\n        out.add('detail/f{a}', '$2')\n"
                        );
                    }
                }
                3 => {
                    s.push_str("    match 'f0' fs nows fs nows nl:\n        out.add('f0?src=\"$2\"&dst=\"$4\"')\n    match 'f1' fs nows fs number nl:\n        out.add('f1?name=\"$2\"&id=\"$4\"')\n    match 'f2' fs nows nl:\n        out.add('f2', '$2')\n");
                }
                4 => {
                    s.push_str("    match 'f0' fs nows fs nows nl:\n        out.open('f0?id=\"$2\"')\n        out.add_attribute('.', 'value', '$4')\n    match 'f1' fs nows nl:\n        out.add('f1', '$2')\n    match 'f2' fs number nl:\n        out.add('f2', '$2')\n");
                }
                5 => {
                    s.push_str("    match 'f0' fs nows nl | 'g0' fs nows nl:\n        out.add('f0', '$2')\n    match 'f1' fs nows nl:\n        out.add('f1', '$2')\n    match 'f2' fs nows nl:\n        out.add('f2', '$2')\n");
                }
                _ => unreachable!(),
            }
            s.push_str("    do.return()\n\n");
        }
    }
    for sect in 0..NUM_SECTIONS {
        let _ = writeln!(s, "grammar s{sect:02}_dispatch(default):");
        for sub in 0..SUBS_PER_SECTION {
            let _ = write!(s, "    match 'g{sub:02}' fs name nl:\n        out.enter('g{sub:02}?name=\"$2\"')\n        s{sect:02}_{sub:02}()\n");
        }
        s.push_str("    do.return()\n\n");
    }
    s.push_str("grammar input(default):\n");
    for sect in 0..NUM_SECTIONS {
        let _ = write!(
            s,
            "    match 'sect{sect:02}' nl:\n        out.enter('sect{sect:02}')\n        s{sect:02}_dispatch()\n"
        );
    }
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// Input generator (deterministic)
// ---------------------------------------------------------------------------

pub fn generate_input(target_bytes: usize) -> String {
    let mut s = String::with_capacity(target_bytes + 4096);
    let mut block_id: usize = 0;
    while s.len() < target_bytes {
        let sect = block_id % NUM_SECTIONS;
        let _ = writeln!(s, "sect{sect:02}");
        let num_subs = 3 + (block_id % (SUBS_PER_SECTION - 2));
        for sub_idx in 0..num_subs {
            let sub = sub_idx % SUBS_PER_SECTION;
            let pat = (sect * SUBS_PER_SECTION + sub) % 6;
            let _ = writeln!(s, " g{sub:02} inst-{block_id:06}-{sub:02}");
            match pat {
                0 => {
                    for a in 0..ATTRS_PER_SUB {
                        let _ = writeln!(s, "  f{a} v{}-{sub}-{a}", block_id % 10000);
                    }
                }
                1 => {
                    let _ = writeln!(
                        s,
                        "  f0 {}.{sub}.0.{} t{}",
                        10 + block_id % 200,
                        block_id & 0xFF,
                        block_id % 8
                    );
                    for a in 1..ATTRS_PER_SUB {
                        let _ = writeln!(s, "  f{a} v{}-{sub}-{a}", block_id % 10000);
                    }
                }
                2 => {
                    for a in 0..ATTRS_PER_SUB {
                        let _ = writeln!(s, "  f{a} mp-{block_id}-{sub}-{a}");
                    }
                }
                3 => {
                    let _ = writeln!(s, "  f0 src-{} dst-{sub}", block_id % 1000);
                    let _ = writeln!(s, "  f1 name-{} {}", block_id % 500, block_id * 10 + sub);
                    let _ = writeln!(s, "  f2 val-{block_id}-{sub}");
                }
                4 => {
                    let _ = writeln!(s, "  f0 id-{block_id:06} data-{sub}");
                    let _ = writeln!(s, "  f1 state-{}", block_id % 3);
                    let _ = writeln!(s, "  f2 {}", block_id * 100 + sub);
                }
                5 => {
                    let key = if (block_id + sub) % 2 == 0 { "g0" } else { "f0" };
                    let _ = writeln!(s, "  {key} alt-{block_id}-{sub}");
                    for a in 1..ATTRS_PER_SUB {
                        let _ = writeln!(s, "  f{a} x{block_id}-{sub}-{a}");
                    }
                }
                _ => unreachable!(),
            }
        }
        s.push_str("!\n");
        block_id += 1;
    }
    s
}

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
