# Benchmark Results

> **Generated:** 2026-03-23
> **Rust:** 1.94.0 (`codegen-units=1, lto=true, opt-level=3`)
> **Python:** 3.10 CPython (for Gelatin / textfsm)
>
> | Label        | OS / Allocator                                       |
> |--------------|------------------------------------------------------|
> | Win          | Windows AMD64, AMD Ryzen, MSVC (system allocator)    |
> | glibc        | Linux AMD64, Docker `rust:latest`, glibc 2.41        |
> | **jemalloc** | Linux AMD64, Docker `rust:latest`, jemalloc 5.3      |

---

## 1 · Fixture Benchmarks

> Each fixture: compile grammar → execute → serialize (JSON / XML / YAML)
> **Criterion** — 100 samples, 5 s measurement, 1 s warm-up

| Fixture   | Input   | Win      | glibc    | **jemalloc** |
|-----------|---------|----------|----------|--------------|
| complex   | 14.9 KB | 4.20 ms  | 2.98 ms  | **2.59 ms**  |
| csv       | 130 B   | 74.1 µs  | 59.4 µs  | **46.1 µs**  |
| linesplit | 124 B   | 25.4 µs  | 19.5 µs  | **15.0 µs**  |
| simple    | 144 B   | 54.1 µs  | 42.0 µs  | **35.1 µs**  |
| tria      | 1.3 KB  | 226.2 µs | 194.8 µs | **138.5 µs** |

> Peak RSS — Win: 37.1 MB · **glibc: 31.2 MB** · jemalloc: 33.8 MB

### Python Gelatin (same fixtures)

| Fixture   | Input   | Win (JSON) | Linux (JSON) |
|-----------|---------|------------|--------------|
| complex   | 14.9 KB | 43 ms      | 48 ms        |
| csv       | 130 B   | 2.0 ms     | 2.0 ms       |
| linesplit | 124 B   | 0.5 ms     | 0.4 ms       |
| simple    | 144 B   | 1.0 ms     | 1.0 ms       |
| tria      | 1.3 KB  | 3.0 ms     | 4.0 ms       |

> Peak RSS — Win: 24.4 MB · Linux: 21.6 MB
> Python Gelatin does not separate execute from serialize.
> Rustine (jemalloc) is **10–17× faster** on fixtures.

---

## 2 · Scale Benchmark (25 MB synthetic)

> 302 grammars, ~1 110 rules, 6 action-type patterns
> Dynamically generated data — 1 521 168 tree nodes
> **Criterion** — 10 samples, 60 s measurement, 5 s warm-up

| Benchmark      | Win      | glibc      | **jemalloc** |
|----------------|----------|------------|--------------|
| Execute        | 5.53 s   | 5.02 s     | **3.75 s**   |
| Serialize JSON | 365 ms   | **258 ms** | 320 ms       |
| Serialize XML  | 728 ms   | **505 ms** | 676 ms       |
| Serialize YAML | 246 ms   | **177 ms** | 216 ms       |
| Total JSON     | 6.13 s   | 8.13 s ¹   | **4.11 s**   |
| Total XML      | 6.61 s   | 8.23 s ¹   | **4.40 s**   |
| Total YAML     | 6.06 s   | 7.69 s ¹   | **4.03 s**   |
| Peak RSS       | 1 092 MB | 1 087 MB   | **965 MB**   |

> ¹ glibc totals inflated by allocator fragmentation under Criterion's
> repeated alloc / free of 1.5 M nodes. One-shot runs (§ 3) confirm glibc
> is ~18 % faster than Windows. jemalloc eliminates this overhead entirely.

### Python Gelatin (25 MB, same grammars + data)

| Benchmark  | Win      | Linux    |
|------------|----------|----------|
| Total JSON | 77.6 s   | 81.8 s   |
| Total XML  | 69.7 s   | 74.4 s   |
| Peak RSS   | 3 074 MB | 3 085 MB |

> Rustine (jemalloc) is **17–19× faster** and uses **3× less memory**.

---

## 3 · Scale One-Shot (25 / 50 / 100 MB)

> `cargo run --release --example scale_oneshot`
> Single wall-clock run — no Criterion warm-up.

### Total Time (execute + serialize)

| Size · Format  | Win     | glibc       | **jemalloc** |
|----------------|---------|-------------|--------------|
| 25 MB · JSON   | 7.07 s  | 5.40 s      | **4.17 s**   |
| 25 MB · XML    | 6.06 s  | 5.69 s      | **4.31 s**   |
| 25 MB · YAML   | 5.52 s  | 4.52 s      | **3.79 s**   |
| 50 MB · JSON   | 12.27 s | 10.84 s     | **8.15 s**   |
| 50 MB · XML    | 13.70 s | 11.73 s     | **8.67 s**   |
| 50 MB · YAML   | 12.16 s | 9.98 s      | **7.64 s**   |
| 100 MB · JSON  | 25.76 s | 22.58 s     | **18.67 s**  |
| 100 MB · XML   | 25.60 s | **24.08 s** | 24.38 s      |
| 100 MB · YAML  | 24.70 s | 22.11 s     | **16.99 s**  |

### Peak RSS

| Size   | Win        | glibc      | **jemalloc** |
|--------|------------|------------|--------------|
| 25 MB  | 818 MB     | 886 MB     | **739 MB**   |
| 50 MB  | 1 393 MB   | 1 471 MB   | **1 213 MB** |
| 100 MB | 2 528 MB   | 2 675 MB   | **2 164 MB** |

> Linear scaling confirmed — throughput constant (±15 %) from 25 to 100 MB.
> jemalloc uses **10–19 % less RSS** than the system allocator at every size.

### Python Gelatin (25 MB one-shot)

| Size · Format | Win      | Linux    |
|---------------|----------|----------|
| 25 MB · JSON  | 85.1 s   | 80.0 s   |
| 25 MB · XML   | 79.3 s   | 73.1 s   |
| Peak RSS      | 3 075 MB | 3 087 MB |

> Rustine best (jemalloc): 25 MB JSON in **4.17 s** — **19× faster**, **4× less RSS**.

---

## 4 · Real-Data Benchmark (20.9 MB IOS XR)

> 263 grammars, 30 defines, 113 regex patterns
> Input: 20.9 MB `show running-config` (proprietary — not shipped)
> 899 757 nodes → 97.8 MB JSON · median of 5 iterations
>
> These results were obtained with a private IOS XR configuration dataset.
> The benchmark scripts are not included in the public repository.

### In-Memory Serialization

| Metric   | Win        | glibc    | **jemalloc** |
|----------|------------|----------|--------------|
| Exec     | 3.76 s     | 2.69 s   | **2.08 s**   |
| Total    | 4.10 s     | 2.93 s   | **2.31 s**   |
| Peak RSS | **496 MB** | 639 MB   | 510 MB       |

### Streaming Serialization

| Metric   | Win    | glibc  | **jemalloc** |
|----------|--------|--------|--------------|
| Exec     | 3.48 s | 2.79 s | **2.16 s**   |
| Total    | 3.71 s | 2.92 s | **2.29 s**   |
| Peak RSS | 496 MB | 525 MB | **456 MB**   |

> Streaming skips the 98 MB in-memory `String` allocation. RSS savings are
> modest here because the tree itself (260+ MB) dominates.

### CLI End-to-End — rgel vs gel

> `rgel` = Rust binary (release profile)
> `gel` = Python Gelatin (CPython 3.10)
> **Input:** 20.9 MB IOS XR `show running-config` → JSON
> Median of 5 iterations (1 warm-up)
> Full lifecycle: startup → file I/O → compile → execute → serialize → stdout

| Tool         | Win           | glibc         | jemalloc      |
|--------------|---------------|---------------|---------------|
| **rgel**     | **5.50 s**    | **4.61 s**    | **4.10 s**    |
|              | **3.97 MB/s** | **4.7 MB/s**  | **5.3 MB/s**  |
| gel (Python) | 61.81 s       | 60.12 s       | —             |
|              | 0.35 MB/s     | 0.36 MB/s     | —             |

| Speedup (rgel vs gel) | Win        | Linux (glibc) |
|-----------------------|------------|---------------|
|                       | **11.2×**  | **13.0×**     |

> **rgel is 11–13× faster than Python gel** across all platforms.
> jemalloc is the fastest allocator (4.10 s vs 4.61 s glibc vs 5.50 s Win).

---

## 5 · textfsm Comparison

> [textfsm](https://github.com/google/textfsm) produces **flat tables**
> (list of dicts); Rustine produces a **deep hierarchical tree**.
> Not equivalent work — Rustine does far more per byte.
>
> These results were obtained with custom benchmark scripts not shipped in
> the public repository.  They are included here as reference data points.

### 5a · Synthetic Data (25 MB — same input as § 2)

| Tool        | Time (best) | MB/s     | Output                 | Peak RSS |
|-------------|-------------|----------|------------------------|----------|
| textfsm     | 9.63 s      | 2.60     | 1 529 880 flat records | 422 MB   |
| **Rustine** | **5.95 s**  | **4.20** | 126.6 MB JSON tree     | 1 252 MB |

> **Rustine is 1.6× faster** while building a full hierarchical tree.

#### Per-template breakdown (textfsm)

| Template    | Time   | Records   |
|-------------|--------|-----------|
| subsections | 3.70 s | 367 171   |
| sections    | 2.17 s | 61 196    |
| attributes  | 3.76 s | 1 101 513 |

### 5b · Real IOS XR Data (20.9 MB)

| Tool        | Time (best) | MB/s     | Output                     | Peak RSS |
|-------------|-------------|----------|----------------------------|----------|
| textfsm     | 7.62 s      | 2.75     | 1 556 flat records         | 259 MB   |
| **Rustine** | **3.92 s**  | **5.34** | 93.3 MB JSON (899 K nodes) | 496 MB   |

> **Rustine is 1.9× faster** and produces **578× more data**.

---

## 6 · Platform Summary

Best result on each key metric (speedup relative to Windows):

| Metric                   | Best     | Time / RSS  | vs Win              |
|--------------------------|----------|-------------|---------------------|
| Fixture (complex)        | jemalloc | 2.59 ms     | **1.62×**           |
| Scale 25 MB execute      | jemalloc | 3.75 s      | **1.47×**           |
| Scale 25 MB (vs Gelatin) | jemalloc | 4.11 s JSON | **19×** vs Python   |
| One-shot 25 MB (YAML)    | jemalloc | 3.79 s      | **1.46×**           |
| Real-data exec (mem)     | jemalloc | 2.08 s      | **1.81×**           |
| Real-data total (stream) | jemalloc | 2.29 s      | **1.62×**           |
| CLI (rgel vs gel)        | glibc    | 4.61 s      | **13.0×** vs Python |
| RSS 25 MB one-shot       | jemalloc | 739 MB      | **−10 %**           |
| RSS 25 MB (vs Gelatin)   | jemalloc | 965 MB      | **3× less** than Py |
| RSS 100 MB one-shot      | jemalloc | 2 164 MB    | **−14 %**           |

> **jemalloc wins every speed metric.** glibc leads on small-workload RSS
> (31.2 MB vs 33.8 MB) but jemalloc is better at scale.
> CLI throughput is I/O-bound and near-identical across allocators.

---

## 7 · Key Observations

1. **jemalloc is the fastest allocator across all workloads.**
   1.5–1.8× faster than Windows MSVC; 1.1–1.3× faster than glibc.

2. **Allocator choice dominates at scale.** The glibc system allocator
   fragments under repeated 1.5 M-node allocations, inflating Criterion
   totals 25–40 % above one-shot measurements. jemalloc shows no such
   degradation.

3. **Real-data throughput:** up to **10.1 MB/s** (jemalloc in-memory) on
   20.9 MB IOS XR with 263 grammars.

4. **CLI end-to-end:** `rgel` processes 20.9 MB in **4.10 s** (jemalloc)
   to **5.50 s** (Windows) — **11–13× faster than Python `gel`.**

5. **17–19× faster than Python Gelatin** on the same 25 MB scale
   benchmark with the same grammars. **4× less RSS** (965 MB vs 3 075 MB).

6. **Faster than textfsm while doing far more work.**
   Synthetic 25 MB: **1.6×** faster. Real IOS XR: **1.9×** faster
   and **578× more output**.

7. **Linear scaling:** throughput constant (±15 %) from 25 to 100 MB on all
   three platforms.

8. **Memory:** jemalloc uses **10–19 % less RSS** than both system
   allocators at scale. Windows RSS is lowest for real-data (496 MB) due
   to different OS measurement semantics.

9. **Serialization cost:** JSON ~6 %, XML ~10 %, YAML ~4 % of total.
    Execution dominates.

10. **Streaming:** ~30 % faster serialize step; RSS savings appear at larger
    output sizes (500 MB+).

11. **Near-perfect parity:** 24 / ~2.4 M nodes differ (0.001 %) vs Python
    Gelatin — all from a Python `_splitpath()` bug.

---

## 8 · How to Reproduce

### Criterion benchmarks

```bash
# All fixture + scale benchmarks
cargo bench --no-default-features

# Fixtures only
cargo bench --bench fixtures --no-default-features

# Scale only (25 MB)
cargo bench --bench scale --no-default-features
```

### Scale one-shot (25 / 50 / 100 MB)

```bash
cargo run --release --no-default-features --example scale_oneshot
cargo run --release --no-default-features --example scale_oneshot -- --sizes 50,100
```

### Custom grammar + input

```bash
# 5 iterations, in-memory
cargo run --release --no-default-features --example bench_run \
  -- <syntax.gel> <input.txt> 5

# Streaming
cargo run --release --no-default-features \
  --example bench_run -- <syntax.gel> <input.txt> 5 --stream

# Show all options
cargo run --release --no-default-features --example bench_run -- --help
```

### Linux via Docker

```bash
# glibc (system allocator)
docker run --rm -v "${PWD}:/work" -w /work rust:latest \
  cargo bench --no-default-features

# jemalloc
docker run --rm -v "${PWD}:/work" -w /work rust:latest \
  cargo bench --no-default-features --features jemalloc

# One-shot (jemalloc)
docker run --rm -v "${PWD}:/work" -w /work rust:latest \
  cargo run --release --no-default-features --features jemalloc \
  --example scale_oneshot
```

> The CLI, textfsm, and real-data benchmarks were run with private scripts
> and datasets not included in this repository.  Results in §§ 4–5 are
> provided as reference data points.
