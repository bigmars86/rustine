"""Python API behaviour: errors, threads and output formats.

Run after building the native module."""
import os
import tempfile
import threading
import time

from Rustine import rustine, util

INVALID_REGEX = "grammar input:\n    match /(/:\n        out.add('x')\n"
ITEMS = "grammar input:\n    match /(\\w+) (\\d+)\\n/:\n        out.create('item?id=\"$2\"', '$1')\n"


def raises(exc, fn, *args, **kwargs):
    try:
        fn(*args, **kwargs)
    except exc as e:
        return e
    raise AssertionError("%s not raised by %s" % (exc.__name__, fn.__name__))


# Every entry point validates the grammar.
raises(ValueError, rustine.compile_grammar, INVALID_REGEX)
for run in (rustine.run_grammar, rustine.run_grammar_xml, rustine.run_grammar_yaml):
    raises(ValueError, run, INVALID_REGEX, "input", "(abc")

# parse_to_json returns the grammar's AST; a grammar that does not parse raises.
assert '"defines"' in rustine.parse_to_json("define ws /\\s+/\n")
raises(ValueError, rustine.parse_to_json, "define x 'unterminated\n")

# Input the grammar does not consume is an error, not a truncated result.
items = rustine.compile_grammar(ITEMS)
err = raises(RuntimeError, items.generate_string, "a 1\nnot an item\n", format="json")
assert "stopped at line 2" in str(err), err

# Grammar calls nest up to 250 levels; deeper nesting raises instead of crashing.
nested = rustine.compile_grammar("grammar input:\n    match /\\{/:\n        input()\n    match /\\}/:\n        do.return()\n")
nested.generate_string("{" * 250 + "}" * 250, format="json")
err = raises(RuntimeError, nested.generate_string, "{" * 5000 + "}" * 5000, format="json")
assert "nested more than 250 levels" in str(err), err

# One regex match spans at most search_window bytes (default 1000).
line = "grammar input:\n    match /[a-z]+\\n/:\n        out.add('line')\n"
long_line = "a" * 1500 + "\n"
err = raises(RuntimeError, util.compile_string(line).generate_string, long_line, format="json")
assert "stopped at line 1" in str(err), err
assert '"line"' in util.compile_string(line, search_window=2000).generate_string(long_line, format="json")
assert '"line"' in rustine.run_grammar(line, "input", long_line, search_window=2000)
for bad in (0, -1):
    raises(ValueError, rustine.compile_grammar, line, search_window=bad)

# A conversion releases the GIL: another thread keeps running in the middle of it.
big = "".join("word%d %d\n" % (i % 97, i) for i in range(400000))
stamps = []
stop = threading.Event()


def spin():
    n = 0
    while not stop.is_set():
        n += 1
        if n % 10000 == 0:
            stamps.append(time.perf_counter())


thread = threading.Thread(target=spin)
thread.start()
start = time.perf_counter()
items.generate_string(big, format="json")
end = time.perf_counter()
stop.set()
thread.join()
quarter = (end - start) / 4
assert any(start + quarter < s < end - quarter for s in stamps), "other threads stalled during the conversion"

# Files are UTF-8; other encodings raise instead of being ignored.
with tempfile.TemporaryDirectory() as tmp:
    gel, data, out = (os.path.join(tmp, name) for name in ("items.gel", "items.txt", "out.json"))
    for path, text in ((gel, ITEMS), (data, "a 1\n")):
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(text)
    conv = util.compile(gel, encoding="UTF8")
    util.generate_to_file(conv, data, out, format="json", in_encoding="utf_8", out_encoding="U8")
    raises(ValueError, util.compile, gel, encoding="latin-1")
    raises(ValueError, util.generate, conv, data, encoding="cp1252")
    raises(ValueError, util.generate_to_file, conv, data, out, in_encoding="latin-1")
    raises(ValueError, util.generate_to_file, conv, data, out, out_encoding="latin-1")
    raises(ValueError, util.generate_string_to_file, conv, "a 1\n", out, out_encoding="utf-16")

print("api: ok")
