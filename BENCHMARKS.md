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
(`Rustine.util`), *Gelatin* Python Gelatin 2.3 on CPython 3.10.

| Workload                         | Engine           | Instructions | Peak memory | CPU time |    Time |
|----------------------------------|------------------|-------------:|------------:|---------:|--------:|
| IOS XR config, 20.0 MB           | Rustine (Rust)   |       14.1 G |      381 MB |   1.08 s |  1.02 s |
|                                  | Rustine (Python) |       15.0 G |      422 MB |   1.24 s |  1.24 s |
|                                  | Python Gelatin   |        357 G |    1 919 MB |   41.7 s |  41.7 s |
| Scale, 25.0 MB                   | Rustine (Rust)   |       15.6 G |      363 MB |   1.52 s |  1.51 s |
|                                  | Rustine (Python) |       16.7 G |      457 MB |   1.65 s |  1.65 s |
|                                  | Python Gelatin   |        486 G |    3 099 MB |   67.2 s |  67.2 s |
| 5 small fixtures, per conversion | Rustine (Rust)   |       20.3 M |       28 MB |  1.70 ms | 1.70 ms |
|                                  | Rustine (Python) |       21.0 M |       33 MB |  1.72 ms | 1.72 ms |
|                                  | Python Gelatin   |        225 M |       22 MB |  18.8 ms | 18.8 ms |

- **Large inputs:** Rustine needs 24–31× fewer instructions than Gelatin,
  is 34–45× faster and uses 4.5–8.5× less memory. The time gap is wider
  than the instruction gap because Gelatin also executes fewer instructions
  per second.
- **Small inputs:** 11× fewer instructions and 11× faster per conversion.
  Peak memory here is mostly the process itself (interpreter, libraries).
- **Python wheel vs Rust API:** 6–7 % more instructions, up to 22 % more
  time and 26 % more memory on large inputs. The wheel builds Python strings
  for the input and the JSON, compiles the regexes on one thread, and reaches
  jemalloc's thread-local data through the slower `__tls_get_addr`, because
  Python loads it at runtime.

## Operating Systems

The engine is the same on every platform; the allocator and the OS are not.
Rustine (Rust) on the same PC, time and peak memory:

| Platform and build                               | Allocator    | IOS XR config   | Scale           | 5 small fixtures |
|--------------------------------------------------|--------------|-----------------|-----------------|------------------|
| Linux wheels (glibc), `--features jemalloc`      | jemalloc     | 1.02 s · 381 MB | 1.51 s · 363 MB | 1.70 ms · 28 MB  |
| Other Linux (glibc) builds, e.g. `cargo install` | glibc malloc | 1.00 s · 348 MB | 1.64 s · 385 MB | 2.01 ms · 23 MB  |
| Windows wheels and builds                        | Windows heap | 1.37 s · 360 MB | 1.92 s · 369 MB | 2.79 ms · 28 MB  |

Windows takes 1.3× (large inputs) to 1.6× (small inputs) the time of Linux
with jemalloc, with about the same memory. glibc malloc executes 6–12 % more
instructions than jemalloc and takes up to 18 % more time. macOS and the
musl wheels (Alpine) use their system allocators and were not measured.

## Method

**Workloads**

- *IOS XR config*: a pseudonymized `show running-config` of 20.0 MB (not
  shipped) and a 113 KB grammar with 263 grammars and 113 regex patterns;
  862 754 output nodes, 89 MB of JSON. Gelatin's tree has 24 more nodes
  (from a bug in its `_splitpath()`), so both process the whole file.
- *Scale*: a synthetic 25.0 MB input and an 86 KB grammar with 302 grammars
  and about 1 110 rules, written by `scale_oneshot --write`; 1 521 168 nodes.
- *Small fixtures*: the five `fixtures/parity/*/input1.txt` files
  (130 B – 15 KB) with their grammars, summed.

**Runs**: each large workload is one conversion per process, including
startup, reading the files and compiling the grammar: median of 5 runs
after a warm-up, highest peak memory. For the small fixtures a process
converts the same input *N* times; one conversion costs
(cost of *N* − cost of 1) / (*N* − 1), so startup and compilation drop out.

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
(same system allocator as the MSVC builds). Rust 1.99.1, CPython 3.10.

## Reproduce

```bash
# Rust engine and scale workload, built like the Linux wheels
cargo build --release --no-default-features --features parallel,mmap,jemalloc \
  --example bench_run --example scale_oneshot
target/release/examples/scale_oneshot --write data --sizes 25

# Rustine wheel
pip install maturin
maturin build --release --features python,jemalloc -o dist

# Separate environments: the Rustine wheel contains a Gelatin compatibility
# package, and Gelatin needs Python 3.11 or older (SimpleParse)
python3.10 -m venv venv-rustine && venv-rustine/bin/pip install dist/rustine-*.whl
python3.10 -m venv venv-gelatin && venv-gelatin/bin/pip install Gelatin

python3 benches/compare.py --instructions \
  --rust target/release/examples/bench_run \
  --rustine venv-rustine/bin/python --gelatin venv-gelatin/bin/python \
  --workload scale data/scale.gel data/scale-25mb.txt \
  --small fixtures/parity
```

`--instructions` needs Valgrind and takes long: Python runs about 50× slower
under it. Without it the table has no instruction counts. Add
`--workload NAME GRAMMAR INPUT` for your own data. On CPUs with performance
and efficiency cores, pin the run to a few cores for stable times
(Docker: `--cpuset-cpus=4-7`).

While developing, the Criterion benchmarks in `benches/*.rs`
(`cargo bench --no-default-features`) time execution and serialization
separately; their numbers are not comparable with the table above.
