"""Compile Gel grammars and generate JSON, XML or YAML.

Files are always UTF-8; any other encoding raises ``ValueError``.
"""

import codecs

from . import rustine as _native


def _require_utf8(encoding):
    if codecs.lookup(encoding).name != 'utf-8':
        raise ValueError("only UTF-8 is supported, not '%s'" % encoding)


def compile_string(syntax, *, preserve_case=False):
    """Compile a grammar from a source string; returns a ``GelContext``.

    ``preserve_case=True`` keeps the case of tag names and attribute keys
    in ``out.*`` paths instead of lowercasing them.
    """
    return _native.compile_string(syntax, preserve_case=preserve_case)


def compile(syntax_file, encoding='utf-8', *, preserve_case=False):
    """Compile a grammar from a UTF-8 ``.gel`` file.

    *preserve_case* works as in :func:`compile_string`.
    """
    return _native.compile_file(syntax_file, encoding=encoding,
                                preserve_case=preserve_case)


def generate(converter, input_file, format='xml', encoding='utf-8'):
    """Run *converter* on *input_file* (UTF-8) and return the output.

    *format* is ``'json'``, ``'xml'``, ``'yaml'`` or ``'none'``.
    """
    return converter.generate(input_file, format=format, encoding=encoding)


def generate_to_file(converter, input_file, output_file,
                     format='xml', in_encoding='utf-8', out_encoding='utf-8'):
    """Run *converter* on *input_file* and write the output to *output_file* (UTF-8)."""
    _require_utf8(in_encoding)
    _require_utf8(out_encoding)
    converter.generate_to_file(input_file, output_file, format=format)


def generate_string(converter, input, format='xml'):
    """Run *converter* on the string *input* and return the output."""
    return converter.generate_string(input, format=format)


def generate_string_to_file(converter, input, output_file,
                            format='xml', out_encoding='utf-8'):
    """Run *converter* on the string *input* and write the output to *output_file* (UTF-8)."""
    _require_utf8(out_encoding)
    converter.generate_string_to_file(input, output_file, format=format)
