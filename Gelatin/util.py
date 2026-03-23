# Backward-compatibility shim — redirects to Rustine.util.
#
# Existing code using `from Gelatin.util import compile` will keep working.
# New code should use `from Rustine.util import compile` instead.

from Rustine.util import (  # noqa: F401
    compile,
    compile_string,
    generate,
    generate_string,
    generate_to_file,
    generate_string_to_file,
)
