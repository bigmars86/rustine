# Getting Started

## Installation

### Python (recommended)

```bash
pip install rustine
```

Pre-built wheels are available for:

| OS      | Architectures         |
|---------|-----------------------|
| Linux   | x86_64, aarch64       |
| macOS   | x86_64, Apple Silicon |
| Windows | x86_64                |

### From Source (Python + Rust)

Requires Rust 1.70+ and Python 3.9+.

```bash
git clone https://github.com/bigmars86/rustine.git
cd rustine
pip install maturin
maturin develop --release
```

### Rust Library

Add to your `Cargo.toml`:

```toml
[dependencies]
rustine = { git = "https://github.com/bigmars86/rustine.git", default-features = false }
```

### CLI Tool

```bash
cargo install rustine --features cli
```

Or build from source:

```bash
cargo build --release --features cli --no-default-features
# Binary at target/release/rgel
```

### Cargo Features

Rustine uses **Cargo features** — compile-time flags that control which
functionality is included in the binary.  They are **not** runtime
configuration; you choose them once when you build and the resulting
binary either has the capability or not.

Enable features with `--features`:

```bash
# CLI binary
cargo build --release --no-default-features --features cli

# CLI + jemalloc (Linux)
cargo build --release --no-default-features --features "cli,jemalloc"

# Multiple features as a library dependency in Cargo.toml:
# rustine = { git = "...", default-features = false, features = ["mmap"] }
```

| Feature     | Default        | Description                                           |
|-------------|----------------|-------------------------------------------------------|
| `python`    | **yes**        | PyO3 extension module (`.pyd` / `.so`)                |
| `cli`       | no             | Build the `rgel` command-line binary                  |
| `mmap`      | no             | Memory-mapped file I/O for `ChunkReader`              |
| `parallel`  | no             | Parallel grammar compilation via Rayon                |
| `jemalloc`  | **Linux wheels** | jemalloc allocator (~20 % faster, better RSS)       |
| `mimalloc`  | no             | mimalloc allocator (cross-platform alternative)       |
| `bench`     | no             | Used by internal benchmark examples                   |
| `profiling` | no             | Emit detailed per-rule timing counters (dev use)      |

> **jemalloc on Linux:** Published Linux wheels (`pip install rustine`)
> include jemalloc by default.  It returns freed memory to the OS via
> `madvise(MADV_DONTNEED)`, giving ~20 % faster execution and
> significantly lower steady-state RSS — important for long-running
> workers (Celery, Gunicorn, etc.).  macOS and Windows wheels use the
> system allocator.  To build without jemalloc, compile from source
> with `--features python` only.

> **Tip — `--no-default-features`:** The default feature set includes
> `python`, which requires a Python interpreter at build time.  When you
> only need the Rust library or CLI, pass `--no-default-features` to skip
> the PyO3 dependency.

---

## Python Usage

### Basic Parsing

```python
from Rustine import rustine

grammar = r"""
define nl /\r?\n/
define ws /\s+/

grammar input:
    match 'Name:' ws /[^\r\n,]+/ /(?:\r?\n|,) */:
        out.open('user')
        out.add_attribute('.', 'name', '$2')
    match /[\w ]+/ ':' ws /[^\r\n,]+/ /(?:\r?\n|,) */:
        out.add('$0', '$3')
    match nl:
        do.return()
"""

input_text = "Name: Alice\nAge: 30\nOffice: 1st Ave\n"

# Output as JSON, XML, or YAML
print(rustine.parse_to_json(grammar, input_text))
print(rustine.parse_to_xml(grammar, input_text))
print(rustine.parse_to_yaml(grammar, input_text))
```

### Full Execution Results

For access to consumed bytes, traces, diagnostics, captures, and the
output tree:

```python
# Returns JSON with all execution metadata
result = rustine.run_grammar(grammar, input_text)

# Also available as XML or YAML
result_xml = rustine.run_grammar_xml(grammar, input_text)
result_yaml = rustine.run_grammar_yaml(grammar, input_text)
```

The result includes:

| Field | Description |
|-------|-------------|
| `consumed` | Total bytes consumed from input |
| `actions` | List of executed actions (post capture substitution) |
| `traces` | Match events, say/warn/fail messages |
| `diagnostics` | Structured messages with severity and spans |
| `capture_history` | Captured group values per match |
| `output` | Nested node tree |
| `error` | Error string if `do.fail` was invoked |

### Backward Compatibility with Gelatin

Rustine includes a `Gelatin` compatibility shim. Existing code using the
Python Gelatin API continues to work:

```python
from Gelatin.util import compile, generate

syntax = compile('syntax.gel')
print(generate(syntax, 'input.txt'))
print(generate(syntax, 'input.txt', format='json'))
```

---

## Rust Usage

### Parsing a Grammar

```rust
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

let tokens = lex(grammar_source).expect("lex failed");
let document = parse_gel_document(&tokens).expect("parse failed");
```

### Executing a Grammar

```rust
use rustine::exec::{execute, serialize_execution, RuntimeFormat};

let result = execute(grammar_source, input_text);
let json = serialize_execution(&result, RuntimeFormat::Json);
let xml  = serialize_execution(&result, RuntimeFormat::Xml);
let yaml = serialize_execution(&result, RuntimeFormat::Yaml);
```

### Streaming Execution

```rust
use rustine::stream::{ChunkReader, StreamingLexer};

// Feed chunks incrementally for large files
let reader = ChunkReader::new(file, 131072);
let mut lexer = StreamingLexer::new();

for chunk in reader {
    lexer.feed(&chunk);
    for event in lexer.drain_events() {
        // Process streaming events
    }
}
```

---

## CLI Usage

The `rgel` binary provides a shell-friendly interface:

```bash
# Convert text to JSON
rgel -s syntax.gel -f json input.txt

# Convert to XML (default) or YAML
rgel -s syntax.gel input.txt
rgel -s syntax.gel -f yaml input.txt

# Version
rgel --version
```

Output goes to stdout; pipe to a file or other tools:

```bash
rgel -s syntax.gel -f json input.txt > output.json
rgel -s syntax.gel -f json input.txt | jq '.'
```

---

## Testing

```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run with output visible
cargo test -- --nocapture
```

The test suite includes 55 test files covering all grammar features,
output actions, triggers, captures, and edge cases.

---

## Benchmarking

```bash
# Execution engine micro-benchmarks (scaling, serialization, captures, arena)
cargo bench --bench exec --no-default-features

# Fixture benchmarks (small grammars)
cargo bench --bench fixtures --no-default-features

# Scale benchmark (25 MB synthetic data)
cargo bench --bench scale --no-default-features

# One-shot scaling (25/50/100 MB)
cargo run --release --no-default-features --example scale_oneshot

# Custom grammar + input (any .gel + .txt)
cargo run --release --no-default-features \
  --example bench_run -- <syntax.gel> <input.txt> 5

# With jemalloc (Linux)
cargo bench --no-default-features --features jemalloc
```

See [BENCHMARKS.md](../BENCHMARKS.md) for full results.
