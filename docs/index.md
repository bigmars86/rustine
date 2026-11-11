# Rustine Documentation

**High-performance Gel syntax parser** — a Rust rewrite of
[Python Gelatin](https://github.com/knipknap/Gelatin), exposed as a native
Python module via [PyO3](https://pyo3.rs) + [maturin](https://maturin.rs).

## Quick Start

```bash
pip install rustine
```

```python
from Rustine.util import compile_string, generate_string

grammar = r"""
define nl /\r?\n/
define ws /\s+/

grammar input:
    match 'Name:' ws /[^\r\n,]+/ /(?:\r?\n|,) */:
        out.open('user')
        out.add_attribute('.', 'name', '$2')
"""

print(generate_string(compile_string(grammar), "Name: Alice\n", format="json"))
```

## Contents

```{toctree}
:maxdepth: 2

install
getting-started
syntax
architecture
performance
migration
changelog
```
