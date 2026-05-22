#!/usr/bin/env python3
# /// script
# dependencies = ["openai"]
# requires-python = ">=3.11"
# ///
"""
Optional LLM cleanup for extracted PDF markdown.

Reads a markdown file (output from paper-to-markdown), splits it by page
separators (---), sends each page to an LLM to remove noise fragments and
fix artifacts, then writes a cleaned markdown file.

Usage:
    uv run scripts/clean_for_rag.py paper.md -o paper_clean.md
    uv run scripts/clean_for_rag.py paper.md --azure
    uv run scripts/clean_for_rag.py paper.md --model gpt-5-mini

Environment variables (set in environment or .env in current directory):
    OpenAI:       OPENAI_API_KEY
    Azure OpenAI: AZURE_OPENAI_API_KEY, AZURE_OPENAI_ENDPOINT (use --azure)
"""

import argparse
import os
import sys
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

from openai import AzureOpenAI, OpenAI

DEFAULT_MODEL = "gpt-5.4-mini"

SYSTEM_PROMPT = """\
You are a text cleanup assistant. You receive raw text extracted from an \
academic paper PDF. The extraction is mostly good but has some artifacts:

1. SHORT NOISE FRAGMENTS: isolated words or partial words from figure \
labels, table annotations, or chart elements. Remove these entirely.

2. HEADERS/FOOTERS: conference names, page numbers, author running \
headers, "Manuscript submitted to ACM", etc. Remove these.

3. BROKEN SENTENCES: occasionally a sentence is split across paragraphs \
due to column layout. If you can see a clear continuation, merge them.

4. HEADING MARKERS: lines starting with # are section headings. Keep \
these but fix any that are clearly wrong.

Rules:
- Output ONLY the cleaned text. No commentary.
- Preserve the original wording exactly — do NOT rephrase, summarize, \
or add content.
- Keep all # heading markers that are genuine section headings.
- Keep paragraph breaks (blank lines between paragraphs).
- Remove noise but do NOT remove real content like figure captions, \
table titles, or equations."""


def load_dotenv():
    """Load .env from current directory if it exists."""
    dotenv = Path.cwd() / ".env"
    if dotenv.exists():
        for line in dotenv.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith('#') and '=' in line:
                key, val = line.split('=', 1)
                os.environ.setdefault(key.strip(), val.strip())


def make_client(use_azure):
    """Create an OpenAI or AzureOpenAI client, or exit with helpful error."""
    load_dotenv()

    if use_azure:
        key = os.environ.get("AZURE_OPENAI_API_KEY")
        endpoint = os.environ.get("AZURE_OPENAI_ENDPOINT")
        missing = []
        if not key:
            missing.append("AZURE_OPENAI_API_KEY")
        if not endpoint:
            missing.append("AZURE_OPENAI_ENDPOINT")
        if missing:
            print("Error: Azure OpenAI requires the following environment variables:", file=sys.stderr)
            for m in missing:
                print(f"  {m}", file=sys.stderr)
            print("\nSet them in your environment or in a .env file in the current directory.", file=sys.stderr)
            print("\nExample .env:", file=sys.stderr)
            print("  AZURE_OPENAI_API_KEY=your-key-here", file=sys.stderr)
            print("  AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com/", file=sys.stderr)
            sys.exit(1)
        return AzureOpenAI(api_key=key, azure_endpoint=endpoint, api_version="2024-06-01")
    else:
        key = os.environ.get("OPENAI_API_KEY")
        if not key:
            print("Error: OpenAI requires the OPENAI_API_KEY environment variable.", file=sys.stderr)
            print("\nSet it in your environment or in a .env file in the current directory.", file=sys.stderr)
            print("\nExample .env:", file=sys.stderr)
            print("  OPENAI_API_KEY=sk-your-key-here", file=sys.stderr)
            print("\nFor Azure OpenAI, use the --azure flag instead.", file=sys.stderr)
            sys.exit(1)
        return OpenAI(api_key=key)


def clean_chunk(client, model, text, chunk_num):
    """Send one chunk to the LLM for cleanup."""
    if not text.strip():
        return ""
    try:
        resp = client.chat.completions.create(
            model=model,
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": f"Clean up this extracted text (section {chunk_num}):\n\n{text}"},
            ],
        )
        return resp.choices[0].message.content.strip()
    except Exception as e:
        print(f"  Warning: chunk {chunk_num} failed: {e}", file=sys.stderr)
        return text


def main():
    parser = argparse.ArgumentParser(
        description="Optional LLM cleanup for extracted PDF markdown",
        epilog="""Environment variables:
  OpenAI:       OPENAI_API_KEY
  Azure OpenAI: AZURE_OPENAI_API_KEY, AZURE_OPENAI_ENDPOINT

Variables are loaded from the environment and from .env in the current directory.""",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("input", type=Path, help="Input markdown file (from paper-to-markdown)")
    parser.add_argument("-o", "--output", type=Path, default=None,
                        help="Output file (default: <input>_clean.md)")
    parser.add_argument("-j", "--workers", type=int, default=5, help="Parallel workers (default: 5)")
    parser.add_argument("--model", type=str, default=DEFAULT_MODEL,
                        help=f"Model name (default: {DEFAULT_MODEL})")
    parser.add_argument("--azure", action="store_true", help="Use Azure OpenAI instead of OpenAI")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"Error: {args.input} not found", file=sys.stderr)
        sys.exit(1)

    out_file = args.output or args.input.with_stem(args.input.stem + "_clean")

    # Split input by page separators
    text = args.input.read_text(encoding="utf-8")
    chunks = [c.strip() for c in text.split("\n---\n") if c.strip()]
    if not chunks:
        chunks = [text]

    client = make_client(args.azure)
    t_start = time.time()

    print(f"Cleaning {len(chunks)} sections with {args.model} ({args.workers} workers)...",
          file=sys.stderr)

    results = {}
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = {}
        for i, chunk in enumerate(chunks):
            future = pool.submit(clean_chunk, client, args.model, chunk, i + 1)
            futures[future] = i

        for future in as_completed(futures):
            idx = futures[future]
            results[idx] = future.result()

    # Reassemble in order
    cleaned = "\n\n---\n\n".join(results[i] for i in sorted(results.keys()) if results[i].strip())
    out_file.write_text(cleaned, encoding="utf-8")

    elapsed = time.time() - t_start
    print(f"Done: {args.input} -> {out_file} ({elapsed:.1f}s)", file=sys.stderr)


if __name__ == "__main__":
    main()
