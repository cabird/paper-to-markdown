#!/usr/bin/env python3
"""
Run the full benchmark: extract all test papers and compare against oracle.

Usage:
    python scripts/run_benchmark.py [--binary ./target/release/paper-to-markdown]
"""

import argparse
import json
import subprocess
import sys
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


def benchmark_one(oracle_file, md_file):
    """Compare one extraction against its oracle. Returns (blocks_found, blocks_total, chars_found, chars_total)."""
    oracle_pages = json.loads(oracle_file.read_text())
    rust_text = normalize(md_file.read_text())

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

    return found, total, chars_found, chars_total


def main():
    parser = argparse.ArgumentParser(description="Run paper-to-markdown benchmark")
    parser.add_argument("--binary", type=str, default=None,
                        help="Path to paper-to-markdown binary (default: auto-detect)")
    args = parser.parse_args()

    # Find binary
    if args.binary:
        binary = Path(args.binary)
    else:
        for candidate in [
            Path("target/release/paper-to-markdown"),
            Path("target/debug/paper-to-markdown"),
        ]:
            if candidate.exists():
                binary = candidate
                break
        else:
            print("Error: paper-to-markdown binary not found. Run 'cargo build --release' first.")
            sys.exit(1)

    test_dir = Path("test_papers")
    oracle_dir = test_dir / "oracle"

    # Check for test papers
    pdfs = sorted(test_dir.glob("*.pdf"))
    if not pdfs:
        print("No test papers found. Run 'python test_papers/download.py' first.")
        sys.exit(1)

    oracles = sorted(oracle_dir.glob("*.json"))
    if not oracles:
        print("No oracle files found in test_papers/oracle/")
        sys.exit(1)

    # Step 1: Extract all PDFs
    print(f"Extracting {len(pdfs)} papers...")
    success = 0
    failed = []
    for pdf in pdfs:
        md_file = pdf.with_suffix(".md")
        result = subprocess.run(
            [str(binary), str(pdf), "-o", str(md_file)],
            capture_output=True, text=True
        )
        if md_file.exists() and md_file.stat().st_size > 0:
            success += 1
        else:
            failed.append(pdf.name)

    print(f"  Extracted: {success}/{len(pdfs)}")
    if failed:
        print(f"  Failed: {', '.join(failed)}")

    # Step 2: Benchmark against oracles
    print(f"\nBenchmarking against {len(oracles)} oracle files...")
    print(f"{'Paper':<35s} {'Blocks':>12s} {'Chars':>12s}")
    print("-" * 62)

    total_found = total_blocks = total_chars_found = total_chars = 0

    for oracle_file in oracles:
        name = oracle_file.stem
        md_file = test_dir / f"{name}.md"
        if not md_file.exists():
            print(f"  {name:<35s} {'(no extraction)':>12s}")
            continue

        found, blocks, chars_found, chars = benchmark_one(oracle_file, md_file)
        bpct = found / blocks * 100 if blocks else 0
        cpct = chars_found / chars * 100 if chars else 0
        print(f"  {name:<35s} {found:>4d}/{blocks:<4d} {bpct:>3.0f}%  {chars_found:>6d}/{chars:<6d} {cpct:>3.0f}%")

        total_found += found
        total_blocks += blocks
        total_chars_found += chars_found
        total_chars += chars

    print("-" * 62)
    bpct = total_found / total_blocks * 100 if total_blocks else 0
    cpct = total_chars_found / total_chars * 100 if total_chars else 0
    print(f"  {'TOTAL':<35s} {total_found:>4d}/{total_blocks:<4d} {bpct:>3.0f}%  {total_chars_found:>6d}/{total_chars:<6d} {cpct:>3.0f}%")

    # Cleanup generated .md files
    for pdf in pdfs:
        md_file = pdf.with_suffix(".md")
        if md_file.exists():
            md_file.unlink()

    print(f"\nOverall: {cpct:.0f}% character match against reference oracle")


if __name__ == "__main__":
    main()
