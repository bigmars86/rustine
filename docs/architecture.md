# Architecture

## Pipeline Overview

Rustine processes text through a four-stage pipeline:

```
Source Text → Lexer → Parser → Executor → Serializer
                                  ↓
                            Output Tree
```

Each stage is independent and can be used standalone via the Rust API.

---

## Module Layout

```
src/
  lib.rs          PyO3 module definition + public API re-exports
  bridge.rs       Python-facing functions (parse_to_json, run_grammar, etc.)
  main.rs         CLI binary (feature-gated behind `cli`)
  parser/
    lexer.rs      Tokenizer — keywords, regex, strings, numbers, punctuation
    ast.rs        AST node types (Grammar, Statement, Action, Define)
    core.rs       Recursive descent parser (tokens → AST)
    json.rs       JSON AST serializer
    xml.rs        XML AST serializer
    mod.rs        Module re-exports, grammar resolution, validation
  exec/
    mod.rs        Execution engine, output builder, trigger system
  stream/
    mod.rs        Streaming lexer, chunk reader, StreamingRunner
```

---

## Lexer

The lexer produces a flat token stream from grammar source text:

- **Keywords:** `grammar`, `match`, `imatch`, `when`, `skip`, `define`
- **Identifiers:** including dotted paths (`out.create`) and names with
  `-`, `.`, `/`, `@`
- **Regex literals:** `/pattern/`
- **String literals:** single and double quoted, with escape sequences
- **Numbers**
- **Punctuation:** `(`, `)`, `{`, `}`, `:`, `,`, `|`
- **Comments:** `#` to end of line

---

## Parser

A hand-written recursive descent parser produces an AST of:

- **Grammar** nodes — named, with optional parent for inheritance
- **Statement** nodes — match, imatch, when, skip; each with patterns and
  an action block
- **Action** nodes — function calls (`out.create(...)`, `do.return()`) with
  arguments
- **Define** nodes — named pattern definitions

Grammar inheritance is resolved via recursive chain resolution with cycle
detection before execution.

### Semantic Validation

A post-parse validation pass (`parser::validate`) runs before execution:

- Regex patterns are compiled eagerly — invalid patterns are caught early.
- Parent grammars referenced in inheritance must exist.
- Calls to undefined grammars and references to undefined variables produce
  warnings.

---

## Executor

The execution engine processes input against a resolved grammar:

1. Compile definitions and resolve grammar inheritance chains.
2. Start at the `input` grammar. For each line of input:

   a. Iterate through grammar statements top-to-bottom.
   b. Match patterns against the current input position.
   c. On match: execute the action block, extract captures, fire triggers.
   d. Advance the input position by the consumed bytes.
3. Grammar calls push/pop a grammar stack.
4. `do.return()` returns to the calling grammar.

### Output Builder

The output builder maintains:

- **Node tree** — rooted hierarchical tree with children, text, and ordered
  attributes.
- **Open/enter stack** — tracks the current insertion point (like a cursor).
- **Trigger queues** — before, after, on_add, on_leave; each with
  single-shot and persistent variants.

### Key Design Decisions

- **Arena allocation** — `bumpalo`-backed arena for temporary allocations
  during execution, pre-sized based on input length.
- **O(1) child lookup** — child-group index maps for constant-time
  `out.add` disambiguation.
- **Lazy regex caching** — regexes compiled once per (pattern, case-flag)
  pair, stored in a flat Vec indexed by slot.
- **Cow captures** — `Cow<'a, str>` for zero-copy when captures aren't
  modified by interpolation.

---

## Serializer

Three output formats are supported:

| Format | Implementation | Notes |
|--------|---------------|-------|
| JSON   | Custom streaming writer | Direct to `Write` sink |
| XML    | `quick-xml` + `AposWriter` adapter | Handles `&apos;` escaping |
| YAML   | Custom lightweight emitter | No external deps; `Cow<str>` for quoting |

All serializers implement the `OutputSink` trait and can stream directly to
files, stdout, or in-memory buffers.

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `pyo3` | Python bindings (feature-gated) |
| `regex` | Pattern matching engine |
| `serde` + `serde_json` | JSON serialization |
| `quick-xml` | XML serialization |
| `bumpalo` | Arena allocator |
| `smallvec` | Stack-allocated small vectors |
| `criterion` | Benchmarking framework (dev) |
| `pretty_assertions` | Test output diffs (dev) |
| `tikv-jemallocator` | jemalloc allocator (optional, Linux) |
