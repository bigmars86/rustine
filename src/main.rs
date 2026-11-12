use clap::Parser;
use rustine::exec::{execute_tree, serialize_tree_to_writer, RuntimeFormat};
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;
use rustine::parser::validate;
use rustine::GelError;
use std::io::{ErrorKind, Read, Write};
use std::{fs, process};

/// Transform text into JSON, XML or YAML with a Gel grammar.
#[derive(Parser, Debug)]
#[command(name = "rgel", version)]
struct Cli {
    /// The file containing the Gel grammar (syntax) for parsing the input.
    #[arg(short = 's', long = "syntax")]
    syntax: String,

    /// Output format: json, xml, yaml.
    #[arg(short = 'f', long = "format", default_value = "xml")]
    format: String,

    /// Entry-point grammar name inside the .gel file.
    #[arg(short = 'g', long = "grammar", default_value = "input")]
    grammar: String,

    /// Keep the case of tag names and attribute keys in out.* paths (default: lowercase).
    #[arg(long = "preserve-case")]
    preserve_case: bool,

    /// Input files to process. Reads stdin when omitted.
    #[arg(value_name = "FILE")]
    input_files: Vec<String>,
}

fn main() {
    // Deeply nested grammars need more stack than the 1 MB of a Windows main thread.
    let worker = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(cli)
        .expect("start the worker thread");
    if worker.join().is_err() {
        process::exit(101);
    }
}

fn cli() {
    let cli = Cli::parse();

    let runtime_format = match cli.format.as_str() {
        "json" => RuntimeFormat::Json,
        "xml" => RuntimeFormat::Xml,
        "yaml" | "yml" => RuntimeFormat::Yaml,
        other => {
            eprintln!("Error: unknown format '{}' (expected json, xml, yaml)", other);
            process::exit(1);
        }
    };

    // Read and compile grammar once.
    let gel_src = fs::read_to_string(&cli.syntax).unwrap_or_else(|e| {
        eprintln!("Error reading grammar '{}': {}", cli.syntax, e);
        process::exit(1);
    });
    let tokens = lex(&gel_src).unwrap_or_else(|e| {
        eprintln!("Lex error: {}", e);
        process::exit(1);
    });
    let mut doc = parse_gel_document(&tokens).unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        process::exit(1);
    });
    doc.preserve_case = cli.preserve_case;
    for d in validate::validate(&doc) {
        if d.severity == rustine::Severity::Error {
            eprintln!("Validation error: {}", d);
            process::exit(1);
        }
        eprintln!("{}", d);
    }
    doc.compile_regexes();

    let run = |input: &str, source: &str| match execute_tree(&doc, &cli.grammar, input) {
        Ok(exec) => {
            if let Some(err) = &exec.error {
                eprintln!("Runtime error{}: {}", source, err);
                process::exit(1);
            }
            let mut out = std::io::BufWriter::new(std::io::stdout().lock());
            let written = serialize_tree_to_writer(&exec, runtime_format, &mut out)
                .and_then(|()| out.flush().map_err(GelError::from));
            match written {
                Ok(()) => {}
                // The reader stopped early, e.g. `rgel ... | head`.
                Err(GelError::Io(e)) if e.kind() == ErrorKind::BrokenPipe => process::exit(0),
                Err(e) => {
                    eprintln!("Output error{}: {}", source, e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Execution error{}: {}", source, e);
            process::exit(1);
        }
    };

    if cli.input_files.is_empty() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap_or_else(|e| {
            eprintln!("Error reading stdin: {}", e);
            process::exit(1);
        });
        run(&input, "");
    }
    for path in &cli.input_files {
        let input = fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error reading '{}': {}", path, e);
            process::exit(1);
        });
        run(&input, &format!(" in '{}'", path));
    }
}
