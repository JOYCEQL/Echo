//! Read-only Parley text geometry queries for custom app-owned document editors.
//! Uses [`super::measure::with_text`] so fonts, wrapping and line metrics match [`crate::rich`] pixels.

use parley::{Alignment, BreakReason, editing::Cursor, layout::Affinity};

use super::measure::with_text;
use super::{RichLayoutOptions, Span, cursor};
use crate::Align;
use crate::tok::TextStyle;

#[derive(Clone, Debug, PartialEq)]
pub struct LineSpan {
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocLayoutInfo {
    pub width: f32,
    pub height: f32,
    pub lines: Vec<LineSpan>,
}

fn options_for(align: Align) -> RichLayoutOptions {
    let a = match align {
        Align::Start => Alignment::Start,
        Align::Center => Alignment::Center,
        Align::End => Alignment::End,
    };
    RichLayoutOptions::new().align(a)
}

fn clamp_char_boundary(s: &str, byte: usize) -> usize {
    let mut i = byte.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Measures `text` with `spans` wrapped at `max_width` points and returns line ranges/bounds.
pub fn measure_lines(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
) -> DocLayoutInfo {
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        let w = if max_width.is_some() { l.width() } else { l.full_width() }.ceil();
        let h = super::line_metrics::spec_height(&l, style, 1.0, true);
        let mut lines = Vec::with_capacity(l.len());
        for line in l.lines() {
            let m = line.metrics();
            let r = line.text_range();
            let end = if line.break_reason() == BreakReason::Explicit && r.end > r.start {
                r.end - 1
            } else {
                r.end
            };
            lines.push(LineSpan {
                start: r.start,
                end: end.max(r.start),
                top: m.block_min_coord.min(m.content_block_min_coord),
                bottom: m.block_max_coord.max(m.content_block_max_coord),
            });
        }
        DocLayoutInfo { width: w, height: h, lines }
    })
}

/// Hit-tests `(x, y)` (in layout points from top-left of the text box) to a char-boundary byte offset.
pub fn hit_test(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
    x: f32,
    y: f32,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        clamp_char_boundary(text, cursor::cursor_from_point(&l, x, y).index())
    })
}

/// Returns `[x, y, w, h]` in layout points for the caret at `byte`.
pub fn caret_rect(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
    byte: usize,
) -> [f32; 4] {
    let lh = style.line_px();
    if text.is_empty() {
        let x = match (align, max_width) {
            (Align::Center, Some(w)) => (w * 0.5).max(0.0),
            (Align::End, Some(w)) => (w - 1.5).max(0.0),
            _ => 0.0,
        };
        return [x, 0.0, 1.5, lh];
    }
    let at = clamp_char_boundary(text, byte);
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        let cur = Cursor::from_byte_index(&l, at, Affinity::Downstream);
        let bb = cursor::cursor_geometry(&cur, &l, 1.5);
        let x = bb.x0 as f32;
        let y = bb.y0 as f32;
        let h = ((bb.y1 - bb.y0) as f32).max(lh);
        [x.max(0.0), y.max(0.0), 1.5, h]
    })
}

/// Returns selection highlight rectangles `[x, y, w, h]` in layout points for byte range `start..end`.
pub fn selection_rects(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
    start: usize,
    end: usize,
) -> Vec<[f32; 4]> {
    let a = clamp_char_boundary(text, start);
    let b = clamp_char_boundary(text, end);
    if a >= b {
        return Vec::new();
    }
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        cursor::range_boxes(&l, a, b, false)
            .into_iter()
            .map(|(bb, _)| {
                let x = bb.x0 as f32;
                let y = bb.y0 as f32;
                let w = ((bb.x1 - bb.x0) as f32).max(0.0);
                let h = ((bb.y1 - bb.y0) as f32).max(style.line_px() * 0.8);
                [x, y, w, h]
            })
            .collect()
    })
}

/// Returns start (`!end`) or end (`end`) byte offset of the visual line containing `byte`.
pub fn line_edge(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
    byte: usize,
    end: bool,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let at = clamp_char_boundary(text, byte);
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        clamp_char_boundary(text, cursor::line_edge(&l, at, end))
    })
}

/// Moves one visual line up (`!down`) or down (`down`), keeping `goal_x` if supplied.
/// Returns `None` when already on the top visual line moving up or bottom visual line moving down.
pub fn move_vertical(
    text: &str,
    spans: &[Span],
    style: TextStyle,
    max_width: Option<f32>,
    align: Align,
    byte: usize,
    goal_x: Option<f32>,
    down: bool,
) -> Option<(usize, f32)> {
    if text.is_empty() {
        return None;
    }
    let at = clamp_char_boundary(text, byte);
    with_text(|t| {
        let l = t.layout_rich_with(text, spans, style, 1.0, max_width, options_for(align));
        if l.len() == 0 {
            return None;
        }
        let cur = Cursor::from_byte_index(&l, at, Affinity::Downstream);
        let bb = cursor::cursor_geometry(&cur, &l, 0.0);
        let cur_x = bb.x0 as f32;
        let cy = ((bb.y0 + bb.y1) * 0.5) as f32;
        let mut line_ix = l.len() - 1;
        for (ix, line) in l.lines().enumerate() {
            let m = line.metrics();
            if cy >= m.block_min_coord && cy <= m.block_max_coord {
                line_ix = ix;
                break;
            }
        }
        let target_ix = if down {
            if line_ix + 1 < l.len() { line_ix + 1 } else { return None; }
        } else if line_ix > 0 {
            line_ix - 1
        } else {
            return None;
        };
        let target = l.get(target_ix)?;
        let tm = target.metrics();
        let target_y = (tm.block_min_coord + tm.block_max_coord) * 0.5;
        let gx = goal_x.unwrap_or(cur_x);
        let next_byte = clamp_char_boundary(text, cursor::cursor_from_point(&l, gx, target_y).index());
        Some((next_byte, gx))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_and_move_vertical_work_across_cjk_and_wrapped_lines() {
        let style = TextStyle::new(16.0, 400.0).lh(26.0);
        let text = "第一行你好世界\n第二行测试";
        let info = measure_lines(text, &[], style, Some(400.0), Align::Start);
        assert_eq!(info.lines.len(), 2);
        let end_of_first = info.lines[0].end;
        let (down_byte, _) = move_vertical(text, &[], style, Some(400.0), Align::Start, 0, None, true).unwrap();
        assert!(down_byte >= info.lines[1].start && down_byte <= info.lines[1].end);
        assert!(move_vertical(text, &[], style, Some(400.0), Align::Start, 0, None, false).is_none());
        assert!(move_vertical(text, &[], style, Some(400.0), Align::Start, down_byte, None, true).is_none());
        assert_eq!(line_edge(text, &[], style, Some(400.0), Align::Start, 0, true), end_of_first);
        let r = caret_rect(text, &[], style, Some(400.0), Align::Start, down_byte);
        assert!(r[1] >= info.lines[1].top - 1.0);
    }
}


/// Hit test the same image-aware layout painted by `El::inline_images`.
pub fn hit_test_images(text: &str, spans: &[Span], images: &[crate::InlineImage], style: TextStyle,
    max_width: Option<f32>, align: Align, x: f32, y: f32) -> usize {
    with_text(|t| {
        let pills: Vec<_> = images.iter().filter_map(|i| i.pill(text)).collect();
        let l = t.layout_blocks(text, spans, &[], &pills, style, 1., max_width, match align {
            Align::Start => Alignment::Start, Align::Center => Alignment::Center, Align::End => Alignment::End,
        });
        clamp_char_boundary(text, cursor::cursor_from_point(&l, x, y).index())
    })
}
