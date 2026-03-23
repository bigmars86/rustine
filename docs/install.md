# Installation

## From PyPI (recommended)

```bash
pip install rustine
```

Wheels are published for:

| OS      | Architectures      |
|---------|--------------------|
| Linux   | x86_64, aarch64    |
| macOS   | x86_64, Apple Silicon |
| Windows | x86_64             |

## From Source

Requires a Rust toolchain (1.70+) and Python 3.9+.

```bash
pip install maturin
git clone https://github.com/knipknap/rustine.git
cd rustine
maturin develop --release
```

## Rust-only (library crate)

Add to your `Cargo.toml`:

```toml
[dependencies]
rustine = { git = "https://github.com/knipknap/rustine.git", default-features = false }
```

## Optional Features

| Feature     | Description                          | Default        |
|-------------|--------------------------------------|----------------|
| `python`    | PyO3 extension module                | yes            |
| `parallel`  | Rayon-based parallel lexing          | no             |
| `mmap`      | Memory-mapped file I/O               | no             |
| `cli`       | CLI binary (`rgel`)                  | no             |
| `jemalloc`  | jemalloc allocator (Linux)           | **Linux wheels** |
| `mimalloc`  | mimalloc allocator (cross-platform)  | no             |

Published Linux wheels include `jemalloc` by default for better
performance and memory behavior.

Enable features with:

```bash
cargo build --features parallel,mmap
```
