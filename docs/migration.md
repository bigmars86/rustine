# Migrating from Python Gelatin

Rustine is a **drop-in replacement** for Python Gelatin. This guide covers
what changed, what stayed the same, and how to switch.

---

## Why Migrate?

Python Gelatin depends on **SimpleParse**, a C extension that has not been
updated since 2020 and does not support Python 3.12+. This makes Gelatin
unusable on modern Python installations.

Rustine eliminates this dependency by reimplementing the entire engine in
Rust, exposed to Python via PyO3. It works on Python 3.11 and newer.

---

## What Stays the Same

- **Gel grammar syntax** — 100% compatible. All `.gel` files work unchanged.
- **Output format** — identical JSON and XML output; YAML output loads
  as the same data as the JSON output.
- **Grammar semantics** — match, imatch, when, skip, define, grammar
  inheritance, all output actions, all control flow actions, triggers.
- **Python API shape** — `compile()` → `generate()` workflow.

---

## What Changed

### Installation

```diff
- pip install Gelatin        # requires SimpleParse → fails on Python 3.12+
+ pip install rustine         # native wheels, no C extension dependency
```

### Import Path

```diff
- from Gelatin.util import compile, generate
+ from Rustine.util import compile, generate
```

The bundled `Gelatin` package maps `Gelatin.util` to `Rustine.util`, so
existing imports keep working.

### New APIs

```python
from Rustine import rustine

ctx = rustine.compile_grammar(grammar_text)              # compile once
ctx.run(input_text)                                      # JSON; also run_xml / run_yaml
rustine.run_grammar(grammar_text, "input", input_text)   # compile + run in one call
```

### CLI Tool

```diff
- gel -s syntax.gel -f json input.txt
+ rgel -s syntax.gel -f json input.txt
```

The `rgel` binary is a compiled Rust executable — no Python runtime needed.

### Optional: Case-Preserving Output

Tag names and attribute keys in `out.*` paths are lowercased by default, so
existing grammars produce the same output. `--preserve-case` (CLI) or
`preserve_case=True` (Python) keeps them as written.

---

## Feature Parity

Rustine implements **100% of the original Python Gelatin feature set**:

| Category | Features | Status |
|----------|----------|--------|
| Grammar keywords | define, grammar, match, imatch, when, skip | ✅ |
| Output actions | create, add, replace, add_attribute, set_root_name, open, enter, leave | ✅ |
| Trigger actions | enqueue_before/after/on_add/on_leave, _persist variants, clear_queue | ✅ |
| Control flow | do.return, do.next, do.skip, do.say, do.warn, do.fail | ✅ |
| Captures | Positional ($N), named ($name), interpolation in paths/values | ✅ |
| Grammar inheritance | Recursive chain resolution with cycle detection | ✅ |
| Output formats | JSON, XML, YAML | ✅ |
| Path attributes | URL-encoded notation (?key="value"&key2="value2") | ✅ |
| Alternation | match pattern1 \| pattern2 | ✅ |

### Known Differences

#### Line Ending Handling (CRLF vs LF)

Python Gelatin only works with LF (`\n`) line endings. On Windows, input
files must be pre-converted or opened in text mode.

Rustine handles **both LF and CRLF** natively — no pre-processing needed.
Grammar patterns use `\r?\n` instead of `[\r\n]` to treat `\r\n` as a
single line ending rather than two separate characters:

```diff
- define nl /[\r\n]/          # Python Gelatin style
+ define nl /\r?\n/           # Rustine (CRLF-safe)

- match /[^\r\n,]+/ /[\r\n,] */:
+ match /[^\r\n,]+/ /(?:\r?\n|,) */:
```

If you are migrating an existing `.gel` grammar, update `[\r\n]` patterns
to `\r?\n` for cross-platform compatibility. The `[\r\n]` character
class still works but splits `\r\n` into two separate matches (matching
`\r` first, then `\n`), which may cause unexpected behaviour on
Windows-style line endings.

#### Encodings

Rustine reads and writes UTF-8 only. The `encoding`, `in_encoding` and
`out_encoding` parameters stay for compatibility, but any value other than
UTF-8 (`'utf-8'`, `'utf8'`, …) raises `ValueError`. Convert other files to
UTF-8 first.

#### Output Parity

Rustine produces **near-perfect output parity** with Python Gelatin. Across
~2.4 million output nodes in the test suite, only 24 nodes differ (0.001%).
All differences trace to a bug in Python Gelatin's `_splitpath()` function,
not in Rustine.

---

## Performance Gains

The Rustine wheel compared with Python Gelatin, each converting the input
to JSON in a new process on the same Linux PC; details in
[BENCHMARKS.md](../BENCHMARKS.md).

| Workload                       | Python Gelatin   | Rustine        | Improvement                      |
|--------------------------------|------------------|----------------|----------------------------------|
| 20.6 MB IOS XR config          | 34.2 s, 1 907 MB | 1.13 s, 346 MB | **30× faster, 5.5× less memory** |
| 25.0 MB synthetic input        | 55.0 s, 3 072 MB | 1.59 s, 409 MB | **35× faster, 7.5× less memory** |
| 5 small inputs, per conversion | 13.8 ms          | 1.74 ms        | **7.9× faster**                  |

---

## Step-by-Step Migration

1. **Install Rustine:**
   ```bash
   pip install rustine
   ```

2. **Test existing grammars** — run your `.gel` files through Rustine and
   compare output. The included `Gelatin` shim means most code works without
   changes.

3. **Update imports** (optional):
   ```python
   from Rustine.util import compile, generate
   ```

4. **Update CLI scripts:**
   ```bash
   # Old
   gel -s syntax.gel -f json input.txt

   # New
   rgel -s syntax.gel -f json input.txt
   ```

5. **Remove Gelatin dependency** once migration is verified:
   ```bash
   pip uninstall Gelatin
   ```
