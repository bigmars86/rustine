# Performance

Rustine was built for throughput on large inputs. This page summarizes the
key performance characteristics and optimization techniques. For the
measured numbers and the method, see [BENCHMARKS.md](../BENCHMARKS.md).

---

## Summary

Rustine compared with Python Gelatin, each converting the input to JSON in
a new process on the same Linux PC:

| Workload               | Rustine      | Instructions   | Time            | Peak memory   |
|------------------------|--------------|----------------|-----------------|---------------|
| IOS XR config, 20.6 MB | Rust API     | **20× fewer**  | **33× faster**  | **5.6× less** |
|                        | Python wheel | **19× fewer**  | **30× faster**  | **5.5× less** |
| Scale, 25.0 MB         | Rust API     | **23× fewer**  | **37× faster**  | **8.4× less** |
|                        | Python wheel | **21× fewer**  | **35× faster**  | **7.5× less** |
| 5 small fixtures       | Rust API     | **7.8× fewer** | **8.6× faster** | —             |
|                        | Python wheel | **7.5× fewer** | **7.9× faster** | —             |

Instruction counts do not depend on the CPU, times do.

### Allocators

The wheels use jemalloc on Linux (glibc and musl) and mimalloc on Windows;
the macOS wheels and source builds without `--features jemalloc` or
`--features mimalloc` use the system allocator. On the same PC, the system
allocators take 4–25 % longer on Linux (glibc), 28 % to 2.1× longer on musl
and 26–39 % longer on Windows. With mimalloc, Windows is within 5 % of Linux
with jemalloc on large inputs.

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

### YAML Scalars

Each scalar is checked in one pass and written straight to the sink; only
strings that YAML would read differently are quoted or escaped.

---

## Memory Profile

Peak memory for one conversion to JSON (Linux):

| Workload               | Rustine (Rust) | Rustine (Python) | Python Gelatin |
|------------------------|---------------:|-----------------:|---------------:|
| IOS XR config, 20.6 MB | 342 MB         | 346 MB           | 1 907 MB       |
| Scale, 25.0 MB         | 364 MB         | 409 MB           | 3 072 MB       |

Rustine keeps the whole tree in memory, so peak memory grows linearly with
the input; the Python wheel also holds the JSON as a Python string. The
difference to Gelatin is primarily due to:

1. **Compact nodes** — Rust nodes are tightly packed; Python uses many
   small dict/list objects with per-object GC overhead.
2. **No GC metadata** — Rust has zero runtime overhead per allocation.
3. **String interning** — shared `Arc<str>` for repeated names vs Python's
   per-instance string copies.

---

## How to Benchmark

[BENCHMARKS.md](../BENCHMARKS.md) shows how to repeat the comparison with
`benches/compare.py`, also for your own grammar and input. While
developing, the Criterion benchmarks time execution and serialization
separately:

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
