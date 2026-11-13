# Installation

## Python

```bash
pip install rustine
```

Wheels are published for:

| OS                        | Architectures         | Allocator |
|---------------------------|-----------------------|-----------|
| Linux (glibc)             | x86_64, aarch64       | jemalloc  |
| Linux (musl, e.g. Alpine) | x86_64, aarch64       | jemalloc  |
| macOS                     | x86_64, Apple Silicon | system    |
| Windows                   | x86_64, x86           | mimalloc  |

### From Source

Requires Rust 1.86+ and Python 3.11+.

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

| Feature     | Default            | Description                         |
|-------------|--------------------|-------------------------------------|
| `python`    | **yes**            | PyO3 extension module               |
| `cli`       | no                 | `rgel` command-line binary          |
| `parallel`  | no                 | Compile regexes in parallel (Rayon) |
| `jemalloc`  | **Linux wheels**   | jemalloc allocator (not on MSVC)    |
| `mimalloc`  | **Windows wheels** | mimalloc allocator                  |
| `bench`     | no                 | Internal benchmark examples         |
| `profiling` | no                 | Per-rule timing counters            |

The wheels use jemalloc on Linux and mimalloc on Windows. With the system
allocator, Rustine takes 1–24 % longer on Linux (glibc), 26 % to 2.1× longer
on musl and 26–32 % longer on Windows with large inputs; mimalloc on Linux
takes 7–13 % less time than jemalloc but needs up to 67 % more memory
([BENCHMARKS.md](https://github.com/bigmars86/rustine/blob/main/BENCHMARKS.md#operating-systems)).
Source builds use the system allocator unless built with
`--features jemalloc` or `--features mimalloc`.
