//! PDF text grouping: glyphs → lines → blocks.
//!
//! All grouping math (spacing, base_offset) uses PDF coordinates
//! (origin bottom-left). Output bboxes use page coordinates.
//! use page coordinates (origin top-left) for downstream use.

use crate::extract::Glyph;

const PARAGRAPH_DIST: f64 = 1.5;
const SPACE_DIST: f64 = 0.15;
const SPACE_MAX_DIST: f64 = 0.8;
const BASE_MAX_DIST: f64 = 0.8;
const FAKE_BOLD_MAX_DIST: f64 = 0.1;

#[derive(Debug, Clone)]
pub struct TextBlock {
    pub bbox: [f64; 4],
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub bbox: [f64; 4],
    pub dir: (f64, f64),
    pub spans: Vec<TextSpan>,
}

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub bbox: [f64; 4],
    pub size: f64,
    pub text: String,
}

struct GroupedChar {
    c: char,
    /// Page coords for bbox output
    page_x: f64,
    page_y: f64,
    size: f64,
    /// Width in page coords
    width: f64,
    is_space: bool,
}

struct BuildingLine {
    dir: (f64, f64),
    chars: Vec<GroupedChar>,
}

struct State {
    blocks: Vec<TextBlock>,
    cur_block: Option<TextBlock>,
    cur_line: Option<BuildingLine>,
    /// All tracking in PDF coords (bottom-left origin)
    pen: Option<(f64, f64)>,
    lag_pen: Option<(f64, f64)>,
    last_char: Option<char>,
    line_dir: (f64, f64),
    line_start_x: f64,
    page_height: f64,
}

impl State {
    fn new(page_height: f64) -> Self {
        Self {
            blocks: Vec::new(),
            cur_block: None,
            cur_line: None,
            pen: None,
            lag_pen: None,
            last_char: None,
            line_dir: (1.0, 0.0),
            line_start_x: 0.0,
            page_height,
        }
    }

    fn add_glyph(&mut self, g: &Glyph) {
        let c = g.text.chars().next().unwrap_or(' ');
        let size = g.size.max(0.01);

        // All grouping math in PDF coords
        let p = g.p_pdf;
        let q = g.q_pdf;
        let ndir = g.dir;

        // Fake bold
        if let Some(lag) = self.lag_pen {
            let dist = ((p.0 - lag.0).powi(2) + (p.1 - lag.1).powi(2)).sqrt() / size;
            if dist < FAKE_BOLD_MAX_DIST && self.last_char == Some(c) {
                return;
            }
        }

        let mut new_para = false;
        let mut new_line = true;
        let mut add_space = false;

        if let Some(pen) = self.pen {
            // Direction change
            let dot = ndir.0 * self.line_dir.0 + ndir.1 * self.line_dir.1;
            if self.cur_line.is_some() && dot < 0.999 {
                new_para = true;
            } else {
                let delta = (p.0 - pen.0, p.1 - pen.1);
                let spacing = (ndir.0 * delta.0 + ndir.1 * delta.1) / size;
                let base_offset = (-ndir.1 * delta.0 + ndir.0 * delta.1) / size;
                if std::env::var("DBG").is_ok() && self.blocks.is_empty() { eprintln!("  {:?} sp={:.3} bo={:.3} p=({:.1},{:.1}) pen=({:.1},{:.1}) q=({:.1},{:.1}) sz={:.1}", c, spacing, base_offset, p.0, p.1, pen.0, pen.1, q.0, q.1, size); }

                if base_offset.abs() < BASE_MAX_DIST {
                    if spacing.abs() < SPACE_DIST {
                        new_line = false;
                    } else if spacing < 0.0 && spacing > -SPACE_MAX_DIST {
                        new_line = false;
                    } else if spacing > 0.0 && spacing < SPACE_MAX_DIST {
                        add_space = true;
                        new_line = false;
                    }
                    // else: large jump → new_line stays true
                } else if base_offset.abs() <= PARAGRAPH_DIST {
                    if (p.0 - self.line_start_x) > 0.5 {
                        new_para = true;
                    }
                    new_line = true;
                } else {
                    new_para = true;
                    new_line = true;
                }
            }
        } else {
            new_para = true;
        }

        if new_para {
            self.flush_line();
            self.flush_block();
            self.cur_block = Some(TextBlock { bbox: [0.0; 4], lines: Vec::new() });
        }
        if new_line {
            self.flush_line();
            self.cur_line = Some(BuildingLine { dir: ndir, chars: Vec::new() });
            self.line_dir = ndir;
            self.line_start_x = p.0;
        }

        // Page coords for output
        let page_x = g.p.0;
        let page_y = g.p.1;
        let width = (g.q_pdf.0 - g.p_pdf.0).abs(); // approximate page-space width

        if add_space {
            if let Some(ref mut line) = self.cur_line {
                line.chars.push(GroupedChar {
                    c: ' ',
                    page_x,
                    page_y,
                    size,
                    width: 0.0,
                    is_space: true,
                });
            }
        }

        if let Some(ref mut line) = self.cur_line {
            line.chars.push(GroupedChar {
                c,
                page_x,
                page_y,
                size,
                width,
                is_space: false,
            });
        }

        self.lag_pen = Some(p);
        self.pen = Some(q);
        self.last_char = Some(c);
    }

    fn flush_line(&mut self) {
        if let Some(line) = self.cur_line.take() {
            if line.chars.is_empty() { return; }
            let text: String = line.chars.iter().map(|c| c.c).collect();
            let bbox = chars_bbox(&line.chars);
            let size = line.chars.iter().find(|c| !c.is_space).map(|c| c.size).unwrap_or(10.0);
            let tl = TextLine {
                bbox,
                dir: line.dir,
                spans: vec![TextSpan { bbox, size, text }],
            };
            if let Some(ref mut block) = self.cur_block {
                block.lines.push(tl);
            }
        }
    }

    fn flush_block(&mut self) {
        if let Some(mut block) = self.cur_block.take() {
            if block.lines.is_empty() { return; }
            block.bbox = lines_bbox(&block.lines);
            self.blocks.push(block);
        }
    }

    fn finish(mut self) -> Vec<TextBlock> {
        self.flush_line();
        self.flush_block();
        self.blocks
    }
}

fn chars_bbox(chars: &[GroupedChar]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for c in chars {
        if c.is_space { continue; }
        b[0] = b[0].min(c.page_x);
        b[1] = b[1].min(c.page_y - c.size * 0.8);
        b[2] = b[2].max(c.page_x + c.width);
        b[3] = b[3].max(c.page_y + c.size * 0.2);
    }
    b
}

fn lines_bbox(lines: &[TextLine]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for l in lines {
        b[0] = b[0].min(l.bbox[0]);
        b[1] = b[1].min(l.bbox[1]);
        b[2] = b[2].max(l.bbox[2]);
        b[3] = b[3].max(l.bbox[3]);
    }
    b
}

pub fn group_glyphs(glyphs: &[Glyph], page_height: f64) -> Vec<TextBlock> {
    let mut state = State::new(page_height);
    for g in glyphs {
        if g.text.is_empty() { continue; }
        state.add_glyph(g);
    }
    state.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(c: char, x: f64, y: f64, size: f64) -> Glyph {
        let w = size * 0.5;
        Glyph {
            p_pdf: (x, y),
            q_pdf: (x + w, y),
            p: (x, 800.0 - y),
            size,
            dir: (1.0, 0.0),
            text: c.to_string(),
            text_object_id: 0,
            spacing: 0.0,
        }
    }

    #[test]
    fn test_single_line() {
        let glyphs: Vec<Glyph> = "Hello"
            .chars()
            .enumerate()
            .map(|(i, c)| glyph(c, i as f64 * 6.0, 700.0, 10.0))
            .collect();
        let blocks = group_glyphs(&glyphs, 800.0);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 1);
    }

    #[test]
    fn test_new_block() {
        let mut glyphs = Vec::new();
        for (i, c) in "AB".chars().enumerate() {
            glyphs.push(glyph(c, i as f64 * 6.0, 700.0, 10.0));
        }
        // y offset = -20pt (going DOWN in PDF coords) = 2.0 * size > PARAGRAPH_DIST
        for (i, c) in "CD".chars().enumerate() {
            glyphs.push(glyph(c, i as f64 * 6.0, 680.0, 10.0));
        }
        let blocks = group_glyphs(&glyphs, 800.0);
        assert_eq!(blocks.len(), 2);
    }
}
