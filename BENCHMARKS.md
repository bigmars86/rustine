# Benchmarks

Rustine compared with Python Gelatin on the same inputs. Each engine
converts the input to JSON in a new process, measured with
[`benches/compare.py`](benches/compare.py).

**Instructions** count the work done, independent of the CPU's speed, so
they stay comparable across x86-64 machines and between versions. Peak
memory depends on the allocator, not on the CPU. Times hold for the
reference PC ([Method](#method)); on other CPUs they and their ratios
differ.

## Results

Linux (x86-64) with jemalloc, like the Linux wheels. *Rust* is Rustine's
Rust API (the `bench_run` example), *Python* the Rustine wheel
(`Rustine.util`) and *Gelatin* Python Gelatin 2.3, both on CPython 3.11.
For reference, *Gelatin, 3.10* is Python Gelatin on CPython 3.10.

| Workload                         | Engine               | Instructions | Peak memory | CPU time |    Time |
|----------------------------------|----------------------|-------------:|------------:|---------:|--------:|
| IOS XR config, 20.6 MB           | Rustine (Rust)       |       14.0 G |      342 MB |   1.08 s |  1.05 s |
|                                  | Rustine (Python)     |       14.8 G |      346 MB |   1.13 s |  1.13 s |
|                                  | Python Gelatin       |        279 G |    1 907 MB |   34.2 s |  34.2 s |
|                                  | Python Gelatin, 3.10 |        358 G |    1 927 MB |   38.8 s |  38.8 s |
| Scale, 25.0 MB                   | Rustine (Rust)       |       15.6 G |      364 MB |   1.50 s |  1.49 s |
|                                  | Rustine (Python)     |       16.6 G |      409 MB |   1.59 s |  1.59 s |
|                                  | Python Gelatin       |        352 G |    3 072 MB |   54.9 s |  55.0 s |
|                                  | Python Gelatin, 3.10 |        486 G |    3 115 MB |   65.9 s |  65.9 s |
| 5 small fixtures, per conversion | Rustine (Rust)       |       20.3 M |       29 MB |  1.60 ms | 1.60 ms |
|                                  | Rustine (Python)     |       21.0 M |       35 MB |  1.74 ms | 1.74 ms |
|                                  | Python Gelatin       |        158 M |       24 MB |  13.8 ms | 13.8 ms |
|                                  | Python Gelatin, 3.10 |        225 M |       22 MB |  19.1 ms | 19.1 ms |

- **Large inputs:** Rustine needs 19–23× fewer instructions than Gelatin,
  is 30–37× faster and uses 5.5–8.4× less memory. The time gap is wider
  than the instruction gap because Gelatin also executes fewer instructions
  per second.
- **Small inputs:** about 8× fewer instructions and 8× faster per
  conversion. Peak memory here is mostly the process itself (interpreter,
  libraries).
- **Gelatin on CPython 3.10** executes 28–42 % more instructions than on
  3.11 and takes 13–38 % longer.
- **Python wheel vs Rust API:** 6 % more instructions, 7–8 % more time and
  up to 12 % more memory on large inputs. The wheel builds Python strings
  for the input and the JSON, compiles the regexes on one thread, and
  reaches jemalloc's thread-local data through the slower `__tls_get_addr`,
  because Python loads it at runtime.

## Python Versions

The Rustine wheel on CPython 3.11 to 3.15, same PC. Python Gelatin only
runs up to 3.11 (SimpleParse), so the table above compares on 3.11.

| Python | Instructions, IOS XR | Peak memory, IOS XR | IOS XR | Scale  | 5 small fixtures |
|--------|---------------------:|--------------------:|-------:|-------:|-----------------:|
| 3.11   |               14.8 G |              346 MB | 1.13 s | 1.59 s |          1.74 ms |
| 3.12   |               14.8 G |              347 MB | 1.16 s | 1.61 s |          1.73 ms |
| 3.13   |               14.8 G |              347 MB | 1.16 s | 1.60 s |          1.80 ms |
| 3.14   |               14.8 G |              348 MB | 1.18 s | 1.62 s |          1.73 ms |
| 3.15   |               14.9 G |              348 MB | 1.17 s | 1.60 s |          1.73 ms |

Instructions and memory differ by less than 1 %: the work happens in Rust,
Python only starts the process and passes the strings. The times vary
within the run-to-run noise of this PC.

## Operating Systems

The engine is the same on every platform; the allocator and the OS are not.
Rustine (Rust) on the same PC, time and peak memory, median of 7 runs; the
builds of each OS ran in turns:

| Platform and build                               | Allocator    | IOS XR config   | Scale, 25 MB    | Scale, 100 MB     | 5 small fixtures |
|--------------------------------------------------|--------------|-----------------|-----------------|-------------------|------------------|
| Linux wheels (glibc)                             | jemalloc     | 1.04 s · 342 MB | 1.48 s · 372 MB | 6.28 s · 1 357 MB | 1.68 ms · 30 MB  |
| Linux (glibc), `--features mimalloc`             | mimalloc     | 0.96 s · 558 MB | 1.37 s · 592 MB | 5.45 s · 1 881 MB | 1.54 ms · 98 MB  |
| Other Linux (glibc) builds, e.g. `cargo install` | glibc malloc | 1.05 s · 305 MB | 1.66 s · 385 MB | 7.11 s · 1 402 MB | 2.08 ms · 23 MB  |
| Linux wheels (musl, e.g. Alpine)                 | jemalloc     | 1.50 s · 341 MB | 2.04 s · 369 MB | 8.84 s · 1 354 MB | 2.07 ms · 27 MB  |
| Linux (musl), `--features mimalloc`              | mimalloc     | 1.40 s · 527 MB | 1.90 s · 615 MB | 7.72 s · 1 850 MB | 1.91 ms · 88 MB  |
| Other Linux (musl) builds                        | musl malloc  | 1.89 s · 275 MB | 2.76 s · 343 MB | 11.5 s · 1 316 MB | 4.42 ms · 20 MB  |
| Windows wheels                                   | mimalloc     | 1.05 s · 324 MB | 1.46 s · 332 MB | 5.95 s · 1 223 MB | 2.65 ms · 40 MB  |
| Other Windows builds                             | Windows heap | 1.32 s · 320 MB | 1.92 s · 371 MB | 7.77 s · 1 394 MB | 2.57 ms · 28 MB  |

- **System allocators** take longer than the allocators of the wheels:
  glibc malloc 1–24 %, musl malloc 26–35 % on large inputs and 2.1× on
  small ones, the Windows heap 26–32 % on large inputs and 3 % less on the
  small fixtures. On Linux they need 11–19 % less memory on the IOS XR
  config and 7 MB less on the small fixtures; on the scale inputs memory
  differs by up to 7 %. The Windows heap needs about as much memory as
  mimalloc on the IOS XR config, 12–14 % more on the scale inputs and
  12 MB less on the small fixtures.
- **mimalloc on Linux** takes 7–13 % less time than jemalloc but needs
  37–67 % more memory on large inputs (about 500 MB more at 100 MB) and
  61–68 MB more on small ones, so the Linux wheels use jemalloc. jemalloc
  does not build for MSVC; the Windows wheels use mimalloc.
- **mimalloc v3** is the version the mimalloc developers recommend and the
  default of the mimalloc crate; the builds use it. Against v2 it needs
  17–39 % more memory on Linux at the same speed (within 3 %). On Windows
  it needs 7–18 % less memory and is 2–4 % faster on large inputs, but
  takes about 40 % longer on the small fixtures (0.8 ms per conversion).
- With mimalloc, **Windows** is within 5 % of Linux with jemalloc on large
  inputs. **musl** builds take 1.2–1.4× the time of glibc builds.
- **macOS** uses its system allocator and was not measured.

## Method

**Workloads**

- *IOS XR config*: a pseudonymized `show running-config` of 20.6 MB,
  shipped xz-packed as `fixtures/parity/complex/input2.txt.xz`, with
  `syntax2.gel` (45 KB: the 146 grammars and 424 rules this input uses);
  862 744 output nodes, 90 MB of JSON. Gelatin's tree has 24 more nodes
  (from a bug in its `_splitpath()`), so both process the whole file.
- *Scale*: a synthetic 25.0 MB input and an 86 KB grammar with 302 grammars
  and about 1 110 rules, written by `scale_oneshot --write`; 1 521 168 nodes.
  *Scale, 100 MB*: the same grammar with a 100 MB input.
- *Small fixtures*: the five `fixtures/parity/*/input1.txt` files
  (130 B – 15 KB) with their grammars, summed.

**Runs**: each large workload is one conversion per process, including
startup, reading the files and compiling the grammar: median of 5 runs
after a warm-up, highest peak memory. For the small fixtures a process
converts the same input *N* times; one conversion costs
(cost of *N* − cost of 1) / (*N* − 1), so startup and compilation drop out.
Between sessions, the times of the same build differed by up to 14 %, its
peak memory by less than 1 %; compare times within one table.

**Metrics**

- *Instructions*: counted by Valgrind for the whole process, with one Rayon
  thread and a fixed Python hash seed; repeated runs agree within 1 %.
  Valgrind shows the program the same CPU on every x86-64 machine with
  AVX2, so the count changes only with the code, compiler and libraries.
  G = 10⁹, M = 10⁶ instructions.
- *Peak memory*: maximum resident set size (Windows: peak working set);
  MB = 2²⁰ bytes.
- *CPU time*: user and system time of all threads. It exceeds the time when
  the Rust build compiles the regexes in parallel (`parallel` feature).
- *Time*: wall-clock time.

**Reference PC**: Intel Core i7-13850HX. Linux: Docker Desktop (WSL 2),
Debian 13, 4 CPUs (`--cpuset-cpus=4-7`). Windows 11 natively, limited to
4 logical CPUs, with `bench_run.exe` built for `x86_64-pc-windows-gnu`
(same allocators as the MSVC builds). Rust 1.99.1, CPython 3.10 to 3.15
from the official Docker images.

## Reproduce

```bash
# Rust engine and scale workload, built like the Linux wheels
cargo build --release --no-default-features --features parallel,jemalloc \
  --example bench_run --example scale_oneshot
target/release/examples/scale_oneshot --write data --sizes 25

# Rustine wheel
pip install maturin
maturin build --release --features python,jemalloc -o dist

# Separate environments: the Rustine wheel contains a Gelatin compatibility
# package. Python 3.11 is the only version both support (SimpleParse fails on 3.12+)
python3.11 -m venv venv-rustine && venv-rustine/bin/pip install dist/rustine-*.whl
python3.11 -m venv venv-gelatin && venv-gelatin/bin/pip install Gelatin

python3 benches/compare.py --instructions \
  --rust target/release/examples/bench_run \
  --rustine venv-rustine/bin/python --gelatin venv-gelatin/bin/python \
  --workload ios-xr fixtures/parity/complex/syntax2.gel fixtures/parity/complex/input2.txt.xz \
  --workload scale data/scale.gel data/scale-25mb.txt \
  --small fixtures/parity
```

`compare.py` unpacks `.xz` and `.gz` inputs itself. `--instructions` needs
Valgrind and takes long: Python runs about 50× slower under it. Without it
the table has no instruction counts. Add `--workload NAME GRAMMAR INPUT`
for your own data. On CPUs with performance and efficiency cores, pin the
run to a few cores for stable times (Docker: `--cpuset-cpus=4-7`).

For the [Operating Systems](#operating-systems) table, build `bench_run`
with `--features parallel` (system allocator), `parallel,jemalloc` or
`parallel,mimalloc`, also with `--target x86_64-unknown-linux-musl` or
`--target x86_64-pc-windows-gnu`; write the inputs with `--sizes 25,100`.
Pass each build as `--rust NAME=PATH` to one `compare.py --runs 7` and add
`--workload scale-100 data/scale.gel data/scale-100mb.txt`. The mimalloc
builds use mimalloc 3.3.2; for 2.3.2 (v2), add `features = ["v2"]` to
`mimalloc-dep` in `Cargo.toml`.

While developing, the Criterion benchmarks in `benches/*.rs`
(`cargo bench --no-default-features`) time execution and serialization
separately; their numbers are not comparable with the table above.
