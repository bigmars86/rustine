#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use rustine::exec::{execute, serialize_tree, RuntimeFormat};
#[cfg(feature = "cli")]
use rustine::parser::lexer::lex;
#[cfg(feature = "cli")]
use rustine::parser::syntax::parse_gel_document;
#[cfg(feature = "cli")]
use rustine::parser::validate;
#[cfg(feature = "cli")]
use std::{fs, process};

/// High-performance Gel syntax parser — transforms unstructured text into
/// JSON, XML, or YAML.
///
/// Drop-in replacement for the Python `gel` command.
#[cfg(feature = "cli")]
#[derive(Parser, Debug)]
#[command(name = "rgel", version, about)]
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

    /// Input files to process. Reads stdin when omitted.
    #[arg(value_name = "FILE")]
    input_files: Vec<String>,
}

#[cfg(feature = "cli")]
fn main() {
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
    let gel_src = fs::read_to_string(&cli.syntax)
        .unwrap_or_else(|e| {
            eprintln!("Error reading grammar '{}': {}", cli.syntax, e);
            process::exit(1);
        });
    let tokens = lex(&gel_src).unwrap_or_else(|e| {
        eprintln!("Lex error: {}", e);
        process::exit(1);
    });
    let doc = parse_gel_document(&tokens).unwrap_or_else(|e| {
        eprintln!("Parse error: {}", e);
        process::exit(1);
    });
    let diags = validate::validate(&doc);
    for d in &diags {
        if d.severity == rustine::Severity::Error {
            eprintln!("Validation error: {}", d);
            process::exit(1);
        }
        eprintln!("{}", d); // print warnings to stderr
    }

    // If no input files, read from stdin
    if cli.input_files.is_empty() {
        let input = {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                process::exit(1);
            });
            buf
        };
        let mut doc_clone = doc.clone();
        match execute(&mut doc_clone, &cli.grammar, &input) {
            Ok(exec) => {
                if let Some(err) = &exec.error {
                    eprintln!("Runtime error: {}", err);
                }
                print!("{}", serialize_tree(&exec, runtime_format));
            }
            Err(e) => {
                eprintln!("Execution error: {}", e);
                process::exit(1);
            }
        }
    } else {
        // Process each input file
        for input_path in &cli.input_files {
            let input = fs::read_to_string(input_path)
                .unwrap_or_else(|e| {
                    eprintln!("Error reading '{}': {}", input_path, e);
                    process::exit(1);
                });
            let mut doc_clone = doc.clone();
            match execute(&mut doc_clone, &cli.grammar, &input) {
                Ok(exec) => {
                    if let Some(err) = &exec.error {
                        eprintln!("Runtime error in '{}': {}", input_path, err);
                    }
                    print!("{}", serialize_tree(&exec, runtime_format));
                }
                Err(e) => {
                    eprintln!("Execution error in '{}': {}", input_path, e);
                    process::exit(1);
                }
            }
        }
    }
}

// When the cli feature is not enabled, provide an empty main to satisfy the binary but keep it trivial.
#[cfg(not(feature = "cli"))]
fn main() {}
