//! Markdown generation from classified blocks.

use crate::layout::Block;

/// Convert ordered blocks into markdown text.
pub fn blocks_to_markdown(blocks: &[&Block]) -> String {
    let mut parts: Vec<String> = Vec::new();

    for block in blocks {
        if block.skip {
            continue;
        }

        let text = block.text();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        if block.heading_level > 0 {
            let prefix = "#".repeat(block.heading_level as usize);
            // Flatten multi-line headings into a single line
            let flat = text.replace('\n', " ");
            parts.push(format!("{} {}", prefix, flat.trim()));
        } else {
            parts.push(text.to_string());
        }
    }

    parts.join("\n\n")
}
