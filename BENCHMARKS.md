# Benchmark Results

> **Measured:** 2026-11-11
> **Machine:** Intel Core i7-13850HX · Docker Desktop (WSL 2), Debian 13 · 4 dedicated CPUs (`--cpuset-cpus=4-7`)
> **Rust:** 1.99.1 (`codegen-units=1, lto=true, opt-level=3`) · MB = 2²⁰ bytes
>
> | Label        | Allocator                            |
> |--------------|--------------------------------------|
> | glibc        | System allocator, glibc 2.41         |
> | **jemalloc** | jemalloc 5.3 (`--features jemalloc`) |
>
> Windows, Python Gelatin and textfsm were not measured again. § 7 keeps their
> results from 2026-03-23 (AMD Ryzen, older Rustine); they are not comparable
> one to one with §§ 1–6.

---

## 1 · Fixture Benchmarks

> Grammar compiled once; each iteration executes and serializes to JSON
> **Criterion** — 100 samples, 5 s measurement, 1 s warm-up

| Fixture   | Input   | glibc      | **jemalloc** |
|-----------|---------|------------|--------------|
| complex   | 14.9 KB | 2.14 ms    | **1.60 ms**  |
| csv       | 130 B   | 36.5 µs    | **30.5 µs**  |
| linesplit | 124 B   | **7.4 µs** | 7.5 µs       |
| simple    | 144 B   | 30.0 µs    | **25.8 µs**  |
| tria      | 1.3 KB  | 135.1 µs   | **108.5 µs** |

> Peak RSS — **glibc: 27.2 MB** · jemalloc: 28.9 MB

---

## 2 · Scale Benchmark (25 MB synthetic)

> 302 grammars, ~1 110 rules, 6 action-type patterns
> Dynamically generated data — 1 521 168 tree nodes
> **Criterion** — 10 samples, 60 s measurement, 5 s warm-up

| Benchmark      | glibc      | **jemalloc** |
|----------------|------------|--------------|
| Execute        | 1.34 s     | **1.25 s**   |
| Serialize JSON | **167 ms** | 217 ms       |
| Serialize XML  | **314 ms** | 466 ms       |
| Serialize YAML | **147 ms** | 175 ms       |
| Total JSON     | 2.15 s ¹   | **1.42 s**   |
| Total XML      | 2.22 s ¹   | **1.61 s**   |
| Total YAML     | 2.20 s ¹   | **1.39 s**   |
| Peak RSS       | 617 MB     | **530 MB**   |

> ¹ glibc totals are inflated by allocator fragmentation under Criterion's
> repeated alloc / free of 1.5 M nodes; the one-shot runs (§ 3) do not show it.

---

## 3 · Scale One-Shot (25 / 50 / 100 MB)

> `cargo run --release --example scale_oneshot -- --sizes 25,50,100`
> Single wall-clock run — no Criterion warm-up.

### Total Time (execute + serialize)

| Size · Format  | glibc  | **jemalloc** |
|----------------|--------|--------------|
| 25 MB · JSON   | 1.49 s | **1.41 s**   |
| 25 MB · XML    | 1.68 s | **1.52 s**   |
| 25 MB · YAML   | 1.60 s | **1.22 s**   |
| 50 MB · JSON   | 3.75 s | **2.71 s**   |
| 50 MB · XML    | 3.70 s | **3.16 s**   |
| 50 MB · YAML   | 3.63 s | **2.71 s**   |
| 100 MB · JSON  | 7.38 s | **6.37 s**   |
| 100 MB · XML   | 8.35 s | **6.60 s**   |
| 100 MB · YAML  | 7.07 s | **6.04 s**   |

### Peak RSS

| Size   | glibc    | **jemalloc** |
|--------|----------|--------------|
| 25 MB  | 397 MB   | **358 MB**   |
| 50 MB  | 757 MB   | **716 MB**   |
| 100 MB | 1 400 MB | **1 284 MB** |

> Close to linear scaling: jemalloc executes 15–20 MB/s at every size and
> uses **5–10 % less RSS** than glibc.

---

## 4 · Real-Data Benchmark (20.0 MB IOS XR)

> 263 grammars, 30 defines, 113 regex patterns (119 KB grammar)
> Input: 20.0 MB pseudonymized `show running-config`, LF line endings (not shipped)
> 862 754 nodes → 89.3 MB JSON · `bench_run`, median of 5 iterations

### In-Memory Serialization

| Metric   | glibc  | **jemalloc** |
|----------|--------|--------------|
| Exec     | 0.93 s | **0.85 s**   |
| Total    | 1.04 s | **0.96 s**   |
| Peak RSS | 405 MB | **385 MB**   |

### Streaming Serialization

| Metric   | glibc  | **jemalloc** |
|----------|--------|--------------|
| Exec     | 0.89 s | **0.78 s**   |
| Total    | 0.92 s | **0.81 s**   |
| Peak RSS | 312 MB | **305 MB**   |

> Streaming skips the 89 MB output `String` and lowers peak RSS by 80–95 MB.

### CLI End-to-End (`rgel`)

> Full lifecycle: startup → file I/O → compile → execute → serialize → `/dev/null`
> Median of 5 runs after 1 warm-up

| Output   | glibc              | **jemalloc**           |
|----------|--------------------|------------------------|
| JSON     | 1.16 s · 17.3 MB/s | **1.08 s · 18.6 MB/s** |
| XML      | 1.24 s · 16.2 MB/s | **1.08 s · 18.5 MB/s** |
| Peak RSS | **344 MB**         | 374 MB                 |

---

## 5 · Compared with the Previous Version

> Same machine and settings; previous version = commit `1a373df`.
> `bench_run`, jemalloc, one iteration, median of 7 interleaved runs.
> Wall time includes reading the files and compiling the grammar;
> Total = execute + serialize to JSON. Instructions counted with callgrind.

| Workload             | Metric       | Before | Now    | Change    |
|----------------------|--------------|--------|--------|-----------|
| Real IOS XR, 20.0 MB | Wall time    | 2.77 s | 1.10 s | **−60 %** |
|                      | Total        | 1.82 s | 0.96 s | **−47 %** |
|                      | Peak RSS     | 479 MB | 373 MB | **−22 %** |
|                      | Instructions | 46.7 G | 14.1 G | **−70 %** |
| Scale, 25 MB         | Wall time    | 4.01 s | 1.44 s | **−64 %** |
|                      | Total        | 3.10 s | 1.38 s | **−56 %** |
|                      | Peak RSS     | 741 MB | 357 MB | **−52 %** |

> Main reasons: the grammar is lexed in linear time (the 119 KB grammar took
> 0.8 s), identical field-list regexes are compiled once, per-match metadata
> is skipped when only the tree is needed, `out.*` paths are parsed without a
> cache, and capture interpolation, regex matching and output escaping no
> longer do duplicate or per-character work.

---

## 6 · Key Observations

1. **Real IOS XR config:** 20 MB executed in **0.85 s** (23.6 MB/s) and in
   0.96 s including JSON (jemalloc); `rgel` takes **1.08 s** end to end.

2. **2.5× faster than the previous version** end to end on the real config
   with 22 % less memory, and **2.8× faster** on the 25 MB scale benchmark
   with half the memory (§ 5).

3. **jemalloc is the fastest allocator** for execution and totals. glibc
   serializes faster (JSON 167 vs 217 ms on 25 MB) and uses less memory for
   small inputs and the CLI.

4. **glibc fragments under Criterion's repeated runs:** its totals are
   35–50 % above execute + serialize; one-shot runs do not show this.

5. **Close to linear scaling** from 25 to 100 MB.

6. **Serialization cost** (jemalloc, 25 MB one-shot): JSON 14 %, XML 22 %,
   YAML 12 % of the total. Execution dominates.

7. **Streaming** lowers peak RSS by 80–95 MB on the real config.

8. **Near-perfect parity:** 24 / ~2.4 M nodes differ (0.001 %) vs Python
   Gelatin — all from a Python `_splitpath()` bug.

---

## 7 · Reference Results (2026-03-23)

> Not measured again: Windows (MSVC), Python Gelatin (CPython 3.10) and
> textfsm, on an AMD Ryzen machine with the Rustine of that date. Rustine is
> faster now (§ 5), so the ratios below are conservative.

### Python Gelatin — Fixtures

| Fixture   | Input   | Gelatin Win (JSON) | Gelatin Linux (JSON) | Rustine (jemalloc) |
|-----------|---------|--------------------|----------------------|--------------------|
| complex   | 14.9 KB | 43 ms              | 48 ms                | 2.59 ms            |
| csv       | 130 B   | 2.0 ms             | 2.0 ms               | 46.1 µs            |
| linesplit | 124 B   | 0.5 ms             | 0.4 ms               | 15.0 µs            |
| simple    | 144 B   | 1.0 ms             | 1.0 ms               | 35.1 µs            |
| tria      | 1.3 KB  | 3.0 ms             | 4.0 ms               | 138.5 µs           |

> Gelatin peak RSS — Win: 24.4 MB · Linux: 21.6 MB. Gelatin does not separate
> execute from serialize. Rustine (jemalloc) was **10–17× faster** on fixtures.

### Python Gelatin — 25 MB Scale (same grammars + data)

| Benchmark                | Gelatin Win | Gelatin Linux | Rustine (jemalloc) |
|--------------------------|-------------|---------------|--------------------|
| Criterion total JSON     | 77.6 s      | 81.8 s        | 4.11 s             |
| Criterion total XML      | 69.7 s      | 74.4 s        | 4.40 s             |
| Criterion peak RSS       | 3 074 MB    | 3 085 MB      | 965 MB             |
| One-shot JSON            | 85.1 s      | 80.0 s        | 4.17 s             |
| One-shot XML             | 79.3 s      | 73.1 s        | 4.31 s             |
| One-shot peak RSS        | 3 075 MB    | 3 087 MB      | 739 MB             |

> Rustine (jemalloc) was **17–19× faster** and used **3–4× less memory**.

### CLI End-to-End — rgel vs gel (20.9 MB IOS XR → JSON)

| Tool         | Win     | glibc   | jemalloc |
|--------------|---------|---------|----------|
| **rgel**     | 5.50 s  | 4.61 s  | 4.10 s   |
| gel (Python) | 61.81 s | 60.12 s | —        |

> `rgel` was **11–13× faster** than Python `gel`.

### Windows (MSVC) vs Linux

| Benchmark                  | Win     | glibc   | jemalloc |
|----------------------------|---------|---------|----------|
| Fixture complex            | 4.20 ms | 2.98 ms | 2.59 ms  |
| Scale 25 MB execute        | 5.53 s  | 5.02 s  | 3.75 s   |
| One-shot 25 MB JSON        | 7.07 s  | 5.40 s  | 4.17 s   |
| Real data exec (20.9 MB)   | 3.76 s  | 2.69 s  | 2.08 s   |
| Real data peak RSS         | 496 MB  | 639 MB  | 510 MB   |

> jemalloc was 1.5–1.8× faster than Windows MSVC.

### textfsm

> [textfsm](https://github.com/google/textfsm) produces **flat tables**
> (list of dicts); Rustine produces a **deep hierarchical tree**.
> Not equivalent work — Rustine does far more per byte.
>
> These results were obtained with custom benchmark scripts not shipped in
> the public repository.  They are included here as reference data points.

#### Synthetic Data (25 MB — same input as § 2)

| Tool        | Time (best) | MB/s     | Output                 | Peak RSS |
|-------------|-------------|----------|------------------------|----------|
| textfsm     | 9.63 s      | 2.60     | 1 529 880 flat records | 422 MB   |
| **Rustine** | **5.95 s**  | **4.20** | 126.6 MB JSON tree     | 1 252 MB |

> **Rustine is 1.6× faster** while building a full hierarchical tree.

##### Per-template breakdown (textfsm)

| Template    | Time   | Records   |
|-------------|--------|-----------|
| subsections | 3.70 s | 367 171   |
| sections    | 2.17 s | 61 196    |
| attributes  | 3.76 s | 1 101 513 |

#### Real IOS XR Data (20.9 MB)

| Tool        | Time (best) | MB/s     | Output                     | Peak RSS |
|-------------|-------------|----------|----------------------------|----------|
| textfsm     | 7.62 s      | 2.75     | 1 556 flat records         | 259 MB   |
| **Rustine** | **3.92 s**  | **5.34** | 93.3 MB JSON (899 K nodes) | 496 MB   |

> **Rustine is 1.9× faster** and produces **578× more data**.

---

## 8 · How to Reproduce

### Criterion benchmarks

```bash
# All benchmarks (exec + fixtures + scale)
cargo bench --no-default-features

# Execution engine micro-benchmarks (scaling, serialization, captures)
cargo bench --bench exec --no-default-features

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

> The real-data, CLI and textfsm benchmarks use private scripts and datasets
> that are not part of this repository (§§ 4, 5 and 7). On CPUs with
> performance and efficiency cores, pin the container to a few cores for
> stable numbers, e.g. `docker run --cpuset-cpus=4-7 …`.
