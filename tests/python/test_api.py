"""Python API behaviour: errors, threads and output formats.

Run after building the native module."""
import threading
import time

from Rustine import rustine

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

# A conversion releases the GIL: another thread keeps running in the middle of it.
items = rustine.compile_grammar(ITEMS)
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

print("api: ok")
