mod extract;
mod grouping;
mod layout;
mod markdown;

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: paper-to-markdown <input.pdf> [-o output.md] [--pages 1,3,5] [--debug]");
        eprintln!();
        eprintln!("Extracts text from a PDF and writes markdown.");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  -o, --output FILE   Output markdown file (default: <input>.md)");
        eprintln!("  --pages 1,3,5       Extract specific pages only");
        eprintln!("  --debug             Also write per-page files to a directory");
        std::process::exit(1);
    }

    let pdf_path = PathBuf::from(&args[1]);
    if !pdf_path.exists() {
        eprintln!("Error: {} not found", pdf_path.display());
        std::process::exit(1);
    }

    let mut output_file: Option<PathBuf> = None;
    let mut page_filter: Option<Vec<u32>> = None;
    let mut debug = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(PathBuf::from(&args[i]));
                }
            }
            "--pages" => {
                i += 1;
                if i < args.len() {
                    page_filter = Some(
                        args[i].split(',')
                            .filter_map(|s| s.trim().parse::<u32>().ok())
                            .collect(),
                    );
                }
            }
            "--debug" => {
                debug = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Default output: same name as input with .md extension
    let out_file = output_file.unwrap_or_else(|| {
        pdf_path.with_extension("md")
    });

    let start = Instant::now();

    // Step 1: Extract glyphs
    let bytes = std::fs::read(&pdf_path).expect("Failed to read PDF");
    let pages = extract::extract_pages(&bytes);
    if pages.is_empty() {
        eprintln!("No pages extracted");
        std::process::exit(1);
    }
    let page_count = pages.len() as u32;

    let page_nums: Vec<u32> = if let Some(ref filter) = page_filter {
        filter.iter().copied().filter(|&p| p >= 1 && p <= page_count).collect()
    } else {
        (1..=page_count).collect()
    };
    if page_nums.is_empty() {
        eprintln!("Error: no valid pages to process");
        std::process::exit(1);
    }

    // Step 2: Group glyphs into blocks per page
    let mut page_blocks: Vec<(u32, Vec<layout::Block>)> = Vec::new();
    for &pn in &page_nums {
        let page = &pages[(pn - 1) as usize];
        let grouped = grouping::group_glyphs(&page.glyphs, page.height);
        let blocks = grouped.into_iter()
            .map(|gb| layout::Block::from_grouped(gb))
            .collect();
        page_blocks.push((pn, blocks));
    }

    // Step 3: Document-level font analysis
    let all_blocks: Vec<&layout::Block> = page_blocks.iter()
        .flat_map(|(_, b)| b.iter()).collect();
    let font_info = layout::analyze_fonts(&all_blocks);

    // Step 4: Detect repeating elements (headers/footers)
    let repeating = layout::detect_repeating_elements(&page_blocks);

    // Step 5: Skip detection
    for (pn, blocks) in &mut page_blocks {
        let pw = pages[(*pn - 1) as usize].width;
        let ph = pages[(*pn - 1) as usize].height;
        layout::mark_skips(blocks, pw, ph, &repeating);
    }

    // Step 6: Column model
    let page_width = pages[(page_nums[0] - 1) as usize].width;
    let page_height = pages[(page_nums[0] - 1) as usize].height;
    let col_model = layout::build_column_model(&page_blocks, page_width, page_height);

    // Step 7: Per-page processing
    let mut all_markdown: Vec<(u32, String)> = Vec::new();

    for (pn, blocks) in &mut page_blocks {
        layout::classify_blocks(blocks, &col_model);
        layout::detect_headings(blocks, &font_info);
        let ordered = layout::compute_reading_order(blocks, &col_model);
        let md = markdown::blocks_to_markdown(&ordered);
        all_markdown.push((*pn, md));
    }

    // Write combined output
    let combined: String = all_markdown.iter()
        .map(|(_, md)| md.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    std::fs::write(&out_file, &combined).expect("Failed to write output markdown");

    // Debug: also write per-page files
    if debug {
        let debug_dir = out_file.with_extension("").to_string_lossy().to_string() + "_pages";
        let debug_dir = PathBuf::from(debug_dir);
        std::fs::create_dir_all(&debug_dir).expect("Failed to create debug directory");
        for (pn, md) in &all_markdown {
            let page_file = debug_dir.join(format!("page{}.md", pn));
            std::fs::write(&page_file, md).expect("Failed to write page markdown");
        }
        eprintln!("Debug: per-page files written to {}/", debug_dir.display());
    }

    let elapsed = start.elapsed();
    let n = page_nums.len();
    eprintln!(
        "{}: {} pages -> {} ({:.0}ms/page, {:.2}s total)",
        pdf_path.file_name().unwrap().to_string_lossy(),
        n,
        out_file.display(),
        elapsed.as_secs_f64() / n as f64 * 1000.0,
        elapsed.as_secs_f64(),
    );
}
