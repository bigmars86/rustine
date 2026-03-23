# Migrating from Python Gelatin

Rustine is a **drop-in replacement** for Python Gelatin. This guide covers
what changed, what stayed the same, and how to switch.

---

## Why Migrate?

Python Gelatin depends on **SimpleParse**, a C extension that has not been
updated since 2020 and does not support Python 3.12+. This makes Gelatin
unusable on modern Python installations.

Rustine eliminates this dependency by reimplementing the entire engine in
Rust, exposed to Python via PyO3. It works on Python 3.9 through 3.13+.

---

## What Stays the Same

- **Gel grammar syntax** — 100% compatible. All `.gel` files work unchanged.
- **Output format** — identical JSON, XML, and YAML output.
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
+ from Rustine import rustine

# Or use the backward-compatible shim:
+ from Gelatin.util import compile, generate   # still works
```

The `Gelatin` package is included as a compatibility shim that delegates to
Rustine internally.

### New APIs

Rustine adds direct parsing functions that don't require a compilation step:

```python
from Rustine import rustine

# One-step parse (grammar source + input text → output)
json_output = rustine.parse_to_json(grammar_text, input_text)
xml_output  = rustine.parse_to_xml(grammar_text, input_text)
yaml_output = rustine.parse_to_yaml(grammar_text, input_text)

# Full execution results (with traces, diagnostics, etc.)
result = rustine.run_grammar(grammar_text, input_text)
```

### CLI Tool

```diff
- gel -s syntax.gel -f json input.txt
+ rgel -s syntax.gel -f json input.txt
```

The `rgel` binary is a compiled Rust executable — no Python runtime needed.

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
to `\r?\n` for cross-platform compatibility. The old `[\r\n]` character
class still works but splits `\r\n` into two separate matches (matching
`\r` first, then `\n`), which may cause unexpected behaviour on
Windows-style line endings.

#### Output Parity

Rustine produces **near-perfect output parity** with Python Gelatin. Across
~2.4 million output nodes in the test suite, only 24 nodes differ (0.001%).
All differences trace to a bug in Python Gelatin's `_splitpath()` function,
not in Rustine.

---

## Performance Gains

| Metric | Python Gelatin | Rustine (best) | Improvement |
|--------|---------------|----------------|-------------|
| 25 MB scale (JSON) | 77.6 s | 4.11 s | **19×** |
| 20.9 MB real-data | 66.3 s | 2.29 s | **29×** |
| Peak RSS (25 MB) | 3 075 MB | 739 MB | **4× less** |
| Fixture: complex | 43 ms | 2.59 ms | **17×** |

---

## Step-by-Step Migration

1. **Install Rustine:**
   ```bash
   pip install rustine
   ```

2. **Test existing grammars** — run your `.gel` files through Rustine and
   compare output. The included `Gelatin` shim means most code works without
   changes.

3. **Update imports** (optional but recommended):
   ```python
   from Rustine import rustine
   result = rustine.parse_to_json(open('syntax.gel').read(), input_text)
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
