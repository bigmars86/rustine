"""YAML output loads (PyYAML) as the same data as the JSON output.

Run after building the native module; needs PyYAML and fixtures/parity."""
import json
import os
import pathlib
import tempfile

import yaml

from Rustine import util

FIXTURES = pathlib.Path(__file__).resolve().parents[2] / "fixtures" / "parity"

EDGE = r"""
grammar input:
    match /e\n/:
        out.create('list/empty')
    match /n (\S+) (\S+)\n/:
        out.open('nested?id="$1"')
        out.create('item', '$1')
        out.create('item', '$2')
    match /a ([^\t\n]*)\t([^\t\n]*)\t([^\n]*)\n/:
        out.add('attrs?$1="$2"', '$3')
    match /([^\t\n]*)\t([^\n]*)\n/:
        out.add('$1', '$2')
"""

# (node name, text): YAML keywords, numbers, indicators, quotes, whitespace,
# characters that need escapes, non-ASCII, empty text and keys long enough
# to be written as explicit `? key` entries.
NODES = [
    ("true", "yes"), ("False", "No"), ("null", "~"), ("y", "n"), ("on", "off"),
    ("042", "0x1F"), ("1.5", ".inf"), ("2024-01-01", "1:20"), ("-dash", "- item"),
    ("colon: here", "a: b"), ("#hash", "x #c"), ("<<", "="), ("quote's", "it's \"q\""),
    ("'lead", "  spaced  "), ("tab", "a\tb"), ("cr", "a\rb"), ("ctl\x01", "v\x7f\x1b"),
    ("nel\x85", "c1\x9f"), ("ls\u2028", "ps\u2029"), ("bom\ufeff", "\ufeffx"),
    ("nonchar", "\uffff\ufffe"), ("emoji \U0001f642", "\u00e9 \u00fc"), ("k" * 300, "long key"),
    ("v", ""), ("back\\slash", "\\n is no newline"), ("%", "@at"), ("[x]", "{y}"),
    ("*", "&"), ("!", "|"), (">", "?"), ("`", ","), ("...", "---"), ("a/b", "slash"),
]
ATTRS = [("x'y", "it's", "t"), ("true", "1", "2"), ("q" * 300, "long", "attribute"),
         ("c\x01", "v\x02", "w\u2028")]


def check(conv, text):
    data = json.loads(util.generate_string(conv, text, format="json"))
    out = util.generate_string(conv, text, format="yaml")
    assert yaml.safe_load(out) == data, out
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "out.yaml")
        util.generate_string_to_file(conv, text, path, format="yaml")
        with open(path, encoding="utf-8", newline="") as f:
            assert f.read() == out
    return out


for syntax in sorted(FIXTURES.glob("*/syntax1.gel")):
    source = (syntax.parent / "input1.txt").read_bytes().decode("utf-8")
    check(util.compile(str(syntax)), source)

edge = util.compile_string(EDGE, preserve_case=True)
text = "e\ne\nn a b\nn c d\n"
text += "".join("a %s\t%s\t%s\n" % attr for attr in ATTRS)
text += "".join("%s\t%s\n" % node for node in NODES)
check(edge, text)

assert check(util.compile_string("grammar input:\n    skip /.*/\n"), "x") == "{}\n"

print("yaml: ok")
