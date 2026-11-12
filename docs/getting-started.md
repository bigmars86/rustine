# Getting Started

Installation and Cargo features: see [Installation](install.md).

---

## Python Usage

```python
from Rustine.util import compile_string, generate_string

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

converter = compile_string(grammar)
print(generate_string(converter, "Name: Alice\nAge: 30\nOffice: 1st Ave\n", format="json"))
```

`format` is `"xml"` (default), `"json"`, `"yaml"` or `"none"`. For files use
`compile(path)`, `generate(converter, path)` and `generate_to_file(...)`;
`from Gelatin.util import ...` works too.

The native module has the same as methods, plus a one-shot call:

```python
from Rustine import rustine

ctx = rustine.compile_grammar(grammar)       # compile once
ctx.run(text)                                # JSON; also run_xml / run_yaml
rustine.run_grammar(grammar, "input", text)  # compile + run, JSON
```

Grammar errors raise `ValueError`, `do.fail()` raises `RuntimeError`.

### Case of Output Names

Rustine lowercases tag names and attribute keys in `out.*` paths. Pass
`preserve_case=True` to keep them as written:

```python
converter = compile_string(grammar, preserve_case=True)
```

---

## Rust Usage

```rust
use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

let mut doc = parse_gel_document(&lex(grammar_source)?)?;
let result = execute(&mut doc, "input", input_text)?;
let json = serialize_tree(&result, RuntimeFormat::Json); // or Xml, Yaml
```

`serialize_execution` writes the whole result instead of only the tree:
`consumed`, `actions`, `traces`, `diagnostics`, `capture_history`, `output`
and `error`. To run one grammar on many inputs, call `doc.compile_regexes()`
once and then `execute_tree(&doc, "input", text)`, which skips that per-match
metadata, or `execute_precompiled` to keep it.

### Streaming Execution

```rust
use rustine::StreamingRunner;

let mut runner = StreamingRunner::new(&mut doc, "input")?;
for chunk in chunks {
    runner.feed(&chunk);
    runner.step()?; // StreamingEvents produced so far
}
let result = runner.finish()?; // serialize like a batch result
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

# Keep the case of tag names (default: lowercase)
rgel -s syntax.gel --preserve-case input.txt

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
# Rust tests
cargo test --no-default-features
cargo test --no-default-features test_name -- --nocapture

# Python bindings
maturin develop
python tests/python/test_preserve_case.py
```

---

## Benchmarking

```bash
# Criterion micro-benchmarks while developing (execution, fixtures, 25 MB scale)
cargo bench --no-default-features

# Instructions (Valgrind), peak memory, CPU time and time for any grammar and input
cargo build --release --no-default-features --features parallel,mmap --example bench_run
python benches/compare.py --rust target/release/examples/bench_run \
  --workload my-data <syntax.gel> <input.txt> --instructions
```

See [BENCHMARKS.md](../BENCHMARKS.md) for results and the full setup.
