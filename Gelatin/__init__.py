# Backward-compatibility shim — redirects to the Rustine package.
#
# Allows existing code using `from Gelatin.util import compile` to keep working.
# New code should use `from Rustine.util import compile` instead.

try:
    from Rustine import rustine as _native
except ImportError:
    # Fallback: try the native module directly
    import rustine as _native

__version__ = _native.__version__

INDENT_WIDTH = 4
INDENT = ' ' * INDENT_WIDTH
SEARCH_WINDOW = 1000
