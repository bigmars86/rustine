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

[Gelatin](https://github.com/knipknap/Gelatin) has been a reliable tool for
converting network device output and other semi-structured text into structured
data. However, its core dependency — **SimpleParse** — has not been updated for
Python 3.12+ and is no longer maintained. This made Gelatin incompatible with
modern Python versions.

Rustine solves this by reimplementing the entire Gelatin engine in Rust:

- **Drop-in replacement** — same Gel grammar language, same output format
- **10–19× faster** than Python Gelatin (depending on workload and platform)
- **3–4× less memory** on large inputs
- **Works on Python 3.9–3.13+** via PyO3 — no C extension dependency
- **100% feature parity** with the original Python implementation

## Quick Start

### Python

```bash
pip install rustine
```

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
print(rustine.parse_to_json(grammar, input_text))
```

### Rust

```bash
cargo add rustine --no-default-features
```

```rust
use rustine::exec::{execute, serialize_execution, RuntimeFormat};

let result = execute(grammar_source, input_text);
let json = serialize_execution(&result, RuntimeFormat::Json);
```

### CLI

```bash
cargo install rustine --features cli

rgel -s syntax.gel -f json input.txt
rgel -s syntax.gel -f xml  input.txt
rgel -s syntax.gel -f yaml input.txt
```

## Performance

Benchmarked on 20.9 MB real-world IOS XR configuration
(263 grammars, 113 regex patterns, 899 757 output nodes):

| Tool / Platform        | Time       | Throughput     | Peak RSS   |
| ---------------------- | ---------- | -------------- | ---------- |
| Python Gelatin (Win)   | 66.3 s     | 0.33 MB/s      | 3 075 MB   |
| **Rustine** (Win)      | **4.10 s** | **5.1 MB/s**   | **496 MB** |
| **Rustine** (jemalloc) | **2.31 s** | **9.1 MB/s**   | **456 MB** |

| Comparison                        | Speedup    |
| --------------------------------- | ---------- |
| Rustine vs Python Gelatin (scale) | **17–19×** |
| Rustine vs Python Gelatin (CLI)   | **11–13×** |
| Rustine vs textfsm (25 MB)        | **1.6×**   |

See [BENCHMARKS.md](BENCHMARKS.md) for full cross-platform results
(Windows, Linux glibc, Linux jemalloc), serialization timings, and
reproduction instructions.

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

### Cargo Features (compile-time)

Features are selected at build time via `--features` and control which
capabilities are compiled into the binary.
See [Getting Started → Cargo Features](docs/getting-started.md#cargo-features)
for the full table and usage examples.

Key features: `cli`, `jemalloc` (default in Linux wheels), `mimalloc`,
`mmap`, `parallel`, `python` (default).

## Documentation

| Document | Description |
| -------- | ----------- |
| [Getting Started](docs/getting-started.md) | Installation, first steps, Python/Rust/CLI usage |
| [Gel Syntax Reference](docs/syntax.md) | Grammar language, statements, actions, triggers |
| [Architecture](docs/architecture.md) | Parser pipeline, module layout, design decisions |
| [Performance](docs/performance.md) | Optimization techniques, benchmark overview |
| [BENCHMARKS.md](BENCHMARKS.md) | Raw benchmark data for all platforms |
| [Migration from Gelatin](docs/migration.md) | Drop-in replacement guide, parity notes |
| [Contributing](CONTRIBUTING.md) | Development setup, coding guidelines, PR process |
| [Changelog](CHANGELOG.md) | Version history |

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
