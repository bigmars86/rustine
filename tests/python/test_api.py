"""Python API behaviour: errors, threads and output formats.

Run after building the native module."""
from Rustine import rustine

INVALID_REGEX = "grammar input:\n    match /(/:\n        out.add('x')\n"


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

print("api: ok")
