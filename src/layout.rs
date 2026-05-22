//! Layout analysis: column detection, reading order, skip detection, headings.

use std::collections::{HashMap, HashSet};

use crate::grouping;

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Vertical tolerance (points) for grouping characters into the same line.
const LINE_Y_TOLERANCE: f64 = 3.0;

/// Vertical gap (points) between lines to start a new block.
const BLOCK_GAP: f64 = 8.0;

/// Minimum x-overlap ratio to merge a line into an existing block.
const BLOCK_X_OVERLAP_MIN: f64 = 0.3;

/// How many blocks to scan backwards when merging lines into blocks.
const BLOCK_MERGE_LOOKBACK: usize = 30;

/// Multiplier: x-gap > median_font_size * this → column break within a row.
const COLUMN_GAP_FONT_MULTIPLIER: f64 = 2.0;

/// Multiplier: char gap > font_size * this → insert a space character.
const SPACE_GAP_FONT_MULTIPLIER: f64 = 0.15;

/// Number of bins in x-occupancy profile for column detection.
const N_BINS: usize = 256;

/// Vertical region of each page used for column model fitting.
const FIT_Y_TOP: f64 = 0.12;
const FIT_Y_BOTTOM: f64 = 0.90;

/// Thresholds for voting on column count (fractions of max occupancy).
const COLUMN_VOTE_THRESHOLDS: [f64; 3] = [0.35, 0.45, 0.55];

/// Minimum gap between components to keep them separate (fraction of N_BINS).
const MIN_COMPONENT_GAP: f64 = 0.02;

/// Minimum component width to be considered a real column (fraction of N_BINS).
const MIN_COMPONENT_WIDTH: f64 = 0.08;

/// Minimum occupancy for the profile to be considered non-empty.
const MIN_OCCUPANCY: f64 = 0.05;

/// Column assignment: block must overlap this fraction of best column.
const COL_OVERLAP_MIN: f64 = 0.75;

/// Column assignment: block must overlap gutters less than this fraction.
const GUTTER_OVERLAP_MAX: f64 = 0.15;

/// Font size ratio above body to qualify as a heading tier.
const HEADING_SIZE_RATIO: f64 = 1.15;

/// Tolerance (points) for matching font size to a heading tier.
const HEADING_SIZE_TOLERANCE: f64 = 0.5;

/// Maximum lines/chars for a block to be considered a heading.
const HEADING_MAX_LINES: usize = 2;
const HEADING_MAX_CHARS: usize = 120;

/// Minimum blocks at one x-position to be classified as margin line numbers.
const LINE_NUMBER_MIN_GROUP: usize = 5;

/// Grid size (points) for bucketing block positions in repetition detection.
const REPEAT_GRID: f64 = 15.0;

/// Fraction of pages a fingerprint must appear on to count as repeating.
const REPEAT_THRESHOLD_FRAC: f64 = 0.4;

/// Page zones (fraction of height) for skip detection.
const SKIP_ZONE_REPEATING: f64 = 0.08;
const SKIP_ZONE_PAGE_NUM: f64 = 0.06;
const SKIP_ZONE_EDGE: f64 = 0.04;
const SKIP_SMALL_FONT_MAX: f64 = 9.0;
const SKIP_MARGIN_FRAC: f64 = 0.08;
const SKIP_LINE_NUM_WIDTH_FRAC: f64 = 0.03;
const SKIP_LINE_NUM_TOLERANCE_FRAC: f64 = 0.01;

/// Tolerance (points) for body_start_y when classifying preamble vs body.
const PREAMBLE_Y_TOLERANCE: f64 = 5.0;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Full,
    /// Column index, 0-based from left to right.
    Col(usize),
}

/// Lightweight character position data for column model building.
pub struct CharData {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub font_size: f64,
    pub is_whitespace: bool,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<TextLine>,
    pub column: Column,
    pub skip: bool,
    pub exclude_from_layout: bool,
    pub heading_level: u8,
}

impl Block {
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn dominant_size(&self) -> f64 {
        let mut size_counts: HashMap<i32, usize> = HashMap::new();
        for line in &self.lines {
            for span in &line.spans {
                let key = (span.size * 10.0).round() as i32;
                *size_counts.entry(key).or_default() += span.text.len();
            }
        }
        size_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, _)| key as f64 / 10.0)
            .unwrap_or(10.0)
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Convert from grouping::TextBlock into layout::Block.
    pub fn from_grouped(gb: grouping::TextBlock) -> Self {
        let lines: Vec<TextLine> = gb
            .lines
            .into_iter()
            .map(|gl| {
                let text: String = gl.spans.iter().map(|s| s.text.as_str()).collect();
                let spans: Vec<SpanInfo> = gl
                    .spans
                    .into_iter()
                    .map(|s| SpanInfo {
                        text: s.text,
                        size: s.size,
                        font: String::new(),
                        flags: 0,
                    })
                    .collect();
                TextLine {
                    x0: gl.bbox[0],
                    y0: gl.bbox[1],
                    x1: gl.bbox[2],
                    y1: gl.bbox[3],
                    text,
                    spans,
                }
            })
            .collect();
        Block {
            x0: gb.bbox[0],
            y0: gb.bbox[1],
            x1: gb.bbox[2],
            y1: gb.bbox[3],
            lines,
            column: Column::Full,
            skip: false,
            exclude_from_layout: false,
            heading_level: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub text: String,
    pub spans: Vec<SpanInfo>,
}

#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub text: String,
    pub size: f64,
    pub font: String,
    pub flags: u32,
}

pub struct FontInfo {
    pub body_size: f64,
    pub tiers: Vec<(f64, u8)>,
}

pub struct ColumnModel {
    pub n_cols: usize,
    pub col_ranges: Vec<(f64, f64)>,
    pub gutters: Vec<(f64, f64)>,
}

// ---------------------------------------------------------------------------
// Font analysis
// ---------------------------------------------------------------------------

pub fn analyze_fonts(blocks: &[&Block]) -> FontInfo {
    let mut size_counts: HashMap<i32, usize> = HashMap::new();

    for block in blocks {
        for line in &block.lines {
            for span in &line.spans {
                if span.text.trim().is_empty() {
                    continue;
                }
                let key = (span.size * 10.0).round() as i32;
                *size_counts.entry(key).or_default() += span.text.len();
            }
        }
    }

    let body_size = size_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(key, _)| *key as f64 / 10.0)
        .unwrap_or(10.0);

    let mut unique_sizes: Vec<f64> = size_counts
        .keys()
        .map(|k| *k as f64 / 10.0)
        .filter(|s| *s > body_size * HEADING_SIZE_RATIO)
        .collect();
    unique_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap());
    unique_sizes.dedup_by(|a, b| (*a - *b).abs() < HEADING_SIZE_TOLERANCE);

    let tiers: Vec<(f64, u8)> = unique_sizes
        .into_iter()
        .take(4)
        .enumerate()
        .map(|(i, size)| (size, (i + 1) as u8))
        .collect();

    FontInfo { body_size, tiers }
}

// ---------------------------------------------------------------------------
// Repeating element detection (headers/footers)
// ---------------------------------------------------------------------------

pub fn detect_repeating_elements(
    page_blocks: &[(u32, Vec<Block>)],
) -> HashSet<String> {
    if page_blocks.len() < 3 {
        return HashSet::new();
    }

    // (fingerprint, pos_bucket) → list of page indices
    let mut sig_pages: HashMap<(String, (i32, i32, i32, i32)), Vec<usize>> = HashMap::new();

    for (page_idx, (_, blocks)) in page_blocks.iter().enumerate() {
        let mut page_sigs = HashSet::new();
        for b in blocks {
            let text = b.text();
            let text = text.trim();
            if text.is_empty() || text.len() > 200 {
                continue;
            }
            let fp = fingerprint(text);
            let bucket = (
                (b.x0 / REPEAT_GRID).round() as i32,
                (b.y0 / REPEAT_GRID).round() as i32,
                (b.x1 / REPEAT_GRID).round() as i32,
                (b.y1 / REPEAT_GRID).round() as i32,
            );
            let sig = (fp, bucket);
            if page_sigs.insert(sig.clone()) {
                sig_pages.entry(sig).or_default().push(page_idx);
            }
        }
    }

    let n = page_blocks.len();
    let threshold = (n as f64 * REPEAT_THRESHOLD_FRAC).max(3.0) as usize;

    let mut repeating = HashSet::new();
    for ((fp, _), pages) in &sig_pages {
        if pages.len() >= threshold {
            repeating.insert(fp.clone());
            continue;
        }
        // Even/odd pattern
        let even_count = pages.iter().filter(|p| *p % 2 == 0).count();
        let odd_count = pages.len() - even_count;
        let n_even = (n + 1) / 2;
        let n_odd = n / 2;
        let even_thresh = 2.max((n_even as f64 * REPEAT_THRESHOLD_FRAC) as usize);
        let odd_thresh = 2.max((n_odd as f64 * REPEAT_THRESHOLD_FRAC) as usize);
        if n_even >= 3 && even_count >= even_thresh {
            repeating.insert(fp.clone());
        } else if n_odd >= 3 && odd_count >= odd_thresh {
            repeating.insert(fp.clone());
        }
    }

    repeating
}

fn fingerprint(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut result = String::new();
    let mut in_digit = false;
    for ch in lower.chars() {
        if ch.is_ascii_digit() {
            if !in_digit {
                result.push('N');
                in_digit = true;
            }
        } else {
            in_digit = false;
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Skip detection
// ---------------------------------------------------------------------------

pub fn mark_skips(
    blocks: &mut [Block],
    page_width: f64,
    page_height: f64,
    repeating: &HashSet<String>,
) {
    let line_num_ids = detect_margin_line_numbers(blocks, page_width);

    let texts: Vec<String> = blocks.iter().map(|b| b.text()).collect();

    for (i, block) in blocks.iter_mut().enumerate() {
        let text = texts[i].trim();

        // === Hard skip: excluded from both output and layout ===

        if text.is_empty() {
            block.skip = true;
            continue;
        }

        // Single very short fragments (< 4 chars)
        if text.len() < 4 {
            block.skip = true;
            continue;
        }

        if line_num_ids.contains(&i) {
            block.skip = true;
            continue;
        }

        let fp = fingerprint(text);
        if repeating.contains(&fp) {
            // Repeating text is confirmed to appear on many pages at the same
            // position. Skip it if it's in the top 15% or bottom 20% of the page.
            // These zones are wider than for other skip rules because the
            // repetition itself is strong evidence of header/footer status.
            if block.y0 < page_height * 0.15 || block.y1 > page_height * 0.80 {
                block.skip = true;
                continue;
            }
        }

        if text.len() < 10 && text.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
            let zone = SKIP_ZONE_PAGE_NUM * page_height;
            if block.y0 < zone || block.y1 > page_height - zone {
                block.skip = true;
                continue;
            }
        }

        let edge_zone = SKIP_ZONE_EDGE * page_height;
        if text.len() < 40
            && (block.y0 < edge_zone || block.y1 > page_height - edge_zone)
            && block.dominant_size() < SKIP_SMALL_FONT_MAX
        {
            block.skip = true;
            continue;
        }

        // === Layout exclusion: kept in output but excluded from column model ===
        // These are blocks that would pollute the x-occupancy profile.

        // Narrow blocks: < 20% of page width are likely figure debris.
        // Body text in a 2-col layout spans ~40% of page width.
        let block_width = block.x1 - block.x0;
        let width_frac = block_width / page_width;
        if width_frac < 0.20 {
            // block.exclude_from_layout = true;
            continue;
        }

        // Multi-line blocks with very short average line length are
        // fragmented figure/table annotations.
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let avg_line_len = if lines.is_empty() {
            0
        } else {
            text.len() / lines.len()
        };
        if lines.len() > 2 && avg_line_len < 15 {
            // block.exclude_from_layout = true;
        }
    }
}

fn detect_margin_line_numbers(blocks: &[Block], page_width: f64) -> HashSet<usize> {
    let margin = SKIP_MARGIN_FRAC * page_width;
    let max_width = SKIP_LINE_NUM_WIDTH_FRAC * page_width;
    let tolerance = SKIP_LINE_NUM_TOLERANCE_FRAC * page_width;

    let mut candidates: Vec<(usize, f64)> = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let text = b.text();
        let text = text.trim();
        if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if b.width() > max_width {
            continue;
        }
        if b.x1 < margin || b.x0 > page_width - margin {
            candidates.push((i, (b.x0 + b.x1) / 2.0));
        }
    }

    if candidates.len() < LINE_NUMBER_MIN_GROUP {
        return HashSet::new();
    }

    // Group by x-position
    let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
    for (idx, x_mid) in &candidates {
        let placed = groups.iter_mut().find(|(key, _)| (x_mid - *key).abs() < tolerance);
        if let Some((_, group)) = placed {
            group.push(*idx);
        } else {
            groups.push((*x_mid, vec![*idx]));
        }
    }

    groups
        .into_iter()
        .filter(|(_, group)| group.len() >= LINE_NUMBER_MIN_GROUP)
        .flat_map(|(_, group)| group)
        .collect()
}

// ---------------------------------------------------------------------------
// Document-level column model
// ---------------------------------------------------------------------------

pub fn build_column_model(
    page_blocks: &[(u32, Vec<Block>)],
    page_width: f64,
    page_height: f64,
) -> ColumnModel {
    let single = ColumnModel {
        n_cols: 1,
        col_ranges: vec![(0.0, page_width)],
        gutters: vec![],
    };

    if page_blocks.is_empty() {
        return single;
    }

    let fit_y0 = FIT_Y_TOP * page_height;
    let fit_y1 = FIT_Y_BOTTOM * page_height;
    let fit_h = fit_y1 - fit_y0;
    if fit_h <= 0.0 {
        return single;
    }

    // Per-page x-occupancy profiles from block bounding boxes
    let page_profiles: Vec<[f64; N_BINS]> = page_blocks
        .iter()
        .map(|(_, blocks)| build_occupancy_profile_from_blocks(blocks, page_width, fit_y0, fit_y1, fit_h))
        .collect();

    if page_profiles.is_empty() {
        return single;
    }

    // Median aggregate across pages
    let doc_profile = median_profile(&page_profiles);

    // Smooth with moving average
    let smoothed = smooth_profile(&doc_profile, 3);

    let max_occ = smoothed.iter().cloned().fold(0.0_f64, f64::max);
    if max_occ < MIN_OCCUPANCY {
        return single;
    }

    // Vote on column count across thresholds
    let count_votes: Vec<usize> = COLUMN_VOTE_THRESHOLDS
        .iter()
        .map(|frac| find_components(&smoothed, frac * max_occ).len())
        .collect();

    let n_cols = mode(&count_votes).max(1);
    if n_cols == 1 {
        return single;
    }

    // Get final column ranges at middle threshold
    let components = find_components(&smoothed, COLUMN_VOTE_THRESHOLDS[1] * max_occ);

    let col_ranges: Vec<(f64, f64)> = components
        .iter()
        .map(|(s, e)| (bin_to_x(*s, page_width), bin_to_x(*e, page_width)))
        .collect();

    let gutters: Vec<(f64, f64)> = col_ranges
        .windows(2)
        .map(|w| (w[0].1, w[1].0))
        .collect();

    ColumnModel {
        n_cols: col_ranges.len(),
        col_ranges,
        gutters,
    }
}

/// Build x-occupancy profile from line bounding boxes within blocks.
/// Uses lines (not blocks) because a single block may contain lines from
/// both columns — but each individual line is always within one column.
fn build_occupancy_profile_from_blocks(
    blocks: &[Block],
    page_width: f64,
    fit_y0: f64,
    fit_y1: f64,
    fit_h: f64,
) -> [f64; N_BINS] {
    let mut profile = [0.0_f64; N_BINS];
    if fit_h <= 0.0 {
        return profile;
    }
    for b in blocks {
        if b.skip || b.exclude_from_layout {
            continue;
        }
        for line in &b.lines {
            let ly0 = line.y0.max(fit_y0);
            let ly1 = line.y1.min(fit_y1);
            if ly1 <= ly0 {
                continue;
            }
            let y_coverage = (ly1 - ly0) / fit_h;
            let i0 = x_to_bin(line.x0, page_width);
            let i1 = ((line.x1 / page_width * N_BINS as f64) as usize).min(N_BINS);
            for slot in &mut profile[i0..i1.max(i0)] {
                *slot = (*slot + y_coverage).min(1.0);
            }
        }
    }
    profile
}

fn median_profile(profiles: &[[f64; N_BINS]]) -> [f64; N_BINS] {
    let mut result = [0.0_f64; N_BINS];
    for i in 0..N_BINS {
        let mut vals: Vec<f64> = profiles.iter().map(|p| p[i]).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        result[i] = vals[vals.len() / 2];
    }
    result
}

fn smooth_profile(profile: &[f64; N_BINS], window: usize) -> [f64; N_BINS] {
    let mut smoothed = *profile;
    let kernel = 2 * window + 1;
    for i in window..(N_BINS - window) {
        let sum: f64 = profile[(i - window)..=(i + window)].iter().sum();
        smoothed[i] = sum / kernel as f64;
    }
    smoothed
}

fn find_components(profile: &[f64; N_BINS], threshold: f64) -> Vec<(usize, usize)> {
    // Find runs above threshold
    let mut components = Vec::new();
    let mut start = None;
    for (i, &val) in profile.iter().enumerate() {
        if val >= threshold {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start {
            components.push((s, i));
            start = None;
        }
    }
    if let Some(s) = start {
        components.push((s, N_BINS));
    }

    // Merge tiny gaps
    let min_gap = (MIN_COMPONENT_GAP * N_BINS as f64) as usize;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for comp in components {
        if let Some(last) = merged.last_mut() {
            if comp.0 - last.1 < min_gap {
                last.1 = comp.1;
                continue;
            }
        }
        merged.push(comp);
    }

    // Drop tiny components
    let min_width = (MIN_COMPONENT_WIDTH * N_BINS as f64) as usize;
    merged.retain(|c| c.1 - c.0 >= min_width);

    merged
}

fn mode(values: &[usize]) -> usize {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for &v in values {
        *counts.entry(v).or_default() += 1;
    }
    let max_count = counts.values().copied().max().unwrap_or(0);
    // On ties, prefer the first value that appeared in the input
    values
        .iter()
        .copied()
        .find(|v| counts.get(v).copied().unwrap_or(0) == max_count)
        .unwrap_or(1)
}

fn x_to_bin(x: f64, page_width: f64) -> usize {
    ((x / page_width * N_BINS as f64) as usize).min(N_BINS - 1)
}

fn bin_to_x(bin: usize, page_width: f64) -> f64 {
    bin as f64 / N_BINS as f64 * page_width
}

// ---------------------------------------------------------------------------
// Block classification against column model
// ---------------------------------------------------------------------------

pub fn classify_blocks(blocks: &mut [Block], model: &ColumnModel) {
    if model.n_cols < 2 {
        for b in blocks.iter_mut() {
            if !b.skip {
                b.column = Column::Full;
            }
        }
        return;
    }

    for b in blocks.iter_mut() {
        if b.skip {
            continue;
        }

        let bw = b.width();
        if bw <= 0.0 {
            b.column = Column::Full;
            continue;
        }

        let overlaps: Vec<f64> = model
            .col_ranges
            .iter()
            .map(|(cx0, cx1)| (b.x1.min(*cx1) - b.x0.max(*cx0)).max(0.0) / bw)
            .collect();

        let gutter_overlap: f64 = model
            .gutters
            .iter()
            .map(|(gx0, gx1)| (b.x1.min(*gx1) - b.x0.max(*gx0)).max(0.0))
            .sum::<f64>()
            / bw;

        let (best_col, &best_overlap) = overlaps
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap_or((0, &0.0));

        if best_overlap >= COL_OVERLAP_MIN && gutter_overlap <= GUTTER_OVERLAP_MAX {
            b.column = Column::Col(best_col);
        } else {
            b.column = Column::Full;
        }
    }
}

// ---------------------------------------------------------------------------
// Heading detection
// ---------------------------------------------------------------------------

const KNOWN_HEADINGS: &[&str] = &[
    "abstract",
    "introduction",
    "background",
    "related work",
    "methodology",
    "method",
    "methods",
    "approach",
    "results",
    "discussion",
    "conclusion",
    "conclusions",
    "references",
    "acknowledgments",
    "acknowledgements",
    "appendix",
    "evaluation",
    "experiments",
    "experimental setup",
    "limitations",
    "future work",
    "threats to validity",
];

pub fn detect_headings(blocks: &mut [Block], font_info: &FontInfo) {
    let texts: Vec<String> = blocks.iter().map(|b| b.text()).collect();

    // === Pass 1: Anchor on known headings to learn heading styles ===
    // Find blocks that match known heading names and record their font sizes.
    // This tells us what font size this paper uses for section headings.
    let mut anchor_sizes: Vec<f64> = Vec::new();
    let mut numbered_anchor_sizes: Vec<(f64, u8)> = Vec::new(); // (size, depth)

    for (i, block) in blocks.iter().enumerate() {
        if block.skip {
            continue;
        }
        let text = texts[i].trim();
        if text.is_empty() {
            continue;
        }
        let is_short = block.lines.len() <= HEADING_MAX_LINES && text.len() < HEADING_MAX_CHARS;
        if !is_short {
            continue;
        }

        let clean = strip_numbering(text).to_lowercase();
        let clean = clean.trim_end_matches('.').trim();
        let size = block.dominant_size();

        if KNOWN_HEADINGS.contains(&clean) && size > font_info.body_size * 0.9 {
            anchor_sizes.push(size);

            // If this known heading is numbered, record the numbering depth
            if let Some(depth) = heading_depth(text) {
                numbered_anchor_sizes.push((size, depth));
            }
        }
    }

    // Determine the heading font sizes from anchors
    // Primary heading size = most common anchor size
    // If we see numbered anchors at different depths with different sizes,
    // we can distinguish heading levels
    let primary_heading_size = if !anchor_sizes.is_empty() {
        // Most common size among anchors
        let mut size_counts: HashMap<i32, usize> = HashMap::new();
        for &s in &anchor_sizes {
            *size_counts.entry((s * 10.0).round() as i32).or_default() += 1;
        }
        size_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, _)| key as f64 / 10.0)
    } else {
        None
    };

    // Build a size → level mapping from numbered anchors
    // e.g., if "1 Introduction" is at 12pt (depth=1) and "1.1 Background" is at 10pt (depth=2),
    // then 12pt → H2, 10pt → H3
    let mut learned_levels: Vec<(f64, u8)> = Vec::new();
    if !numbered_anchor_sizes.is_empty() {
        let mut by_depth: HashMap<u8, Vec<f64>> = HashMap::new();
        for &(size, depth) in &numbered_anchor_sizes {
            by_depth.entry(depth).or_default().push(size);
        }
        let mut depths: Vec<u8> = by_depth.keys().copied().collect();
        depths.sort();
        for (rank, &depth) in depths.iter().enumerate() {
            let sizes = &by_depth[&depth];
            let avg_size = sizes.iter().sum::<f64>() / sizes.len() as f64;
            let level = (rank as u8 + 2).min(4); // depth 0 → H2, depth 1 → H3, etc.
            learned_levels.push((avg_size, level));
        }
    }

    // === Pass 2: Apply heading detection using learned styles + original signals ===
    for (i, block) in blocks.iter_mut().enumerate() {
        if block.skip {
            continue;
        }

        let text = texts[i].trim();
        if text.is_empty() {
            continue;
        }

        let size = block.dominant_size();
        let is_short = block.lines.len() <= HEADING_MAX_LINES && text.len() < HEADING_MAX_CHARS;
        if !is_short {
            continue;
        }

        let mut level: u8 = 0;

        // Signal 1: Learned heading style from anchors
        // If this block's font size matches a learned heading level, use it
        for &(learned_size, learned_level) in &learned_levels {
            if (size - learned_size).abs() < HEADING_SIZE_TOLERANCE {
                level = learned_level;
                break;
            }
        }

        // Signal 2: Primary heading size from anchors (when no numbered anchors gave us levels)
        if level == 0 {
            if let Some(primary) = primary_heading_size {
                if (size - primary).abs() < HEADING_SIZE_TOLERANCE {
                    level = 2;
                }
            }
        }

        // Signal 3: Font size tier (original approach, fallback)
        if level == 0 {
            level = font_info
                .tiers
                .iter()
                .find(|(tier_size, _)| (size - tier_size).abs() < HEADING_SIZE_TOLERANCE)
                .map(|(_, tier_level)| *tier_level)
                .unwrap_or(0);
        }

        // Signal 4: Known heading text (fallback for same-size headings)
        if level == 0 {
            let clean = strip_numbering(text).to_lowercase();
            let clean = clean.trim_end_matches('.');
            if KNOWN_HEADINGS.contains(&clean) {
                level = if size >= font_info.body_size { 2 } else { 3 };
            }
        }

        // Signal 5: All-caps short text
        if level == 0 && text.len() > 3 {
            let alpha: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
            if alpha.len() > 2 && alpha.iter().all(|c| c.is_uppercase()) {
                level = 2;
            }
        }

        // Adjust for numbered heading depth
        let numbered = text.starts_with(|c: char| c.is_ascii_digit())
            || text.starts_with(|c: char| "IVXLC".contains(c));
        if numbered && level > 1 {
            if let Some(depth) = heading_depth(text) {
                level = level.min(2 + depth).min(4);
            }
        }

        if level > 0 {
            block.heading_level = level.min(4);
        }
    }
}

fn strip_numbering(text: &str) -> &str {
    let s = text.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    let s = s.trim_start_matches(|c: char| "IVXLC".contains(c));
    s.trim_start_matches(|c: char| c == '.' || c == ' ')
}

fn heading_depth(text: &str) -> Option<u8> {
    let prefix: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.matches('.').count() as u8)
}

// ---------------------------------------------------------------------------
// Reading order
// ---------------------------------------------------------------------------

pub fn compute_reading_order<'a>(blocks: &'a [Block], model: &ColumnModel) -> Vec<&'a Block> {
    let active: Vec<&Block> = blocks.iter().filter(|b| !b.skip).collect();

    if model.n_cols < 2 {
        let mut sorted = active;
        sorted.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap());
        return sorted;
    }

    let body_start_y = detect_body_start(&active, model);

    // Categorize blocks
    let mut preamble: Vec<&Block> = Vec::new();
    let mut col_content: Vec<Vec<&Block>> = vec![Vec::new(); model.n_cols];
    let mut floating_spans: Vec<&Block> = Vec::new();

    for b in &active {
        match b.column {
            Column::Full => {
                if b.y0 < body_start_y + PREAMBLE_Y_TOLERANCE {
                    preamble.push(b);
                } else {
                    floating_spans.push(b);
                }
            }
            Column::Col(i) if i < model.n_cols => {
                col_content[i].push(b);
            }
            _ => {
                floating_spans.push(b);
            }
        }
    }

    // Sort each group by y-position
    preamble.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap());
    for col in &mut col_content {
        col.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap());
    }
    floating_spans.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap());

    // Build result: preamble, then merge column stream with floating spans
    let mut result = preamble;

    // Column stream: all columns in left-to-right order
    let column_stream: Vec<&Block> = col_content.iter().flatten().copied().collect();

    if floating_spans.is_empty() {
        result.extend(column_stream);
    } else {
        // Merge floating spans into the column stream at column transitions.
        // Build a list of (y_threshold, stream_index) for each transition.
        let mut transition_y: Vec<(f64, usize)> = Vec::new();
        let mut offset = 0;
        for col in &col_content {
            offset += col.len();
            let max_y = col.iter().map(|b| b.y1).fold(f64::NEG_INFINITY, f64::max);
            transition_y.push((max_y, offset));
        }

        // Merge the two sorted-by-y sequences: column_stream and floating_spans.
        // Floats are inserted at the first column transition whose y >= float.y0.
        let mut stream_idx = 0;
        let mut float_idx = 0;
        let mut next_transition = 0; // index into transition_y

        while stream_idx < column_stream.len() || float_idx < floating_spans.len() {
            // Check if we're at a transition point and have floats to insert
            if let Some(&(trans_y, trans_offset)) = transition_y.get(next_transition) {
                if stream_idx >= trans_offset {
                    // We've passed this transition — insert any floats that belong here
                    while float_idx < floating_spans.len()
                        && floating_spans[float_idx].y0 <= trans_y + PREAMBLE_Y_TOLERANCE
                    {
                        result.push(floating_spans[float_idx]);
                        float_idx += 1;
                    }
                    next_transition += 1;
                    continue;
                }
            }

            if stream_idx < column_stream.len() {
                result.push(column_stream[stream_idx]);
                stream_idx += 1;
            } else {
                // Remaining floats
                result.push(floating_spans[float_idx]);
                float_idx += 1;
            }
        }

        // Any remaining floats after all transitions
        while float_idx < floating_spans.len() {
            result.push(floating_spans[float_idx]);
            float_idx += 1;
        }
    }

    result
}

fn detect_body_start(blocks: &[&Block], model: &ColumnModel) -> f64 {
    if model.n_cols < 2 {
        return 0.0;
    }

    let mut col_starts: Vec<f64> = Vec::new();
    for col_idx in 0..model.n_cols {
        if let Some(min_y) = blocks
            .iter()
            .filter(|b| b.column == Column::Col(col_idx))
            .map(|b| b.y0)
            .reduce(f64::min)
        {
            col_starts.push(min_y);
        }
    }

    if col_starts.len() < 2 {
        // Fewer than 2 columns have content — no clear body start.
        // Return a high value so nothing is classified as preamble.
        return f64::MAX;
    }

    col_starts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    col_starts[0]
}

// ---------------------------------------------------------------------------
// Debug JSON output
// ---------------------------------------------------------------------------

/// Produce a JSON string describing all blocks and their classifications.
pub fn debug_json(
    blocks: &[Block],
    ordered: &[&Block],
    page_width: f64,
    page_height: f64,
    model: &ColumnModel,
) -> String {
    let order_map: HashMap<*const Block, usize> = ordered
        .iter()
        .enumerate()
        .map(|(i, b)| (*b as *const Block, i + 1))
        .collect();

    let block_entries: Vec<String> = blocks
        .iter()
        .map(|b| {
            let col_str = match b.column {
                Column::Full => "full".to_string(),
                Column::Col(i) => format!("col{}", i),
            };
            let classification = if b.skip {
                "skip"
            } else if b.heading_level > 0 {
                "heading"
            } else {
                "text"
            };
            let order = order_map.get(&(b as *const Block)).copied().unwrap_or(0);
            let text: String = b.text().chars().take(80).collect();
            let text = escape_json_string(&text);

            format!(
                "    {{\"bbox\": [{:.1}, {:.1}, {:.1}, {:.1}], \"column\": \"{}\", \
                 \"type\": \"{}\", \"heading\": {}, \"skip\": {}, \"layout_excluded\": {}, \
                 \"order\": {}, \"text\": \"{}\"}}",
                b.x0, b.y0, b.x1, b.y1, col_str, classification, b.heading_level, b.skip,
                b.exclude_from_layout, order,
                text
            )
        })
        .collect();

    let col_ranges: String = model
        .col_ranges
        .iter()
        .map(|(x0, x1)| format!("[{:.1}, {:.1}]", x0, x1))
        .collect::<Vec<_>>()
        .join(", ");

    let gutter_ranges: String = model
        .gutters
        .iter()
        .map(|(x0, x1)| format!("[{:.1}, {:.1}]", x0, x1))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "{{\n  \"page_width\": {:.1},\n  \"page_height\": {:.1},\n  \
         \"n_cols\": {},\n  \"col_ranges\": [{}],\n  \"gutters\": [{}],\n  \
         \"blocks\": [\n{}\n  ]\n}}",
        page_width, page_height, model.n_cols, col_ranges, gutter_ranges,
        block_entries.join(",\n")
    )
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
