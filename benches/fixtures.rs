//! Benchmark: dynamically discover and benchmark all `fixtures/parity/*/` dirs.
//!
//! For each fixture directory that contains `syntax1.gel` + `input1.txt`,
//! benchmarks execution and serialisation to JSON, XML, and YAML.
//!
//! Run with:  `cargo bench --bench fixtures --no-default-features`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rustine::exec::{execute_precompiled, serialize_tree, RuntimeFormat};
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::time::Duration;

/// Discover fixture directories under `fixtures/parity/`.
fn discover_fixtures() -> Vec<(String, String, String)> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/parity");
    let mut fixtures = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&base) {
        let mut dirs: Vec<_> = entries.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).collect();
        dirs.sort_by_key(|e| e.file_name());

        for entry in dirs {
            let dir = entry.path();
            let name = dir.file_name().unwrap().to_string_lossy().to_string();

            // Skip "real" fixture — too large for criterion (separate oneshot)
            if name == "real" {
                continue;
            }

            let syntax_path = dir.join("syntax1.gel");
            let input_path = dir.join("input1.txt");
            if syntax_path.exists() && input_path.exists() {
                let syntax = std::fs::read_to_string(&syntax_path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", syntax_path.display()));
                let input = std::fs::read_to_string(&input_path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", input_path.display()));
                fixtures.push((name, syntax, input));
            }
        }
    }
    fixtures
}

fn bench_fixtures(c: &mut Criterion) {
    let fixtures = discover_fixtures();
    if fixtures.is_empty() {
        eprintln!("WARNING: no fixtures found under fixtures/parity/");
        return;
    }

    let formats = [
        ("json", RuntimeFormat::Json),
        ("xml", RuntimeFormat::Xml),
        ("yaml", RuntimeFormat::Yaml),
    ];

    for (name, syntax_src, input) in &fixtures {
        let tokens = lexer::lex(syntax_src).expect("lex");
        let mut doc = parse_gel_document(&tokens).expect("parse");
        doc.compile_regexes();

        let input_bytes = input.len() as u64;

        // Benchmark execution (no serialisation)
        {
            let mut group = c.benchmark_group(format!("fixture/{name}"));
            group.throughput(Throughput::Bytes(input_bytes));
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(5));

            group.bench_function("execute", |b| {
                b.iter(|| {
                    let result = execute_precompiled(&doc, "input", input).expect("execute");
                    std::hint::black_box(result.consumed);
                });
            });
            group.finish();
        }

        // Benchmark execution + serialisation per format
        for (fmt_name, fmt) in &formats {
            let mut group = c.benchmark_group(format!("fixture/{name}"));
            group.throughput(Throughput::Bytes(input_bytes));
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(5));

            group.bench_with_input(BenchmarkId::new("total", *fmt_name), &(*fmt,), |b, (fmt,)| {
                b.iter(|| {
                    let result = execute_precompiled(&doc, "input", input).expect("execute");
                    let out = serialize_tree(&result, *fmt);
                    std::hint::black_box(out.len());
                });
            });
            group.finish();
        }
    }

    // Print peak RSS at the end (informational)
    print_peak_rss("fixtures");
}

/// Print peak RSS (platform-specific).
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
                    let mb = kb as f64 / 1024.0;
                    eprintln!("[{label}] Peak RSS: {mb:.1} MB");
                    break;
                }
            }
        }
    }
}

criterion_group! {
    name = fixture_benches;
    config = Criterion::default()
        .plotting_backend(criterion::PlottingBackend::Plotters);
    targets = bench_fixtures
}
criterion_main!(fixture_benches);
