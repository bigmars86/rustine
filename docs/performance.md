# Performance

Rustine was built for throughput on large inputs. This page summarizes the
key performance characteristics and optimization techniques. For raw
benchmark data across all platforms, see [BENCHMARKS.md](../BENCHMARKS.md).

---

## Summary

| Comparison | Speedup | RSS Improvement |
|------------|---------|-----------------|
| Rustine vs Python Gelatin (25 MB scale) | **35–36×** | **6–9× less** |
| Rustine vs Python Gelatin (CLI, 20.0 MB real data) | **38–46×** | **4–5× less** |
| Rustine vs Python Gelatin (small fixtures) | **7–23×** | — |
| Rustine vs textfsm (25 MB synthetic, 2026-03-23) | **1.6×** | — |
| Rustine vs textfsm (20.9 MB real, 2026-03-23) | **1.9×** | — |

All tools were measured on the same machine (Linux), except textfsm.

### Allocator Comparison (Rustine, Linux)

| Allocator | 25 MB One-Shot (best total) | 20.0 MB Real Data (exec) |
|-----------|-----------------------------|--------------------------|
| glibc | 1.49 s | 0.93 s |
| **jemalloc** | **1.22 s** | **0.85 s** |

jemalloc executes about 1.1× faster than glibc. Windows (MSVC) was not
measured again; in the 2026-03-23 numbers jemalloc was 1.5–1.8× faster than
MSVC.

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

Child-group index maps find the node group for `out.add` in constant time
instead of a linear scan, which keeps deep trees O(n) overall.

### Lazy Regex Compilation

Patterns are compiled once per (pattern, case-flag) pair and cached by slot
index. Subsequent matches reuse the compiled DFA.

### Streaming Serialization

JSON, XML, and YAML writers stream directly to any `std::io::Write` sink
without an intermediate `String` / `Vec<u8>` buffer. The streaming path
reduces serialize-step time by ~30%.

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
| 25 MB      | 358 MB             | 3 095 MB       |
| 50 MB      | 716 MB             | —              |
| 100 MB     | 1 284 MB           | —              |

The difference is primarily due to:

1. **Compact nodes** — Rust nodes are tightly packed; Python uses many
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

# Execution engine micro-benchmarks (scaling, serialization, captures)
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
