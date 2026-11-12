//! One-shot scale run: time + peak RSS for 25/50/100 MB inputs.
//!
//! Usage:
//!   cargo run --release --no-default-features --example scale_oneshot
//!   cargo run --release --no-default-features --example scale_oneshot -- --sizes 50,100
//!   cargo run --release --no-default-features --features jemalloc --example scale_oneshot
//!   cargo run --release --no-default-features --example scale_oneshot -- --write data --sizes 25

use rustine::exec::{execute_tree, serialize_tree, RuntimeFormat};
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::time::Instant;

#[path = "../benches/support/scale_data.rs"]
mod scale_data;
use scale_data::{generate_grammar, generate_input};

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sizes: Vec<usize> = args
        .iter()
        .position(|a| a == "--sizes")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![25, 50, 100]);

    // --write DIR saves the grammar and inputs (scale.gel, scale-<MB>mb.txt) instead of running them.
    if let Some(dir) = args.iter().position(|a| a == "--write").and_then(|i| args.get(i + 1)) {
        std::fs::create_dir_all(dir).expect("create output dir");
        std::fs::write(format!("{dir}/scale.gel"), generate_grammar()).expect("write grammar");
        for &mb in &sizes {
            std::fs::write(format!("{dir}/scale-{mb}mb.txt"), generate_input(mb * 1_048_576)).expect("write input");
        }
        return;
    }

    eprintln!("=== Rustine Scale One-Shot ===");
    eprintln!("Sizes: {:?} MB\n", sizes);

    // Compile grammar once
    let grammar_src = generate_grammar();
    let tokens = lexer::lex(&grammar_src).expect("lex");
    let mut doc = parse_gel_document(&tokens).expect("parse");
    doc.compile_regexes();
    eprintln!(
        "Grammar: {} bytes ({} grammars)\n",
        grammar_src.len(),
        doc.grammars.len()
    );

    let formats: [(&str, RuntimeFormat); 3] = [
        ("json", RuntimeFormat::Json),
        ("xml", RuntimeFormat::Xml),
        ("yaml", RuntimeFormat::Yaml),
    ];

    eprintln!(
        "{:>6} | {:>10} | {:>7} | {:>10} | {:>10} | {:>8} | {:>8}",
        "Size", "Format", "Nodes", "Exec", "Serialize", "Total", "Peak RSS"
    );
    eprintln!("{}", "-".repeat(82));

    for &mb in &sizes {
        let target = mb * 1_048_576;
        let input = generate_input(target);
        let input_mb = input.len() as f64 / 1_048_576.0;

        for (fmt_name, fmt) in &formats {
            let t0 = Instant::now();
            let result = execute_tree(&doc, "input", &input).expect("execute");
            let exec_time = t0.elapsed();

            let node_count = result.flat.as_ref().map_or(0, |f| f.len());

            let t1 = Instant::now();
            let output = serialize_tree(&result, *fmt);
            let ser_time = t1.elapsed();

            let total = exec_time + ser_time;
            let rss = peak_rss_mb();

            eprintln!(
                "{:>4} MB | {:>10} | {:>7} | {:>9.3}s | {:>9.3}s | {:>7.3}s | {:>6.1} MB",
                mb,
                fmt_name,
                node_count,
                exec_time.as_secs_f64(),
                ser_time.as_secs_f64(),
                total.as_secs_f64(),
                rss
            );
            eprintln!(
                "       |            |         | {:>8.2} MB/s | {:>6} bytes out",
                input_mb / total.as_secs_f64(),
                output.len()
            );

            // drop output early to reduce RSS for next iteration
            drop(output);
            drop(result);
        }
        eprintln!("{}", "-".repeat(82));
    }
}
