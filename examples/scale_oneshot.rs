//! One-shot scale run: time + peak RSS for 25/50/100 MB inputs.
//!
//! Usage:
//!   cargo run --release --no-default-features --example scale_oneshot
//!   cargo run --release --no-default-features --example scale_oneshot -- --sizes 50,100
//!   cargo run --release --no-default-features --features jemalloc --example scale_oneshot

use rustine::exec::{execute_precompiled, serialize_tree, RuntimeFormat};
use rustine::parser::lexer;
use rustine::parser::syntax::parse_gel_document;
use std::fmt::Write;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Constants (match scale bench + Python bench/bench_scale.py)
// ---------------------------------------------------------------------------
const NUM_SECTIONS: usize = 30;
const SUBS_PER_SECTION: usize = 9;
const ATTRS_PER_SUB: usize = 3;

fn generate_grammar() -> String {
    let mut s = String::with_capacity(100_000);
    s.push_str("define fs /[\\t ]+/\ndefine ws /\\s+/\ndefine nl /[\\r\\n]+/\ndefine nonl /[^\\r\\n]+/\ndefine nows /\\S+/\ndefine number /\\d+/\ndefine word /\\w+/\ndefine name /[\\w][\\w\\-]*/\ndefine ipaddr /\\d+\\.\\d+\\.\\d+\\.\\d+/\ndefine comment /[!#][^\\r\\n]*[\\r\\n]/\n\n");
    s.push_str("grammar default:\n    skip fs\n    skip comment\n\n");
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

fn generate_input(target_bytes: usize) -> String {
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
            let result = execute_precompiled(&doc, "input", &input).expect("execute");
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
