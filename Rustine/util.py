# Rustine.util — public API mirroring the original Python Gelatin.util module.
#
# Usage:
#   from Rustine.util import compile, compile_string
#   from Rustine.util import generate, generate_string
#   from Rustine.util import generate_to_file, generate_string_to_file

from . import rustine as _native


def compile_string(syntax):
    """Compile a Gel grammar from a source string.

    Returns a context object whose ``generate_string`` / ``generate``
    methods produce structured output (JSON, XML, YAML).
    """
    return _native.compile_string(syntax)


def compile(syntax_file, encoding='utf-8'):
    """Compile a Gel grammar from a ``.gel`` file.

    Parameters
    ----------
    syntax_file : str
        Path to the ``.gel`` grammar file.
    encoding : str, optional
        Character encoding (default ``'utf-8'``).
    """
    return _native.compile_file(syntax_file, encoding=encoding)


def generate(converter, input_file, format='xml', encoding='utf-8'):
    """Read *input_file*, run the grammar, and return the output string.

    Parameters
    ----------
    converter : GelContext
        A compiled grammar (as returned by :func:`compile`).
    input_file : str
        Path to the input text file.
    format : str
        Output format (``'json'``, ``'xml'``, ``'yaml'``, ``'none'``).
    encoding : str
        Input file encoding (default ``'utf-8'``).
    """
    return converter.generate(input_file, format=format, encoding=encoding)


def generate_to_file(converter, input_file, output_file,
                     format='xml', in_encoding='utf-8', out_encoding='utf-8'):
    """Read *input_file*, run the grammar, write result to *output_file*."""
    _ = out_encoding  # Rust writes UTF-8
    converter.generate_to_file(input_file, output_file, format=format)


def generate_string(converter, input, format='xml'):
    """Run the grammar against *input* (a string) and return the result."""
    return converter.generate_string(input, format=format)


def generate_string_to_file(converter, input, output_file,
                            format='xml', out_encoding='utf-8'):
    """Run the grammar against *input* and write result to *output_file*."""
    _ = out_encoding  # Rust writes UTF-8
    converter.generate_string_to_file(input, output_file, format=format)
