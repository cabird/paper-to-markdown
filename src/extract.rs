//! PDF character extraction via pdf-extract's OutputDev.
//!
//! Computes per-glyph positions from the PDF content stream:
//! - size = matrix_expansion(trm) = sqrt(|det(trm)|)
//! - dir = normalize(transform_vector((1,0), trm))  
//! - p = (trm.e, trm.f) after Y-flip
//! - q = p + adv * dir (pen after glyph advance)
//!
//! Also pre-scans content stream with lopdf to track BT/ET text object
//! boundaries for the text_object_id.

use lopdf::Document as LopdfDocument;
use pdf_extract::{Document as PdfDocument, MediaBox, OutputDev, OutputError, Transform};
use std::collections::BTreeMap;
use std::panic;

/// Per-glyph record with precise positioning from the PDF content stream.
#[derive(Debug, Clone)]
pub struct Glyph {
    /// Glyph origin in PDF coords (origin bottom-left) — used for grouping
    pub p_pdf: (f64, f64),
    /// Pen position after advance in PDF coords — used for grouping
    pub q_pdf: (f64, f64),
    /// Glyph origin in page coords (origin top-left) — used for output
    pub p: (f64, f64),
    /// Font size (matrix expansion)
    pub size: f64,
    /// Normalized text direction in PDF coords
    pub dir: (f64, f64),
    /// Decoded unicode character
    pub text: String,
    /// BT/ET text object ID
    pub text_object_id: u32,
    /// Character spacing passed from text state
    pub spacing: f64,
}

/// All extracted data for one page.
#[derive(Debug)]
pub struct PageData {
    pub page_num: u32,
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
}

// ---------------------------------------------------------------------------
// Pass 1: Pre-scan BT/ET boundaries
// ---------------------------------------------------------------------------

fn prescan_text_objects(bytes: &[u8]) -> BTreeMap<u32, Vec<u32>> {
    let doc = match LopdfDocument::load_mem(bytes) {
        Ok(d) => d,
        Err(_) => return BTreeMap::new(),
    };

    let pages = doc.get_pages();
    let mut result = BTreeMap::new();

    for (&page_num, &page_id) in &pages {
        let content = match doc.get_and_decode_page_content(page_id) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut text_object_id: u32 = 0;
        let mut in_text_object = false;
        let mut word_to_object: Vec<u32> = Vec::new();

        for op in &content.operations {
            let operator: &str = &op.operator;
            match operator {
                "BT" => {
                    text_object_id += 1;
                    in_text_object = true;
                }
                "ET" => {
                    in_text_object = false;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    if in_text_object {
                        word_to_object.push(text_object_id);
                    } else {
                        text_object_id += 1;
                        word_to_object.push(text_object_id);
                    }
                }
                _ => {}
            }
        }

        result.insert(page_num, word_to_object);
    }

    result
}

// ---------------------------------------------------------------------------
// Pass 2: Extract glyphs with precise positioning
// ---------------------------------------------------------------------------

struct PdfOutputDev {
    pages: Vec<PageData>,
    current_glyphs: Vec<Glyph>,
    current_page_num: u32,
    page_width: f64,
    page_height: f64,
    flip_ctm: Transform,

    // Text object tracking
    page_word_maps: BTreeMap<u32, Vec<u32>>,
    word_index: usize,
    current_text_object_id: u32,
    fallback_id_counter: u32,
}

impl PdfOutputDev {
    fn new(page_word_maps: BTreeMap<u32, Vec<u32>>) -> Self {
        Self {
            pages: Vec::new(),
            current_glyphs: Vec::new(),
            current_page_num: 0,
            page_width: 0.0,
            page_height: 0.0,
            flip_ctm: Transform::identity(),
            page_word_maps,
            word_index: 0,
            current_text_object_id: 0,
            fallback_id_counter: 100_000,
        }
    }
}

impl OutputDev for PdfOutputDev {
    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.current_page_num = page_num;
        self.page_width = media_box.urx - media_box.llx;
        self.page_height = media_box.ury - media_box.lly;
        self.flip_ctm = Transform::row_major(1., 0., 0., -1., 0., self.page_height);
        self.current_glyphs = Vec::new();
        self.word_index = 0;
        self.current_text_object_id = 0;
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        self.pages.push(PageData {
            page_num: self.current_page_num,
            width: self.page_width,
            height: self.page_height,
            glyphs: std::mem::take(&mut self.current_glyphs),
        });
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,   // w0 = glyph width / 1000 (text space)
        spacing: f64,  // character_spacing + word_spacing
        font_size: f64,
        ch: &str,
    ) -> Result<(), OutputError> {
        // size = matrix_expansion(trm)
        // Note: pdf-extract's trm does NOT include font_size (it's passed separately)
        // pdf-extract's trm does NOT include font_size, so we multiply.
        let expansion = (trm.m11 * trm.m22 - trm.m12 * trm.m21).abs().sqrt();
        let size = expansion * font_size.abs();

        // dir = transform_vector((1,0), trm) then normalize
        // dir = (trm.m11, trm.m12) — includes scaling
        let dir_x = trm.m11 as f64;
        let dir_y = trm.m12 as f64;
        let dir_len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        let ndir = if dir_len > 0.0 {
            (dir_x / dir_len, dir_y / dir_len)
        } else {
            (1.0, 0.0)
        };

        // p = (trm.e, trm.f) in PDF coords (origin bottom-left)
        let p_pdf = (trm.m31 as f64, trm.m32 as f64);

        // q = p + w0 * dir (where dir includes font_size)
        // pdf-extract's trm doesn't include font_size, so we scale by it
        let q_pdf = (
            p_pdf.0 + width * dir_x * font_size.abs(),
            p_pdf.1 + width * dir_y * font_size.abs(),
        );

        // Flip to page coords (origin top-left) for output only
        let p = (p_pdf.0, self.page_height - p_pdf.1);

        self.current_glyphs.push(Glyph {
            p_pdf,
            q_pdf,
            p,
            size,
            dir: ndir,  // PDF-space direction (NOT flipped)
            text: ch.to_string(),
            text_object_id: self.current_text_object_id,
            spacing,
        });
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        if let Some(word_map) = self.page_word_maps.get(&self.current_page_num) {
            if self.word_index < word_map.len() {
                self.current_text_object_id = word_map[self.word_index];
            } else {
                self.fallback_id_counter += 1;
                self.current_text_object_id = self.fallback_id_counter;
            }
        } else {
            self.fallback_id_counter += 1;
            self.current_text_object_id = self.fallback_id_counter;
        }
        self.word_index += 1;
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn extract_pages(bytes: &[u8]) -> Vec<PageData> {
    let page_word_maps = prescan_text_objects(bytes);

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let doc = PdfDocument::load_mem(bytes).expect("Failed to load PDF");
        let mut output = PdfOutputDev::new(page_word_maps);
        pdf_extract::output_doc(&doc, &mut output).expect("Failed to extract PDF");
        output.pages
    }));

    match result {
        Ok(pages) => pages,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            eprintln!("Warning: pdf-extract panicked: {msg}");
            Vec::new()
        }
    }
}
