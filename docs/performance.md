# Performance

Rustine was built for throughput on large inputs. This page summarizes the
key performance characteristics and optimization techniques. For raw
benchmark data across all platforms, see [BENCHMARKS.md](../BENCHMARKS.md).

---

## Summary

| Comparison | Speedup | RSS Improvement |
|------------|---------|-----------------|
| Rustine vs Python Gelatin (25 MB scale) | **17–19×** | **3–4× less** |
| Rustine vs Python Gelatin (CLI, 20.9 MB) | **11–13×** | **6× less** |
| Rustine vs textfsm (25 MB synthetic) | **1.6×** | — |
| Rustine vs textfsm (20.9 MB real) | **1.9×** | — |

### Platform Comparison (Rustine only)

| Platform | 25 MB Scale (best total) | 20.9 MB Real-Data (exec) |
|----------|--------------------------|--------------------------|
| Windows (MSVC) | 5.52 s | 3.76 s |
| Linux (glibc) | 4.52 s | 2.69 s |
| **Linux (jemalloc)** | **3.79 s** | **2.08 s** |

jemalloc is the fastest allocator across all workloads — 1.5–1.8× faster
than Windows MSVC, 1.1–1.3× faster than glibc.

---

## Optimization Techniques

### Zero-Copy Captures

Regex matches borrow from the input via `Cow<'a, str>`. The hot path avoids
`.to_string()` entirely — allocations only occur when capture interpolation
modifies the string.

### Arc\<str\> Interning

Grammar names, function call names, and named capture groups use `Arc<str>`
for cheap clones. This eliminates redundant string allocations in
parse-once-execute-many scenarios.

### O(1) Child Lookup

Child-group index maps provide constant-time node grouping instead of O(n)
linear scans. This was the single largest optimization for deep trees,
converting the `out.add` path from O(n²) to O(n).

### Lazy Regex Compilation

Patterns are compiled once per (pattern, case-flag) pair and cached by slot
index. Subsequent matches reuse the compiled DFA.

### Arena Allocation

`bumpalo`-backed arena for bulk-freed temporaries during execution.
Pre-sized based on input length for minimal reallocation.

### Streaming Serialization

JSON, XML, and YAML writers stream directly to any `std::io::Write` sink
via a unified `OutputSink` trait, eliminating intermediate `String` / `Vec<u8>`
buffers. The streaming path reduces serialize-step time by ~30%.

### Stack-Allocated Small Vecs

`SmallVec<[_; 8]>` for captures and names avoids heap allocation for ≤ 8
groups (the common case).

### Static Indent Lookup

YAML indentation uses a 64-byte static const instead of `"  ".repeat(n)`.

### Cow\<str\> in YAML

`yaml_quote_key` / `yaml_quote_value` return `Cow<str>`, avoiding
allocations for the common unquoted case.

---

## Memory Profile

RSS scales linearly with input size:

| Input Size | Rustine (jemalloc) | Python Gelatin |
|------------|--------------------:|---------------:|
| 25 MB      | 739 MB             | 3 075 MB       |
| 50 MB      | 1 213 MB           | —              |
| 100 MB     | 2 164 MB           | —              |

The difference is primarily due to:

1. **Arena allocation** — Rust nodes are tightly packed; Python uses many
   small dict/list objects with per-object GC overhead.
2. **No GC metadata** — Rust has zero runtime overhead per allocation.
3. **String interning** — shared `Arc<str>` for repeated names vs Python's
   per-instance string copies.

---

## How to Benchmark

### Criterion Benchmarks

```bash
# All benchmarks (exec + fixtures + scale)
cargo bench --no-default-features

# Execution engine micro-benchmarks (scaling, serialization, captures, arena)
cargo bench --bench exec --no-default-features

# Fixtures only (5 small grammars, 100 samples)
cargo bench --bench fixtures --no-default-features

# Scale (25 MB synthetic, 10 samples)
cargo bench --bench scale --no-default-features
```

### One-Shot Scaling

```bash
cargo run --release --no-default-features --example scale_oneshot
cargo run --release --no-default-features --example scale_oneshot -- --sizes 50,100
```

### Real-Data

```bash
# In-memory (5 iterations, median)
cargo run --release --no-default-features \
  --example bench_run -- syntax.gel input.txt 5

# Streaming serialization
cargo run --release --no-default-features \
  --example bench_run -- syntax.gel input.txt 5 --stream
```

### CLI

```bash
cargo build --release --features cli --no-default-features
target/release/rgel -s syntax.gel -f json input.txt > /dev/null
```

### Linux via Docker

```bash
# glibc
docker run --rm -v "${PWD}:/work" -w /work rust:latest \
  cargo bench --no-default-features

# jemalloc
docker run --rm -v "${PWD}:/work" -w /work rust:latest \
  cargo bench --no-default-features --features jemalloc
```
