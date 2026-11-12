# Contributing to Rustine

Thank you for your interest in improving Rustine!

## Prerequisites

- **Rust** stable toolchain (1.86+)
- **Python** 3.11+ (for PyO3 development)
- **maturin** (`pip install maturin`)

Recommended VS Code extensions: rust-analyzer, Python, markdownlint, GitLens.

## Development Setup

```bash
# Rust
cargo build
cargo test --no-default-features

# Python bridge
pip install maturin
maturin develop
python -c "import Rustine; print(Rustine.__version__)"
```

## Project Structure

| Directory | Purpose |
|-----------|---------|
| `src/parser/` | Lexer, AST, recursive descent parser |
| `src/exec/` | Execution engine, output builder, triggers |
| `src/stream/` | Streaming lexer, chunk reader |
| `src/bridge.rs` | PyO3 Python bindings |
| `src/main.rs` | CLI binary (`rgel`) |
| `tests/` | Rust integration tests |
| `benches/` | Criterion benchmarks, `compare.py` |
| `fixtures/` | Test grammars and input data |
| `Rustine/` | Python package (PyO3 module) |
| `Gelatin/` | `Rustine.util` under the `Gelatin` import path |

## Coding Guidelines

- Prefer explicit error propagation via `GelError` — avoid `.unwrap()` in
  library code.
- Minimize allocations on hot paths — use `Cow<str>` and `SmallVec` where
  appropriate.
- Keep attributes in insertion order; the byte-exact parity tests depend on it.
- Comments and docs describe the current behavior and its reason, briefly:
  no history or issue numbers.
- Avoid `unsafe` unless profiling clearly justifies it.
- Keep the public API surface small — expose through `lib.rs` and
  `bridge.rs`.

## Adding a New Action

1. Add the action variant to `execute_runtime_actions` in `src/exec/mod.rs`.
2. Add a test under `tests/` verifying the semantics.
3. Document the action in `docs/syntax.md` (Output Actions section).
4. Run the full test suite: `cargo test --no-default-features`.

## Testing

```bash
# Full suite
cargo test --no-default-features

# Single test
cargo test --no-default-features test_name -- --nocapture

# Python bridge
maturin develop
python tests/python/test_preserve_case.py
```

## Benchmarking

```bash
# Execution engine micro-benchmarks (scaling, serialization, captures)
cargo bench --bench exec --no-default-features

# Quick fixture benchmarks
cargo bench --bench fixtures --no-default-features

# Full scale benchmarks (25 MB synthetic — takes several minutes)
cargo bench --bench scale --no-default-features
```

The numbers in [BENCHMARKS.md](BENCHMARKS.md) come from
`benches/compare.py`, which also compares with Python Gelatin; the file
explains how to rerun it.

## Submitting Changes

1. Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.
2. Run `cargo test --no-default-features` — all tests must pass.
3. Update documentation if behavior changes.
4. Open a PR with a concise description.

## License

Contributions are accepted under the same dual license as the project:
MIT or Apache-2.0.
