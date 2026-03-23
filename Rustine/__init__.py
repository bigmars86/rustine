# Rustine — high-performance Gel syntax parser.
#
# This package provides `from Rustine.util import compile` etc., delegating
# to the native Rust `rustine` extension module built with PyO3.

from . import rustine as _native

__version__ = _native.__version__

INDENT_WIDTH = 4
INDENT = ' ' * INDENT_WIDTH
SEARCH_WINDOW = 1000
