# Rustine Documentation

**High-performance Gel syntax parser** — a Rust rewrite of
[Python Gelatin](https://github.com/knipknap/Gelatin), exposed as a native
Python module via [PyO3](https://pyo3.rs) + [maturin](https://maturin.rs).

## Quick Start

```bash
pip install rustine
```

```python
from Rustine import rustine

grammar = r"""
define nl /\r?\n/
define ws /\s+/

grammar input:
    match 'Name:' ws /[^\r\n,]+/ /(?:\r?\n|,) */:
        out.open('user')
        out.add_attribute('.', 'name', '$2')
"""

print(rustine.parse_to_json(grammar, "Name: Alice\n"))
```

## Contents

```{toctree}
:maxdepth: 2

getting-started
syntax
architecture
performance
migration
changelog
```
