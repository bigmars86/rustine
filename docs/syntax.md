# Gel Syntax Reference

This document covers the complete Gel grammar language as implemented by
Rustine. The syntax is fully compatible with the original Python Gelatin.

---

## Definitions

```gelatin
define ws   /\s+/
define nl   /\r?\n/
define num  /[0-9]+/
define word /\w+/
```

Definitions create reusable named patterns that can be referenced in match
statements. They are expanded by name during parsing.

---

## Grammars

```gelatin
grammar input:
    match /pattern/ { actions }
```

All statements live inside named grammars. The grammar named `input` is the
entry point. Grammars are matched top-to-bottom; when a match succeeds, its
action block executes, then the grammar restarts from the top.

If the end of a grammar is reached before the input is consumed, an error is
raised.

### Names

Grammar names (and other identifiers, such as `define` names and grammar-call
targets) are `word` tokens drawn from the character set `[A-Za-z0-9_]+`. A name
**may start with a digit**, so `7600_modules` is a valid grammar name:

```gelatin
grammar 7600_modules(default):
    match /x/:
        out.add('x')
```

A token consisting **only** of digits (e.g. `42`) is instead a numeric literal,
not an identifier.

### Grammar Inheritance

```gelatin
grammar base:
    skip /^#/
    match /common/ { out.create("base") }

grammar child(base):
    match /extra/ { out.add("extra") }
```

A child grammar inherits all statements from its parent. Multi-level chains
(`A → B → C`) are resolved recursively with cycle detection. The child's own
statements take precedence.

### Grammar Calls

```gelatin
grammar input:
    match 'section' nl:
        out.enter('section')
        section_parser()
```

A bare function call invokes another grammar. When the called grammar returns
(via `do.return()` or end-of-grammar), execution continues in the caller.

---

## Statements

### match

```gelatin
match /pattern/:
    actions

match 'literal' ws /regex/ nl:
    actions
```

Case-sensitive pattern matching. Multiple patterns in a single match statement
are concatenated. Supports string literals, regex literals, and defined names.

#### Alternation

```gelatin
match 'foo' ws word nl | 'bar' ws word nl:
    out.add('$2')
```

The `|` operator separates alternative patterns. The first matching alternative
wins.

### imatch

```gelatin
imatch /pattern/:
    actions
```

Case-insensitive variant of `match`.

### when

```gelatin
when /pattern/:
    actions
```

Non-consuming test: checks the current line without advancing the input
position. Useful for lookahead decisions.

### skip

```gelatin
skip /pattern/
skip ws
```

Skips matching input without producing output. Commonly used for whitespace
and comments.

---

## Captures

```gelatin
match /(\w+)\s+(\d+)/:
    out.create("item?name=$1&value=$2")
```

### Positional Captures

`$0` is the full match text. `$1`, `$2`, ... are numbered capture groups.

### Named Captures

```gelatin
match /(?P<name>\w+)\s+(?P<age>\d+)/:
    out.add("person?name=$name&age=$age")
```

Named captures from `(?P<name>...)` groups are available as `$name`.

### Interpolation

Capture variables can appear in:
- Path arguments: `out.create("root/$1")`
- Attribute values: `out.add("node?key=$2")`
- Text arguments: `out.add("node", "$1")`
- Trigger paths and values

---

## Output Actions

Output actions build a hierarchical node tree. Nodes can have children, text
content, and ordered attributes.

### Path Syntax

Paths use `/` as separator. Attributes are encoded in URL-style query notation:

```
node                       → simple node
parent/child               → nested path
node?key="value"           → node with attribute
node?a="1"&b="2"          → multiple attributes
```

### out.create(path [, text])

Always creates a new leaf node at the given path. Intermediate segments are
created as needed. If a node with the same name exists, a new sibling is
created regardless.

```gelatin
out.create("root/item?id=$1")
out.create("root/item", "$2")       # with text content
```

### out.add(path [, text])

Creates or reuses the final node if one with the same name and attributes
exists. If text is provided and the node exists, text is appended.

```gelatin
out.add("root/entry", "$1")
```

### out.replace(path [, text])

Removes any existing matching leaf (same name + attributes) and creates a
fresh leaf.

### out.add_attribute(path, name, value)

Ensures the path exists and appends an ordered attribute pair to the final
node.

```gelatin
out.add_attribute('.', 'name', '$1')
out.add_attribute('user', 'role', 'admin')
```

The special path `.` refers to the current node (top of the open/enter stack).

### out.set_root_name(name)

Sets the root element name (default: `xml` for XML output, omitted for JSON).

### out.open(path)

Traverses the path (creating nodes as needed), then pushes the final node onto
the open stack. The leaf segment is always created fresh.

```gelatin
out.open('user')            # creates <user> and enters it
out.add('name', 'Alice')    # adds <name> under <user>
```

### out.enter(path)

Like `out.open`, but reuses an existing leaf node if found (by name and
attributes).

### out.leave()

Pops one level from the open/enter stack, returning to the parent context.

---

## Triggers

Triggers fire automatically at specific lifecycle points. They are registered
via `out.enqueue_*` actions and matched by regex against the last matched text.

### Trigger Kinds

| Action | Fires When |
|--------|-----------|
| `out.enqueue_before(regex, path [, text])` | Before actions of a matching line |
| `out.enqueue_after(regex, path [, text])` | After actions of a matching line |
| `out.enqueue_on_add(regex, path [, text])` | Immediately after an out.add/create/open/enter |
| `out.enqueue_on_leave(regex, path [, text])` | When out.leave() is called |

### Single-Shot vs Persistent

By default, triggers are single-shot — they fire once and are removed.
Add `_persist` to keep them active:

```gelatin
out.enqueue_before_persist(/section.*/, "root/marker")
```

### Capture Interpolation in Triggers

```gelatin
match /(foo\w+)/:
    out.enqueue_before(/foo.*/, "root/before?val=$1")
    out.create("root?start=$1")
```

Positional and named captures are interpolated at registration time.

### out.clear_queue()

Clears all registered triggers (both single-shot and persistent).

---

## Control Flow

### do.return()

Terminates the current grammar and returns to the caller.

### do.next()

Stops the current action block; the grammar evaluation loop continues with
the next statement.

### do.skip()

Halts remaining actions in the current block but continues scanning input.

### do.say(message)

Appends an informational trace entry.

### do.warn(message)

Appends a warning trace entry.

### do.fail([message])

Records an error, appends failure traces, and terminates execution.

---

## Error Model

Rustine uses a structured error system with source spans:

| Error Kind   | When | Span |
|-------------|------|------|
| `Lex`        | Unexpected character, unterminated literal | line/col/offset |
| `Parse`      | Missing colon, unexpected token | line/col |
| `Runtime`    | Grammar not found, regex failure | optional |
| `Validation` | Invalid regex pattern, undefined parent grammar | optional |
| `Io`         | Streaming read failure, UTF-8 decode error | offset |

### Semantic Validation

A post-parse validation pass runs before execution:

- **Regex pre-validation** — invalid patterns produce errors that prevent
  execution.
- **Inheritance checks** — undefined parent grammars are rejected.
- **Undefined references** — unknown grammar calls and undefined variables
  produce warnings.

### Python Exception Mapping

| Error Kind | Python Exception |
|-----------|-----------------|
| Lex / Parse / Validation | `ValueError` |
| Runtime | `RuntimeError` |
| Io | `IOError` |
