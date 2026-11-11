"""out.* paths are lowercased by default; preserve_case=True keeps their case.

Run after building the native module."""
from Gelatin.util import compile_string as gelatin_compile_string
from Rustine import rustine
from Rustine.util import compile_string, generate_string

GRAMMAR = "grammar input:\n    match /(\\w+)\\n/:\n        out.add('MixedCase', '$1')\n"
LOWER = "<mixedcase>value</mixedcase>"
PRESERVED = "<MixedCase>value</MixedCase>"


def check(output, expected):
    assert expected in output, output


check(generate_string(compile_string(GRAMMAR), "value\n", format="xml"), LOWER)
check(generate_string(gelatin_compile_string(GRAMMAR), "value\n", format="xml"), LOWER)
check(generate_string(compile_string(GRAMMAR, preserve_case=True), "value\n", format="xml"), PRESERVED)
check(rustine.run_grammar_xml(GRAMMAR, "input", "value\n"), LOWER)
check(rustine.run_grammar_xml(GRAMMAR, "input", "value\n", preserve_case=True), PRESERVED)
print("preserve_case: ok")
