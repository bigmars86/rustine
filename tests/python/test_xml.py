"""XML output parses with ElementTree and keeps every character; names and
characters that XML 1.0 cannot represent raise instead of giving broken XML.

Run after building the native module; needs fixtures/parity."""
import os
import pathlib
import tempfile
import xml.etree.ElementTree as ET

from Rustine import util

FIXTURES = pathlib.Path(__file__).resolve().parents[2] / "fixtures" / "parity"

ITEMS = r"""
grammar input:
    match /([^|\n]*)\|([^\n]*)\n/:
        out.create('item?v="$1"', '$2')
"""
PAIRS = r"""
grammar input:
    match /([^|\n]*)\|([^\n]*)\n/:
        out.add('$1', '$2')
"""


def raises(exc, fn, *args, **kwargs):
    try:
        fn(*args, **kwargs)
    except exc as e:
        return e
    raise AssertionError("%s not raised by %s" % (exc.__name__, fn.__name__))


for syntax in sorted(FIXTURES.glob("*/syntax1.gel")):
    source = (syntax.parent / "input1.txt").read_bytes().decode("utf-8")
    ET.fromstring(util.generate_string(util.compile(str(syntax)), source, format="xml"))

# Markup characters, tab, CR, leading and trailing spaces, non-ASCII, DEL and NEL.
values = ["<a> & \"q\" 'x'", "tab\there", "cr\rlf", "  spaced  ", "\u00e9\U0001f642", "\x7f\x85"]
items = util.compile_string(ITEMS, preserve_case=True)
root = ET.fromstring(util.generate_string(items, "".join("%s|%s\n" % (v, v) for v in values), format="xml"))
assert [(item.get("v"), item.text) for item in root.findall("item")] == [(v, v) for v in values]

pairs = util.compile_string(PAIRS, preserve_case=True)
for bad in ("042|x\n", "a:b|x\n", "t|a\x01b\n", "t|\uffff\n"):
    util.generate_string(pairs, bad, format="json")
    raises(RuntimeError, util.generate_string, pairs, bad, format="xml")
with tempfile.TemporaryDirectory() as tmp:
    path = os.path.join(tmp, "out.xml")
    raises(RuntimeError, util.generate_string_to_file, pairs, "t|a\x01b\n", path, format="xml")
    assert not os.path.exists(path), "no partial file"

print("xml: ok")
