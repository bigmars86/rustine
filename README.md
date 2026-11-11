# Rustine

[![CI](https://github.com/bigmars86/rustine/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/bigmars86/rustine/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/rustine.svg)](https://pypi.org/project/rustine/)
[![Docs](https://readthedocs.org/projects/rustine/badge/?version=latest)](https://rustine.readthedocs.io/en/latest/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

**High-performance Gel syntax parser** that transforms unstructured text into
JSON, XML, or YAML. A complete Rust rewrite of
[Python Gelatin](https://github.com/knipknap/Gelatin), usable as a native
Python module, a Rust library, or a standalone CLI tool.

## Why Rustine?

[Gelatin](https://github.com/knipknap/Gelatin) depends on SimpleParse, which
does not support Python 3.12+. Rustine reimplements the engine in Rust:

- **Drop-in replacement** — same Gel grammar language, same output
- **35–46× faster** than Python Gelatin on large inputs, **4–9× less memory**
- **Python 3.9+** — abi3 wheels, no SimpleParse

## Quick Start

### Python

```bash
pip install rustine
```

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

### Rust

```bash
cargo add rustine --no-default-features
```

```rust
use rustine::exec::{execute, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

let mut doc = parse_gel_document(&lex(grammar_source)?)?;
let result = execute(&mut doc, "input", input_text)?;
let json = serialize_tree(&result, RuntimeFormat::Json);
```

### CLI

```bash
cargo install rustine --no-default-features --features cli

rgel -s syntax.gel -f json input.txt
rgel -s syntax.gel -f xml  input.txt
rgel -s syntax.gel -f yaml input.txt

# Keep the case of tag names (default: lowercase)
rgel -s syntax.gel --preserve-case input.txt
```

## Performance

Benchmarked on a 20.0 MB real-world IOS XR configuration
(263 grammars, 113 regex patterns, 862 754 output nodes), CLI end to end to
JSON, all tools on the same machine (Linux):

| Tool                   | Time       | Throughput    | Peak RSS   |
| ---------------------- | ---------- | ------------- | ---------- |
| Python Gelatin (`gel`) | 49.9 s     | 0.40 MB/s     | 1 920 MB   |
| **Rustine** (glibc)    | **1.16 s** | **17.3 MB/s** | **344 MB** |
| **Rustine** (jemalloc) | **1.08 s** | **18.6 MB/s** | **374 MB** |

| Comparison                                 | Speedup    |
| ------------------------------------------ | ---------- |
| Rustine vs Python Gelatin (25 MB scale)    | **35–36×** |
| Rustine vs Python Gelatin (CLI, real data) | **38–46×** |
| Rustine vs Python Gelatin (small fixtures) | **7–23×**  |
| Rustine vs textfsm (25 MB, 2026-03-23)     | **1.6×**   |

See [BENCHMARKS.md](BENCHMARKS.md) for full results (Linux glibc and
jemalloc, Python Gelatin, older Windows and textfsm numbers), serialization
timings, and reproduction instructions.

## Alternatives

| Tool | Approach | Output | vs Rustine |
| ---- | -------- | ------ | ---------- |
| [textfsm](https://github.com/google/textfsm) | Line-by-line FSM templates | Flat tables (list of dicts) | Simpler for quick extraction; no hierarchy, no nesting, pure Python |
| [TTP](https://github.com/dmulyalin/ttp) | Template-based parsing | Nested dicts | Flexible templates; Python-only, slower on large inputs |
| [Napalm](https://github.com/napalm-automation/napalm) | Device abstraction + textfsm | Flat dicts per getter | Higher-level (device drivers); not a general text parser |
| [PyATS/Genie](https://developer.cisco.com/pyats/) | Model-driven parsing | Structured models | Cisco ecosystem; heavy dependencies, not general-purpose |
| [nom](https://github.com/rust-bakery/nom) / [pest](https://pest.rs) | Rust parser combinators / PEG | Custom AST | Maximum flexibility; requires writing a parser in code, no DSL file |

**Rustine's niche:** a grammar-driven text→tree transformer with a concise DSL
(`.gel` files), hierarchical output, and native performance. It sits between
simple template extractors (textfsm) and full parser generators (nom/pest).

## Feature Highlights

- **Gel grammar language** — match, imatch, when, skip, define, grammar
  inheritance
- **Rich output actions** — create, add, replace, add_attribute, open, enter,
  leave, set_root_name
- **Trigger system** — enqueue_before/after/on_add/on_leave (single-shot and
  persistent)
- **Captures** — positional (`$1`, `$2`) and named (`$name`) with
  interpolation in paths and values
- **Three output formats** — JSON, XML, YAML
- **Streaming execution** — feed chunks incrementally via `StreamingRunner`
- **Structured errors** — `GelError` with source spans (line, column, offset)
- **Semantic validation** — regex pre-validation, inheritance checks,
  undefined grammar/variable warnings
- **Python bindings** — PyO3 + maturin, installable via `pip`
- **CLI tool** — `rgel` binary for shell pipelines

## Documentation

| Document | Description |
| -------- | ----------- |
| [Installation](docs/install.md) | pip, source build, Rust crate, CLI, Cargo features |
| [Getting Started](docs/getting-started.md) | Python, Rust and CLI usage, tests, benchmarks |
| [Gel Syntax Reference](docs/syntax.md) | Grammar language, statements, actions, triggers |
| [Architecture](docs/architecture.md) | Parser pipeline, module layout, design decisions |
| [Performance](docs/performance.md) | Optimization techniques, benchmark overview |
| [BENCHMARKS.md](BENCHMARKS.md) | Raw benchmark data for all platforms |
| [Migration from Gelatin](docs/migration.md) | Drop-in replacement guide, parity notes |
| [Contributing](CONTRIBUTING.md) | Development setup, coding guidelines, PR process |
| [Changelog](CHANGELOG.md) | Version history |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
