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
  lib.rs            Public API re-exports, error types, PyO3 module
  bridge.rs         Python bindings (feature `python`)
  main.rs           `rgel` CLI (feature `cli`)
  parser/
    lexer.rs        Tokenizer (nom)
    syntax.rs       Recursive descent parser (tokens → AST)
    ast.rs          AST types, regex compilation
    validate.rs     Semantic checks
    json.rs         AST as JSON (`parse_to_json`)
    mod.rs          `Parser` facade
  exec/
    mod.rs          Execution engine, triggers, output serializers
    out.rs          Output tree
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
2. Start at the `input` grammar. Until the input is consumed:

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

- **Node arena** — output nodes live in one `Vec<Node>` addressed by `u32`
  ids; after execution it is flattened into a BFS-ordered `FlatTree` for the
  serializers and freed in one piece.
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
| XML    | Custom writer | Rejects names and characters XML 1.0 cannot represent |
| YAML   | Custom block-style writer | Same structure as JSON; quotes only where needed |

All three stream directly to any `std::io::Write` (files, stdout) or build a
`String`.

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `pyo3` | Python bindings (feature-gated) |
| `regex` | Pattern matching engine |
| `serde_json` | JSON serialization |
| `smallvec` | Stack-allocated small vectors |
| `criterion` | Benchmarking framework (dev) |
| `tikv-jemallocator` | jemalloc allocator (optional, Linux wheels) |
| `mimalloc` | mimalloc allocator (optional, Windows wheels) |
