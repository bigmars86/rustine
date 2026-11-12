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
- **30–37× faster** than Python Gelatin on large inputs, **5.5–8.4× less memory**
- **Python 3.11+** — abi3 wheels, no SimpleParse

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
let json = serialize_tree(&result, RuntimeFormat::Json)?;
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

A 20.6 MB pseudonymized IOS XR configuration (shipped in `fixtures/`)
converted to JSON, each engine in a new process on the same Linux PC
(146 grammars, 862 744 output nodes):

| Engine                         | Instructions | Peak memory | Time       |
| ------------------------------ | -----------: | ----------: | ---------: |
| Python Gelatin, CPython 3.10   | 358 G        | 1 927 MB    | 38.8 s     |
| Python Gelatin, CPython 3.11   | 279 G        | 1 907 MB    | 34.2 s     |
| **Rustine** (Python 3.11)      | **14.8 G**   | **346 MB**  | **1.13 s** |
| **Rustine** (Rust)             | **14.0 G**   | **342 MB**  | **1.05 s** |

Instructions (G = 10⁹) measure the work independently of the CPU, times do
not. [BENCHMARKS.md](BENCHMARKS.md) adds a synthetic 25 MB input, small
inputs and Windows, and shows how to repeat the measurement.

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
| [BENCHMARKS.md](BENCHMARKS.md) | Results against Python Gelatin, method, how to reproduce |
| [Migration from Gelatin](docs/migration.md) | Drop-in replacement guide, parity notes |
| [Contributing](CONTRIBUTING.md) | Development setup, coding guidelines, PR process |
| [Changelog](CHANGELOG.md) | Version history |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
