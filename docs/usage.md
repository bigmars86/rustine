# Usage

## Python API

### `parse_to_json(grammar, input) → str`

Parse input using a Gel grammar and return structured JSON output.

```python
import rustine

grammar = r"""
match /name:\s*(.+)/ {
    out.create("person")
    out.add("person/name?text=$1")
}
match /age:\s*(\d+)/ {
    out.add("person/age?text=$1")
}
"""

input_text = "name: Alice\nage: 30"
result = rustine.parse_to_json(grammar, input_text)
print(result)
```

### `parse_to_xml(grammar, input) → str`

Same as above but returns XML.

### `parse_to_yaml(grammar, input) → str`

Same as above but returns YAML.

### `run_grammar(grammar, input) → str`

Execute a grammar and return full execution results as JSON including:
consumed lines, actions, traces, diagnostics, captures, and the output tree.

### `run_grammar_xml(grammar, input) → str`

Full execution results in XML format.

### `run_grammar_yaml(grammar, input) → str`

Full execution results in YAML format.

## Rust API

### Parsing

```rust
use rustine::parser::core::parse_gel;

let grammar_source = r#"match /foo/ { out.create("root") }"#;
let ast = parse_gel(grammar_source).expect("parse failed");
println!("{:#?}", ast);
```

### Execution

```rust
use rustine::exec::execute;

let result = execute(grammar_source, input_text);
```

### Streaming

Streaming is always available — no feature flag required:

```rust
use rustine::stream::{ChunkReader, StreamingLexer};

let reader = ChunkReader::open("large_input.txt", 128 * 1024)?;
let mut lexer = StreamingLexer::new(reader);

while let Some(batch) = lexer.next_batch()? {
    for token in &batch.tokens {
        println!("{:?}", token.kind);
    }
    if batch.finished { break; }
}
```

## CLI

Build the CLI binary:

```bash
cargo build --release --features cli
```

Parse a file:

```bash
rgel parse grammar.gel input.txt --format json --chunk 131072
```

Options:
- `--format json|xml|yaml` — output format (default: json)
- `--chunk BYTES` — streaming chunk size (default: 131072)
