# Contributing to Rustine

Thank you for your interest in improving Rustine!

## Prerequisites

- **Rust** stable toolchain (1.70+)
- **Python** 3.9+ (for PyO3 development)
- **maturin** (`pip install maturin`)

Recommended VS Code extensions: rust-analyzer, Python, markdownlint, GitLens.

## Development Setup

```bash
# Rust
cargo build
cargo test

# Python bridge
pip install maturin
maturin develop
python -c "from Rustine import rustine; print(rustine.parse_to_json('grammar g:'))"
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
| `benches/` | Criterion benchmarks |
| `fixtures/` | Test grammars and input data |
| `scripts/` | Python benchmark and comparison scripts |
| `Rustine/` | Python package (PyO3 module) |
| `Gelatin/` | Backward-compatible Python shim |

## Coding Guidelines

- Prefer explicit error propagation via `GelError` — avoid `.unwrap()` in
  library code.
- Minimize allocations on hot paths — use `Cow<str>`, `SmallVec`, and
  arena allocation where appropriate.
- Maintain ordered attributes (`Vec<(String, String)>`) for output parity
  with Python Gelatin.
- Avoid `unsafe` unless profiling clearly justifies it.
- Keep the public API surface small — expose through `lib.rs` and
  `bridge.rs`.

## Adding a New Action

1. Add the action variant to `execute_runtime_actions` in `src/exec/mod.rs`.
2. Add a test under `tests/` verifying the semantics.
3. Document the action in `docs/syntax.md` (Output Actions section).
4. Run the full test suite: `cargo test`.

## Testing

```bash
# Full suite
cargo test

# Single test
cargo test test_name -- --nocapture

# Python bridge
maturin develop
python -m pytest  # if Python tests exist
```

The test suite includes 55 integration tests covering all grammar features,
output actions, triggers, captures, serialization, and edge cases.

## Benchmarking

```bash
# Quick fixture benchmarks
cargo bench --bench fixtures --no-default-features

# Full scale benchmarks
cargo bench --bench scale --no-default-features

# Custom grammar + input
cargo run --release --no-default-features \
  --example bench_run -- <syntax.gel> <input.txt> 5 --help
```

## Submitting Changes

1. Run `cargo fmt` (if configured) and `cargo clippy`.
2. Run `cargo test` — all tests must pass.
3. Update documentation if behavior changes.
4. Open a PR with a concise description.

## License

Contributions are accepted under the same dual license as the project:
MIT or Apache-2.0.
