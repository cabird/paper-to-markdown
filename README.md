# paper-to-markdown

LLM-free PDF to markdown extraction for RAG pipelines, aimed at academic papers
including multi-column layouts. Optionally, a cheap LLM post-processing step can
clean up residual noise.

Pure Rust, MIT licensed, no C dependencies.

## Installation

```bash
cargo build --release
```

The binary is at `target/release/paper-to-markdown`.

## Usage

```
paper-to-markdown <input.pdf> [-o output.md] [--pages 1,3,5] [--debug]
```

### Options

| Option | Description |
|---|---|
| `<input.pdf>` | Path to the PDF file (required) |
| `-o`, `--output FILE` | Output markdown file. Default: `<input>.md` (same name, `.md` extension) |
| `--pages 1,3,5` | Extract only specific pages (comma-separated, 1-indexed) |
| `--debug` | Also write individual per-page `.md` files to a `<output>_pages/` directory |

### Examples

```bash
# Basic: produces paper.md alongside paper.pdf
paper-to-markdown paper.pdf

# Specify output path
paper-to-markdown paper.pdf -o /tmp/extracted.md

# Extract only pages 1-5
paper-to-markdown paper.pdf --pages 1,2,3,4,5 -o intro.md

# Debug mode: also writes paper_pages/page1.md, page2.md, etc.
paper-to-markdown paper.pdf -o paper.md --debug
```

### Output Format

The output is a single markdown file with:
- `#` / `##` / `###` headings detected from font sizes and section patterns
- Paragraphs separated by blank lines
- Pages separated by `---` (horizontal rules)

## Optional LLM Cleanup

The extraction is LLM-free and works well for most papers. For papers with heavy
figure annotations or complex table layouts, an optional LLM cleanup step can
remove residual noise:

```bash
# Using OpenAI
uv run scripts/clean_for_rag.py paper.md -o paper_clean.md

# Using Azure OpenAI
uv run scripts/clean_for_rag.py paper.md --azure -o paper_clean.md

# Different model
uv run scripts/clean_for_rag.py paper.md --model gpt-5-mini
```

### Cleanup Script Options

```
scripts/clean_for_rag.py <input.md> [-o output.md] [--azure] [--model NAME] [-j WORKERS]
```

| Option | Description |
|---|---|
| `<input.md>` | Input markdown file from paper-to-markdown (required) |
| `-o`, `--output FILE` | Output file. Default: `<input>_clean.md` |
| `--azure` | Use Azure OpenAI instead of OpenAI |
| `--model NAME` | Model deployment name. Default: `gpt-5.4-mini` |
| `-j`, `--workers N` | Parallel workers. Default: 10 |

### API Key Setup

Set credentials in your environment or in a `.env` file in the current directory:

**OpenAI:**
```
OPENAI_API_KEY=sk-your-key-here
```

**Azure OpenAI** (requires `--azure` flag):
```
AZURE_OPENAI_API_KEY=your-key-here
AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com/
```

### Cost

| Model | Cost/paper | Speed |
|---|---|---|
| gpt-5-mini | ~$0.05 | ~5s/page |
| gpt-5.4-mini | ~$0.11 | ~1s/page |

Both models produce near-identical output and preserve original wording faithfully.

## Performance

Tested on 38 academic papers across three sets:
- 20 arxiv papers (single and multi-column, various fields)
- 9 ICSE 2026 papers (software engineering, 2-column ACM format)
- 9 CHI 2026 papers (HCI, mix of 1-column and 2-column ACM format)

### Extraction Quality

| Metric | Result |
|---|---|
| Character match vs reference oracle | **93%** (20-paper benchmark) |
| Papers rated "good for RAG" (no cleanup needed) | **82%** (31/38) |
| Papers rated "usable" | **92%** (35/38) |
| Papers with garbled/interleaved text | **0%** |
| Extraction speed | 15-100ms per page |

### Where It Struggles

The remaining ~18% of papers that need LLM cleanup typically have:

- **Heavy figure/chart annotations**: PDFs where figure labels, axis values, and
  diagram text are placed as individual text objects overlapping body text. The
  extraction correctly separates them (no garbling), but short noise fragments
  like isolated words or numbers appear between paragraphs.

- **Complex tables**: Table cell text extracted as flat text without structure.
  The content is present but not formatted as a table.

- **Inline code or diffs**: Papers with large code blocks or diff displays
  produce noisy output because the code layout doesn't follow paragraph conventions.

- **Dense mathematical notation**: Equations and mathematical symbols are extracted
  as text but may be fragmented or hard to interpret.

In all these cases, the body text paragraphs remain intact and readable — the
noise appears *between* paragraphs, not within them. An LLM chunker for RAG
will typically create clean chunks from the body paragraphs and ignore the noise.
The optional LLM cleanup step (`scripts/clean_for_rag.py`) can remove this noise
for ~$0.05-0.11 per paper.

## How It Works

1. **Character extraction** — Walks the PDF content stream via `pdf-extract`,
   computing per-glyph positions with precise text matrix math
2. **Text object tracking** — Pre-scans content streams with `lopdf` to track
   BT/ET boundaries, preventing character interleaving from overlapping text layers
3. **Text grouping** — Groups glyphs into lines and blocks using well-tuned
   thresholds for baseline proximity, spacing, and paragraph breaks
4. **Column detection** — Document-level x-occupancy profiling across all pages
   to detect multi-column layouts
5. **Reading order** — Column-aware ordering with preamble/body separation
6. **Header/footer removal** — Cross-page repetition analysis to identify and
   skip repeated headers, footers, and page numbers
7. **Heading detection** — Font size tiers, bold/all-caps signals, numbered
   section patterns, and known heading names

## Testing

```bash
# Extract test papers
for pdf in test_papers/*.pdf; do
  ./target/release/paper-to-markdown "$pdf"
done

# Compare against reference oracle
for oracle in test_papers/oracle/*.json; do
  name=$(basename "$oracle" .json)
  echo -n "$name: "
  python3 scripts/benchmark_blocks.py "$oracle" "test_papers/${name}.md"
done
```

## Project Structure

```
src/
  main.rs         — CLI entry point and pipeline orchestration
  extract.rs      — PDF character extraction with precise glyph positioning
  grouping.rs     — Character → line → block grouping
  layout.rs       — Column detection, skip detection, heading detection, reading order
  markdown.rs     — Block-to-markdown output

scripts/
  clean_for_rag.py      — Optional LLM cleanup (OpenAI / Azure OpenAI)
  benchmark_blocks.py   — Reference oracle comparison tool

test_papers/            — Test PDFs and reference oracle JSON files
```

## Dependencies

All pure Rust, MIT/Apache-2.0 licensed:
- [pdf-extract](https://crates.io/crates/pdf-extract) (Apache-2.0) — PDF content stream walking and font handling
- [lopdf](https://crates.io/crates/lopdf) (MIT) — PDF object and content stream parsing

## License

MIT
