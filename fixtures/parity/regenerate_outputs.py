#!/usr/bin/env python3
"""
Regenerate all parity reference output files using Python Gelatin.

Run from the Rustine root:
    python fixtures/parity/regenerate_outputs.py

This ensures the expected output files are authoritative (produced by the
Python reference implementation), not hand-crafted.
"""
import os
import sys

# Ensure Gelatin is importable
try:
    from Gelatin.util import compile_string, generate_string
except ImportError:
    sys.exit("ERROR: Gelatin not installed. Run: pip install -e ../Gelatin")

PARITY_DIR = os.path.dirname(os.path.abspath(__file__))
FORMATS = ["json", "xml"]

def main():
    demos = sorted(
        d for d in os.listdir(PARITY_DIR)
        if os.path.isdir(os.path.join(PARITY_DIR, d))
    )
    for demo in demos:
        demo_dir = os.path.join(PARITY_DIR, demo)
        syntax_file = os.path.join(demo_dir, "syntax1.gel")
        input_file = os.path.join(demo_dir, "input1.txt")
        if not os.path.exists(syntax_file) or not os.path.exists(input_file):
            print(f"  SKIP {demo} (missing syntax1.gel or input1.txt)")
            continue

        with open(syntax_file, encoding="utf-8") as f:
            syntax_src = f.read()
        with open(input_file, encoding="utf-8") as f:
            input_src = f.read()

        converter = compile_string(syntax_src)

        for fmt in FORMATS:
            out_file = os.path.join(demo_dir, f"output1.{fmt}")
            result = generate_string(converter, input_src, format=fmt)
            with open(out_file, "w", encoding="utf-8", newline="\n") as f:
                f.write(result)
            size = len(result)
            print(f"  {demo}/output1.{fmt}  ({size:,} bytes)")

    print("\nDone. All output files regenerated from Python Gelatin.")


if __name__ == "__main__":
    main()
