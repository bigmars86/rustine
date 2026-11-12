# Installation

## Python

```bash
pip install rustine
```

Wheels are published for:

| OS      | Architectures         |
|---------|-----------------------|
| Linux   | x86_64, aarch64       |
| macOS   | x86_64, Apple Silicon |
| Windows | x86_64                |

### From Source

Requires Rust 1.86+ and Python 3.9+.

```bash
git clone https://github.com/bigmars86/rustine.git
cd rustine
pip install maturin
maturin develop --release
```

## Rust Library

```bash
cargo add rustine --no-default-features
```

`--no-default-features` drops the `python` feature, which needs a Python
interpreter at build time.

## CLI

```bash
cargo install rustine --no-default-features --features cli
```

From a checkout: `cargo build --release --no-default-features --features cli`
(binary: `target/release/rgel`).

## Cargo Features

Compile-time switches, enabled with `--features`:

| Feature     | Default          | Description                                |
|-------------|------------------|--------------------------------------------|
| `python`    | **yes**          | PyO3 extension module                      |
| `cli`       | no               | `rgel` command-line binary                 |
| `mmap`      | no               | Memory-mapped file input for `ChunkReader` |
| `parallel`  | no               | Compile regexes in parallel (Rayon)        |
| `jemalloc`  | **Linux wheels** | jemalloc allocator (not on MSVC)           |
| `mimalloc`  | no               | mimalloc allocator                         |
| `bench`     | no               | Internal benchmark examples                |
| `profiling` | no               | Per-rule timing counters                   |

jemalloc is up to 18 % faster than glibc malloc
([BENCHMARKS.md](../BENCHMARKS.md)) and returns freed memory to the OS, for a
lower steady-state RSS in long-running workers. Source
builds use the system allocator unless built with `--features jemalloc`.
