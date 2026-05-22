#!/usr/bin/env python3
"""Compare extraction output against reference oracle."""
import json, sys
from pathlib import Path

def normalize(text):
    return ' '.join(text.split()).strip()

def block_text(block):
    text = ""
    for line in block["lines"]:
        for span in line["spans"]:
            text += span["text"]
        text += "\n"
    return normalize(text)

def main():
    oracle_file = Path(sys.argv[1])
    rust_output = Path(sys.argv[2])

    oracle_pages = json.loads(oracle_file.read_text())

    # Support both single file and directory
    if rust_output.is_file():
        rust_text = normalize(rust_output.read_text())
    elif rust_output.is_dir():
        parts = []
        for f in sorted(rust_output.glob("page*.md")):
            parts.append(f.read_text())
        rust_text = normalize(" ".join(parts))
    else:
        print(f"Error: {rust_output} not found")
        sys.exit(1)

    total = found = chars_total = chars_found = 0

    for page in oracle_pages:
        for block in page["blocks"]:
            bt = block_text(block)
            if len(bt) < 10:
                continue
            total += 1
            chars_total += len(bt)
            if bt[:30] in rust_text:
                found += 1
                chars_found += len(bt)

    bpct = found / total * 100 if total else 0
    cpct = chars_found / chars_total * 100 if chars_total else 0
    print(f"  Blocks: {found}/{total} ({bpct:.0f}%)  Chars: {chars_found}/{chars_total} ({cpct:.0f}%)")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: benchmark_blocks.py <oracle.json> <output.md or output_dir/>")
        sys.exit(1)
    main()
