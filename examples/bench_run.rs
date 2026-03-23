//! Benchmark any .gel grammar + input file.
//!
//! Usage:
//!   cargo run --release --no-default-features --example bench_run -- <syntax.gel> <input.txt> [iterations]
//!   cargo run --release --no-default-features --example bench_run -- my.gel data.txt 5
//!   cargo run --release --no-default-features --example bench_run -- my.gel data.txt 5 --dump
//!   cargo run --release --no-default-features --example bench_run -- my.gel data.txt 5 --stream

use rustine::exec::{execute_precompiled, serialize_tree, serialize_tree_to_writer, RuntimeFormat};
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("bench_run — Benchmark a .gel grammar against an input file");
        eprintln!();
        eprintln!("Usage:");
        eprintln!("  cargo run --release --no-default-features --example bench_run -- [OPTIONS] <syntax.gel> <input.txt> [iterations]");
        eprintln!();
        eprintln!("Arguments:");
        eprintln!("  <syntax.gel>    Path to the Gel grammar file");
        eprintln!("  <input.txt>     Path to the input text file");
        eprintln!("  [iterations]    Number of iterations (default: 3)");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --dump          Print the full JSON output after benchmarking");
        eprintln!("  --diag          Run in diagnostic envelope mode (serialize_execution)");
        eprintln!("  --stream        Use the streaming writer for serialization");
        eprintln!("  -h, --help      Show this help message");
        if args.iter().any(|a| a == "-h" || a == "--help") {
            std::process::exit(0);
        }
        std::process::exit(1);
    }
    let syntax_path = &args[1];
    let input_path = &args[2];
    let iterations: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
    let dump_output = args.iter().any(|a| a == "--dump");
    let diag_mode = args.iter().any(|a| a == "--diag");
    let stream_mode = args.iter().any(|a| a == "--stream");

    let grammar_src = std::fs::read_to_string(syntax_path).unwrap_or_else(|e| {
        eprintln!("Cannot read {syntax_path}: {e}");
        std::process::exit(1);
    });
    let input = std::fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Cannot read {input_path}: {e}");
        std::process::exit(1);
    });

    let input_mb = input.len() as f64 / 1_048_576.0;
    eprintln!(
        "Grammar: {} bytes, Input: {} bytes ({:.1} MB)",
        grammar_src.len(),
        input.len(),
        input_mb
    );
    eprintln!("Iterations: {}\n", iterations);

    // Compile grammar once
    let tokens = lexer::lex(&grammar_src).expect("lexer error");
    let mut doc = parse_gel_document(&tokens).expect("parse error");
    doc.compile_regexes();
    eprintln!(
        "Grammar compiled ({} grammars, {} defines, {} regex patterns)",
        doc.grammars.len(),
        doc.defines.len(),
        doc.regex_patterns.len()
    );

    let mut times_exec = Vec::with_capacity(iterations);
    let mut times_total = Vec::with_capacity(iterations);
    let mut node_count = 0usize;

    for i in 0..iterations {
        // Execute
        let t0 = Instant::now();
        let result = execute_precompiled(&doc, "input", &input).expect("execution error");
        let exec_time = t0.elapsed();

        if i == 0 {
            eprintln!("  consumed: {} / {} bytes", result.consumed, input.len());
            eprintln!("  error: {:?}", result.error);
            let flat_nodes = result.flat.as_ref().map(|f| f.nodes.len()).unwrap_or(0);
            eprintln!("  flat tree nodes: {}", flat_nodes);
            if !diag_mode {
                eprintln!("  traces (last 20):");
                for t in result.traces.iter().rev().take(20).rev() {
                    eprintln!("    {}", t);
                }
            }
        }

        if diag_mode && i == 0 {
            // Memory diagnostics
            eprintln!("\n=== Memory Diagnostics ===");
            eprintln!("  actions: {} items", result.actions.len());
            eprintln!("  traces: {} items", result.traces.len());
            let traces_bytes: usize = result
                .traces
                .iter()
                .map(|t| t.len() + std::mem::size_of::<String>())
                .sum();
            eprintln!(
                "  traces total: {} bytes ({:.1} MB)",
                traces_bytes,
                traces_bytes as f64 / 1_048_576.0
            );
            eprintln!("  capture_history: {} items", result.capture_history.len());
            let cap_bytes: usize = result
                .capture_history
                .iter()
                .map(|v| {
                    std::mem::size_of::<Vec<String>>()
                        + v.iter().map(|s| std::mem::size_of::<String>() + s.len()).sum::<usize>()
                })
                .sum();
            eprintln!(
                "  capture_history total: {} bytes ({:.1} MB)",
                cap_bytes,
                cap_bytes as f64 / 1_048_576.0
            );
            eprintln!("  capture_names_history: {} items", result.capture_names_history.len());
            let cap_names_bytes: usize = result
                .capture_names_history
                .iter()
                .map(|v| {
                    std::mem::size_of::<Vec<Option<std::sync::Arc<str>>>>()
                        + v.len() * std::mem::size_of::<Option<std::sync::Arc<str>>>()
                })
                .sum();
            eprintln!(
                "  capture_names_history total: {} bytes ({:.1} MB)",
                cap_names_bytes,
                cap_names_bytes as f64 / 1_048_576.0
            );
            eprintln!("  diagnostics: {} items", result.diagnostics.len());
            let diag_bytes: usize = result
                .diagnostics
                .iter()
                .map(|d| std::mem::size_of_val(d) + d.message.len())
                .sum();
            eprintln!(
                "  diagnostics total: {} bytes ({:.1} MB)",
                diag_bytes,
                diag_bytes as f64 / 1_048_576.0
            );

            // FlatTree analysis
            if let Some(ref flat) = result.flat {
                eprintln!("  flat nodes: {}", flat.nodes.len());
                let flat_node_size = std::mem::size_of::<rustine::exec::out::FlatNode>();
                eprintln!("  FlatNode size: {} bytes", flat_node_size);
                eprintln!(
                    "  flat nodes shallow: {} bytes ({:.1} MB)",
                    flat.nodes.len() * flat_node_size,
                    (flat.nodes.len() * flat_node_size) as f64 / 1_048_576.0
                );

                // Pooled attributes analysis
                let attr_count = flat.attrs.len();
                let attr_pool_overhead = flat.attrs.capacity() * std::mem::size_of::<(std::rc::Rc<str>, String)>();
                let mut attr_str_bytes = 0usize;
                for (k, v) in &flat.attrs {
                    attr_str_bytes += k.len() + v.len();
                }
                eprintln!(
                    "  flat attr pool: {} pairs, pool capacity overhead: {} bytes ({:.1} MB)",
                    attr_count,
                    attr_pool_overhead,
                    attr_pool_overhead as f64 / 1_048_576.0
                );
                eprintln!(
                    "  flat attr string content: {} bytes ({:.1} MB)",
                    attr_str_bytes,
                    attr_str_bytes as f64 / 1_048_576.0
                );

                let mut text_count = 0usize;
                let mut text_bytes = 0usize;
                let mut name_bytes = 0usize;
                for n in &flat.nodes {
                    name_bytes += n.name.len();
                    if let Some(ref t) = n.text {
                        text_count += 1;
                        text_bytes += t.len() + std::mem::size_of::<String>();
                    }
                }
                eprintln!(
                    "  flat text nodes: {}, text bytes: {} ({:.1} MB)",
                    text_count,
                    text_bytes,
                    text_bytes as f64 / 1_048_576.0
                );
                eprintln!("  flat name unique bytes (non-interned): {} bytes", name_bytes);
            }

            // OutputTree (should be mostly empty after flatten)
            eprintln!("  output.root children: {}", result.output.root_child_count());

            eprintln!("  Peak RSS after 1st iter: {:.1} MB", peak_rss_mb());
            eprintln!("=== End Diagnostics ===\n");
        }

        // Serialize JSON (execute_precompiled already did compact + FlatTree)
        let t1 = Instant::now();
        let json_len;
        if stream_mode {
            // Streaming: write directly to a counting sink (no in-memory String)
            let mut counter = ByteCounter(0);
            serialize_tree_to_writer(&result, RuntimeFormat::Json, &mut counter).expect("streaming serialize error");
            json_len = counter.0;
        } else {
            let json = serialize_tree(&result, RuntimeFormat::Json);
            json_len = json.len();

            if dump_output && i == 0 {
                use std::io::Write;
                std::io::stdout().write_all(json.as_bytes()).unwrap();
            }
        }
        let post_time = t1.elapsed();

        node_count = result.flat.as_ref().map(|f| f.nodes.len()).unwrap_or(0);
        let total = exec_time + post_time;

        times_exec.push(exec_time);
        times_total.push(total);

        eprintln!("[{}/{}] exec: {:.3}s  post: {:.3}s  total: {:.3}s  ({:.2} MB/s exec, {:.2} MB/s total)  nodes: {}  json: {} bytes{}",
            i + 1, iterations,
            exec_time.as_secs_f64(),
            post_time.as_secs_f64(),
            total.as_secs_f64(),
            input_mb / exec_time.as_secs_f64(),
            input_mb / total.as_secs_f64(),
            node_count,
            json_len,
            if stream_mode { "  [stream]" } else { "" },
        );
    }

    // Summary: median
    times_exec.sort();
    times_total.sort();
    let mid = iterations / 2;
    let med_exec = times_exec[mid];
    let med_total = times_total[mid];

    eprintln!(
        "\n=== Median of {} iterations{} ===",
        iterations,
        if stream_mode { " (streaming)" } else { "" }
    );
    eprintln!("  Input:     {:.1} MB ({} bytes)", input_mb, input.len());
    eprintln!("  Nodes:     {}", node_count);
    eprintln!(
        "  Exec:      {:.3}s  ({:.2} MB/s)",
        med_exec.as_secs_f64(),
        input_mb / med_exec.as_secs_f64()
    );
    eprintln!(
        "  Total:     {:.3}s  ({:.2} MB/s)",
        med_total.as_secs_f64(),
        input_mb / med_total.as_secs_f64()
    );
    eprintln!("  Peak RSS:  {:.1} MB", peak_rss_mb());
}

fn peak_rss_mb() -> f64 {
    #[cfg(target_os = "windows")]
    {
        use std::mem::MaybeUninit;
        #[repr(C)]
        struct Pmc {
            cb: u32,
            pf: u32,
            peak_wss: usize,
            wss: usize,
            _r: [usize; 6],
        }
        extern "system" {
            fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
            fn GetCurrentProcess() -> isize;
        }
        unsafe {
            let mut c = MaybeUninit::<Pmc>::zeroed().assume_init();
            c.cb = std::mem::size_of::<Pmc>() as u32;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) != 0 {
                return c.peak_wss as f64 / 1_048_576.0;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmHWM:") {
                    let kb: usize = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
                    return kb as f64 / 1024.0;
                }
            }
        }
    }
    0.0
}

/// Byte-counting writer — counts bytes written without keeping the output in memory.
/// Used with `--stream` to measure serialization cost while avoiding the ~98 MB String allocation.
struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
