use crate::editor_palette::callout_palette_colors;
use crate::{
    doc_core::{
        DocAlign, DocBlock, DocBlockKind, DocCore, DocDocument, DocMark, DocPos, DocPreedit,
        DocSel, RichText, TITLE_ID, clamp_utf8, word_at,
    },
    icons::*,
    model::{Note, State},
    ui::{c, font, label},
};
use rune::{
    Align, ClipboardContent, Cursor, El, Family, FileDialogOptions, GrowthSpec, Icon, Ime, Key,
    KeyEvent, MotionIntent, Phase, ScrollSpec, Span, Spring, TextEdit, TextHighlight, TextStyle,
    Tx, col, el, rich, row, text,
};

#[path = "image_block.rs"]
mod image_block;

/// Rendering context shared by a whole page and a document inside a canvas card.
pub struct RenderState<'a> {
    pub state: &'a State,
    card_id: Option<&'a str>,
    blank_canvas_input: bool,
    pub editor: SessionView<'a>,
}
/// A render projection is bound to its card, independently of keyboard ownership.
pub struct SessionView<'a> {
    pub active: &'a DocSession,
}
impl std::ops::Deref for RenderState<'_> {
    type Target = State;
    fn deref(&self) -> &State {
        self.state
    }
}
fn render_state(s: &State) -> RenderState<'_> {
    RenderState {
        state: s,
        editor: SessionView {
            active: &s.editor.active,
        },
        card_id: None,
        blank_canvas_input: false,
    }
}

impl RenderState<'_> {
    pub fn in_canvas(&self) -> bool { self.mode == crate::model::WorkspaceMode::Canvas }

    fn body_style(&self) -> TextStyle {
        if self.in_canvas() { font(14.5, 400.).lh(21.) } else { font(BODY_SIZE, 400.).lh(BODY_LINE) }
    }
    fn heading_style(&self, level: u8) -> (f32, f32, f32) {
        match (self.in_canvas(), level) {
            (true, 1) => (21., 28., 650.),
            (true, 2) => (18., 25., 620.),
            (true, 3) => (16., 23., 600.),
            (true, _) => (15., 21., 580.),
            (false, 1) => (26., 36.4, 500.),
            (false, 2) => (22., 30.8, 500.),
            (false, 3) => (20., 28., 500.),
            (false, _) => (18., 25.2, 500.),
        }
    }
}

fn style_lh(style: &TextStyle) -> f32 {
    if style.line_height > 0. {
        style.line_height
    } else {
        style.size * 1.25
    }
}

fn track_rect(el: El, f: impl Fn(&mut State, [f32; 4]) + Clone + 'static) -> El {
    let f2 = f.clone();
    el.on_layout_tx(move |s: &mut State, r, tx| {
        f(s, r);
        tx.live_only();
    })
    .on_drawn_layout_tx(move |s: &mut State, r, tx| {
        f2(s, r);
        tx.live_only();
    })
}
use rune_kit::ScrollBar;
use std::{cell::RefCell, collections::HashMap, time::Instant};

#[derive(Clone)]
pub struct TextBoxMeta {
    drawn_rect: rune::Live<[f32; 4]>,
    pub scale: f32,
    pub align: Align,
    pub rect: [f32; 4],
    pub display_text: String,
    pub spans: Vec<Span>,
    pub style: TextStyle,
    pub max_width: Option<f32>,
    pub raw_len: usize,
    pub preedit_start: usize,
    pub preedit_len: usize,
    preedit_removed: usize,
    pub is_placeholder: bool,
    pub inline_preview: Option<crate::rich_render::InlinePreview>,
}

impl TextBoxMeta {
    pub fn hit_test(&self, x: f32, y: f32) -> usize {
        match &self.inline_preview {
            Some(preview) => rune::text::doc_text::hit_test_images(&self.display_text, &self.spans,
                &preview.images, self.style, self.max_width, self.align, x, y),
            None => rune::text::doc_text::hit_test(&self.display_text, &self.spans, self.style,
                self.max_width, self.align, x, y),
        }
    }
    pub fn raw_to_display(&self, raw_off: usize) -> usize {
        let off = raw_off.min(self.raw_len);
        if let Some(preview) = &self.inline_preview { return preview.raw_to_display(off); }
        if off <= self.preedit_start {
            off
        } else {
            off.max(self.preedit_start + self.preedit_removed) - self.preedit_removed + self.preedit_len
        }
    }

    pub fn display_to_raw(&self, disp_off: usize) -> usize {
        if let Some(preview) = &self.inline_preview { return preview.display_to_raw(disp_off).min(self.raw_len); }
        if disp_off <= self.preedit_start {
            disp_off.min(self.raw_len)
        } else if disp_off < self.preedit_start + self.preedit_len {
            self.preedit_start.min(self.raw_len)
        } else {
            (disp_off - self.preedit_len + self.preedit_removed).min(self.raw_len)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReorderRow {
    before: bool,
    // Sibling insertion anchor and last visible row for the indicator.
    below: Option<(u64, u64)>,
}

#[derive(Clone, Debug)]
pub enum DocDrag {
    Selecting {
        moved: bool,
    },
    ReorderBlock {
        block_id: u64,
        target_block: Option<u64>,
        after: bool,
        moved: bool,
        targets: std::rc::Rc<HashMap<u64, ReorderRow>>,
        pointer: [f32; 2],
    },
}

#[derive(Clone, Debug)]
pub struct DocSlash {
    pub block: u64,
    pub trigger_offset: usize,
    pub trigger: char,
    pub query: String,
    pub selected: usize,
}

#[derive(Clone, Debug)]
pub enum DocPopover {
    BlockMenu { block_id: u64 },
    CalloutStyle { block_id: u64 },
    CodeLang { block_id: u64 },
    HeadingMenu,
    ColorMenu,
    AlignMenu,
    MoreMenu,
    LinkMenu,
}

use crate::insert_menu::{SLASH_CMDS, SlashCmd};

const CODE_LANGS: &[&str] = &[
    "Plain Text",
    "Rust",
    "TypeScript",
    "JavaScript",
    "Python",
    "SQL",
    "JSON",
    "Bash",
    "Markdown",
    "Mermaid",
    "LaTeX",
];

const CALLOUT_EMOJIS: &[&str] = &["💡", "✨", "📌", "⚠️", "🚀", "📘", "❓", "🎯"];
const CALLOUT_PALETTES: &[(&str, &str)] = &[
    ("orange", "暖橙"),
    ("blue", "海蓝"),
    ("green", "翠绿"),
    ("purple", "星紫"),
    ("red", "警示红"),
    ("gray", "静谧灰"),
];

pub struct DocSession {
    pub note_id: String,
    pub core: DocCore,
    pub focused: bool,
    pub typing: bool,
    pub hover_block: Option<u64>,
    pub goal_x: Option<f32>,
    row_index: RefCell<DocumentRowIndex>,
    pub block_rects: HashMap<u64, [f32; 4]>,
    pub text_layouts: RefCell<HashMap<u64, std::rc::Rc<RefCell<TextBoxMeta>>>>,
    pub caret_opacity: rune::Live<f32>,
    caret_drawn_rect: rune::Live<[f32; 4]>,
    pub window_focused: bool,
    caret_epoch: f64,
    caret_selection: Option<DocSel>,
    pub page_rect: Option<[f32; 4]>,
    pub viewport_rect: Option<[f32; 4]>,
    pub drag: Option<DocDrag>,
    pub slash: Option<DocSlash>,
    pub popover: Option<DocPopover>,
    pub copied_block: Option<(u64, Instant)>,
    pub editing_cell: Option<(u64, usize, usize, String)>,
    // Native blur callbacks finish after the active cell changes. Keep their editors alive.
    pub cell_inputs: HashMap<String, TextEdit>,
    pub cell_undo_started: bool,
    pub table_selection: Option<crate::doc_tables::CellSelection>,
    image_resize: Option<image_block::Resize>,
    pub table_resize: Option<(u64, usize, Vec<f32>)>,
    pub link_input: TextEdit,
}

impl Default for DocSession {
    fn default() -> Self {
        Self {
            note_id: String::new(),
            core: DocCore::new(DocDocument::empty()),
            focused: true,
            typing: false,
            hover_block: None,
            goal_x: None,
            row_index: RefCell::default(),
            block_rects: HashMap::new(),
            text_layouts: RefCell::new(HashMap::new()),
            caret_opacity: rune::Live::new(1.),
            caret_drawn_rect: rune::Live::new([0.; 4]),
            window_focused: true,
            caret_epoch: 0.,
            caret_selection: None,
            page_rect: None,
            viewport_rect: None,
            drag: None,
            slash: None,
            popover: None,
            copied_block: None,
            editing_cell: None,
            cell_inputs: HashMap::new(),
            cell_undo_started: false,
            table_selection: None,
            image_resize: None,
            table_resize: None,
            link_input: TextEdit::new(""),
        }
    }
}

impl DocSession {
    pub fn text_layout(&self, id: u64) -> Option<TextBoxMeta> {
        let layouts = self.text_layouts.borrow();
        let entry = layouts.get(&id)?;
        if std::rc::Rc::strong_count(entry) == 1 {
            return None;
        }
        let mut meta = entry.borrow().clone();
        meta.rect = meta.drawn_rect.get();
        if meta.max_width.is_some() {
            meta.max_width = Some((meta.rect[2] / meta.scale).max(1.));
        }
        Some(meta)
    }

    pub fn reset_caret_blink(&mut self, now: f64) {
        self.caret_epoch = now;
        self.caret_selection = Some(self.core.sel);
        self.caret_opacity.snap(1.);
    }

    pub fn poll_caret(&mut self, now: f64, enabled: bool, tx: &mut Tx) {
        let r = self.caret_drawn_rect.get();
        let in_view = self
            .viewport_rect
            .is_some_and(|v| r[3] > 0. && r[1] < v[1] + v[3] && r[1] + r[3] > v[1]);
        let active = enabled
            && self.window_focused
            && self.focused
            && self.editing_cell.is_none()
            && self.core.sel.is_collapsed()
            && self.core.block_sel.is_none()
            && in_view
            && self
                .text_layouts
                .borrow()
                .get(&self.core.sel.head.block)
                .is_some_and(|entry| std::rc::Rc::strong_count(entry) > 1);
        if !active {
            self.caret_selection = None;
            self.caret_opacity.snap(0.);
            return;
        }
        if self.caret_selection != Some(self.core.sel) {
            self.reset_caret_blink(now);
        }
        if self.core.preedit.is_some() || self.drag.is_some() {
            self.reset_caret_blink(now);
            return;
        }
        // A discrete timer updates only the caret's Live opacity, never the document view.
        const HALF_PERIOD: f64 = 0.53;
        let elapsed = (now - self.caret_epoch).max(0.);
        let phase = (elapsed / HALF_PERIOD).floor() as u64;
        self.caret_opacity
            .snap(if phase % 2 == 0 { 1. } else { 0. });
        let wait = HALF_PERIOD - elapsed % HALF_PERIOD;
        tx.wake_after(std::time::Duration::from_secs_f64(wait.max(0.001)));
    }

    pub fn load(&mut self, note_id: &str, doc: DocDocument, focus: bool) {
        self.note_id = note_id.to_owned();
        self.core = DocCore::new(doc);
        self.focused = focus;
        self.typing = false;
        self.hover_block = None;
        self.goal_x = None;
        self.block_rects.clear();
        self.text_layouts.get_mut().clear();
        self.caret_selection = None;
        self.drag = None;
        self.slash = None;
        self.popover = None;
        self.editing_cell = None;
        self.cell_undo_started = false;
        self.table_selection = None;
        self.table_resize = None;
        self.image_resize = None;
    }

    pub fn focus_title_end(&mut self) {
        let len = self.core.doc.title.len();
        self.core.sel = DocSel::caret(DocPos::new(TITLE_ID, len));
        self.core.clear_block_sel();
        self.focused = true;
        self.goal_x = None;
    }

    pub fn commit_cell_edit(&mut self) {
        self.flush_cell_edit();
        self.editing_cell = None;
        self.cell_undo_started = false;
    }

    pub fn flush_cell_edit(&mut self) {
        if let Some((bid, r, c_idx, key)) = &self.editing_cell {
            let ed = &self.cell_inputs[key];
            let (bid, r, c_idx) = (*bid, *r, *c_idx);
            let new_val = crate::doc_tables::cell_markdown(ed);
            let changed = self
                .core
                .block(bid)
                .and_then(|b| match &b.kind {
                    DocBlockKind::Table(t) => t.rows.get(r).and_then(|row| row.get(c_idx)),
                    _ => None,
                })
                .is_some_and(|old| {
                    if old == &new_val { return false; }
                    // Focusing a formatted cell must preserve its original Markdown spelling.
                    if old.contains('`') {
                        let previous = crate::doc_tables::cell_input(old);
                        return previous.text != ed.text || previous.marks() != ed.marks();
                    }
                    true
                });
            if changed {
                if !self.cell_undo_started {
                    self.core.with_undo(|_| {});
                    self.cell_undo_started = true;
                }
                if let Some(b) = self.core.block_mut(bid)
                    && let DocBlockKind::Table(t) = &mut b.kind
                    && let Some(row) = t.rows.get_mut(r)
                    && let Some(cell) = row.get_mut(c_idx)
                {
                    *cell = new_val;
                }
                self.core.dirty = true;
            }
        }
    }

    pub fn filtered_slash_cmds(&self) -> Vec<SlashCmd> {
        let Some(slash) = &self.slash else {
            return Vec::new();
        };
        crate::insert_menu::filtered(&slash.query)
    }

    pub fn caret_window_rect(&self) -> Option<[f32; 4]> {
        let (block, raw_off) = if let Some(pre) = &self.core.preedit {
            (pre.block, pre.offset)
        } else {
            (self.core.sel.head.block, self.core.sel.head.offset)
        };
        let meta = self.text_layout(block)?;
        let disp_off = if let Some(pre) = &self.core.preedit {
            if pre.block == block {
                let pe = pre
                    .cursor
                    .map(|(_, e)| e.min(pre.text.len()))
                    .unwrap_or(pre.text.len());
                meta.preedit_start + pe
            } else {
                meta.raw_to_display(raw_off)
            }
        } else if meta.is_placeholder {
            0
        } else {
            meta.raw_to_display(raw_off)
        };
        let local = rune::text::doc_text::caret_rect(
            &meta.display_text,
            &meta.spans,
            meta.style.clone(),
            meta.max_width,
            meta.align,
            disp_off,
        );
        Some([
            meta.rect[0] + local[0] * meta.scale,
            meta.rect[1] + local[1] * meta.scale,
            local[2].max(1.5) * meta.scale,
            local[3].max(style_lh(&meta.style)) * meta.scale,
        ])
    }

    pub fn selection_anchor_window_rect(&self) -> Option<[f32; 4]> {
        if self.core.sel.is_collapsed() {
            return None;
        }
        let (s, _) = self.core.ordered();
        let meta = self.text_layout(s.block)?;
        let local = rune::text::doc_text::caret_rect(
            &meta.display_text,
            &meta.spans,
            meta.style.clone(),
            meta.max_width,
            meta.align,
            meta.raw_to_display(s.offset),
        );
        Some([
            meta.rect[0] + local[0] * meta.scale,
            meta.rect[1] + local[1] * meta.scale,
            local[2].max(1.5) * meta.scale,
            local[3].max(style_lh(&meta.style)) * meta.scale,
        ])
    }

    /// Only the actual document tail can create a new paragraph; the last mounted
    /// virtual row may be thousands of blocks before it.
    pub fn is_below_last_block(&self, wy: f32) -> bool {
        let Some(last) = self.core.units().into_iter().rfind(|u| u.id != TITLE_ID) else {
            return false;
        };
        self.unit_rect(last.id)
            .is_some_and(|r| wy > r[1] + r[3] + 6.)
    }

    fn unit_rect(&self, id: u64) -> Option<[f32; 4]> {
        if let Some(meta) = self.text_layouts.borrow().get(&id)
            && std::rc::Rc::strong_count(meta) > 1
        {
            return Some(meta.borrow().drawn_rect.get());
        }
        self.block_rects.get(&id).copied()
    }

    pub fn hit_test_window(&self, wx: f32, wy: f32) -> DocPos {
        if let Some(meta) = self.text_layout(TITLE_ID) {
            let title_bottom = meta.rect[1] + meta.rect[3];
            if wy <= title_bottom + 4. {
                if meta.is_placeholder {
                    return DocPos::new(TITLE_ID, 0);
                }
                let off = meta.hit_test(
                    ((wx - meta.rect[0]) / meta.scale).max(0.),
                    ((wy - meta.rect[1]) / meta.scale)
                        .clamp(0., (meta.rect[3] / meta.scale).max(1.)),
                );
                return DocPos::new(TITLE_ID, meta.display_to_raw(off));
            }
        }

        let us = self.core.units();
        let mut best_id: Option<u64> = None;
        let mut best_dist = f32::MAX;
        for u in &us {
            if u.id == TITLE_ID {
                continue;
            }
            let Some(r) = self.unit_rect(u.id) else {
                continue;
            };
            if wy >= r[1] && wy <= r[1] + r[3] {
                best_id = Some(u.id);
                break;
            }
            let dist = if wy < r[1] {
                r[1] - wy
            } else {
                wy - (r[1] + r[3])
            };
            if dist < best_dist {
                best_dist = dist;
                best_id = Some(u.id);
            }
        }

        let Some(id) = best_id else {
            let first = us
                .iter()
                .find(|u| u.id != TITLE_ID)
                .map(|u| u.id)
                .unwrap_or(TITLE_ID);
            return DocPos::new(first, 0);
        };

        if let Some(meta) = self.text_layout(id) {
            if meta.is_placeholder {
                return DocPos::new(id, 0);
            }
            let lx = ((wx - meta.rect[0]) / meta.scale).max(0.);
            let ly =
                ((wy - meta.rect[1]) / meta.scale).clamp(0., (meta.rect[3] / meta.scale).max(1.));
            let off = meta.hit_test(lx, ly);
            return DocPos::new(id, meta.display_to_raw(off));
        }

        DocPos::new(id, 0)
    }

    fn move_vertical_2d(&mut self, down: bool, extend: bool) -> bool {
        let h = self.core.sel.head;
        let gx_win = self
            .goal_x
            .or_else(|| self.caret_window_rect().map(|r| r[0]));

        if let Some(meta) = self.text_layout(h.block) {
            let disp = meta.raw_to_display(h.offset);
            let local_gx = gx_win.map(|x| ((x - meta.rect[0]) / meta.scale).max(0.));
            if !meta.is_placeholder
                && let Some((next_disp, next_local_gx)) = rune::text::doc_text::move_vertical(
                    &meta.display_text,
                    &meta.spans,
                    meta.style.clone(),
                    meta.max_width,
                    meta.align,
                    disp,
                    local_gx,
                    down,
                )
            {
                let next_raw = meta.display_to_raw(next_disp);
                self.goal_x = Some(gx_win.unwrap_or(meta.rect[0] + next_local_gx * meta.scale));
                self.core.set_head(DocPos::new(h.block, next_raw), extend);
                return true;
            }
        }

        let us = self.core.units();
        let Some(idx) = us.iter().position(|u| u.id == h.block) else {
            return false;
        };
        let target = if down {
            us.get(idx + 1)
        } else {
            idx.checked_sub(1).and_then(|k| us.get(k))
        };
        let Some(target) = target else {
            return false;
        };

        let next_pos = if let Some(meta) = self.text_layout(target.id) {
            if meta.is_placeholder {
                DocPos::new(target.id, 0)
            } else {
                let target_x = gx_win
                    .map(|x| ((x - meta.rect[0]) / meta.scale).max(0.))
                    .unwrap_or(0.);
                let target_y = if down {
                    2.0
                } else {
                    (meta.rect[3] / meta.scale - 2.0).max(0.0)
                };
                let off = meta.hit_test(target_x, target_y);
                DocPos::new(target.id, meta.display_to_raw(off))
            }
        } else {
            let off = if down { 0 } else { self.core.len_of(target.id) };
            DocPos::new(target.id, off)
        };
        self.goal_x = gx_win;
        self.core.set_head(next_pos, extend);
        true
    }

    fn move_line_edge(&mut self, end: bool, extend: bool) {
        let h = self.core.sel.head;
        self.goal_x = None;
        if let Some(meta) = self.text_layout(h.block) {
            if meta.is_placeholder {
                self.core.set_head(DocPos::new(h.block, 0), extend);
                return;
            }
            let disp = meta.raw_to_display(h.offset);
            let edge_disp = rune::text::doc_text::line_edge(
                &meta.display_text,
                &meta.spans,
                meta.style.clone(),
                meta.max_width,
                meta.align,
                disp,
                end,
            );
            let raw = meta.display_to_raw(edge_disp);
            self.core.set_head(DocPos::new(h.block, raw), extend);
        } else {
            let off = if end { self.core.len_of(h.block) } else { 0 };
            self.core.set_head(DocPos::new(h.block, off), extend);
        }
    }

    fn refresh_slash_query(&mut self) {
        let Some(mut slash) = self.slash.take() else {
            return;
        };
        let p = self.core.sel.head;
        if !self.core.sel.is_collapsed()
            || p.block != slash.block
            || p.offset <= slash.trigger_offset
        {
            return;
        }
        let Some(txt) = self.core.text_of(p.block).map(|t| t.text.as_str()) else {
            return;
        };
        let Some(ch) = txt
            .get(slash.trigger_offset..)
            .and_then(|s| s.chars().next())
        else {
            return;
        };
        if ch != slash.trigger {
            return;
        }
        let query_start = slash.trigger_offset + ch.len_utf8();
        if p.offset < query_start || p.offset > txt.len() {
            return;
        }
        let q = &txt[query_start..p.offset];
        if q.contains('\n') || q.contains(' ') || q.chars().count() > 24 {
            return;
        }
        if slash.query != q {
            slash.selected = 0;
        }
        slash.query = q.to_owned();
        let lq = slash.query.trim().to_ascii_lowercase();
        let len = SLASH_CMDS.iter().filter(|cmd| cmd.matches(&lq)).count();
        slash.selected = if len == 0 {
            0
        } else {
            slash.selected.min(len - 1)
        };
        self.slash = Some(slash);
    }

    fn maybe_open_slash(&mut self, typed: &str) {
        if typed != "/" && typed != "、" {
            return;
        }
        if !self.core.sel.is_collapsed() {
            return;
        }
        let p = self.core.sel.head;
        if p.block == TITLE_ID || self.core.is_code(p.block) {
            return;
        }
        let trigger = typed.chars().next().unwrap();
        let trigger_offset = p.offset.saturating_sub(typed.len());
        let ok_prefix = self
            .core
            .text_of(p.block)
            .map(|t| {
                trigger_offset == 0
                    || t.text[..trigger_offset]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_whitespace() || matches!(c, '，' | '。' | ',' | '.'))
            })
            .unwrap_or(false);
        if ok_prefix {
            self.slash = Some(DocSlash {
                block: p.block,
                trigger_offset,
                trigger,
                query: String::new(),
                selected: 0,
            });
        }
    }
}

pub fn sync_to_state(s: &mut State) {
    if s.mode == crate::model::WorkspaceMode::Canvas {
        crate::canvas_document::sync(s);
        return;
    }
    if !s.editor.active.note_id.is_empty() && s.editor.active.core.dirty {
        let nid = s.editor.active.note_id.clone();
        let raw_title = s.editor.active.core.doc.title.text.trim().to_owned();
        let display_title = if raw_title.is_empty() {
            s.tr("无标题文档").to_owned()
        } else {
            raw_title
        };
        let preview = s.editor.active.core.doc.preview_text();
        let preview = if preview.is_empty() {
            s.tr("开始输入正文…").to_owned()
        } else {
            preview
        };
        let doc_clone = s.editor.active.core.doc.clone();
        let dirty = s.editor.active.core.dirty;
        if let Some(n) = s.notes.iter_mut().find(|n| n.id == nid) {
            n.title = display_title;
            n.preview = preview;
            n.doc = Some(doc_clone);
            if dirty {
                n.updated = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                n.stamp = "刚刚".into();
            }
        }
        if dirty {
            s.queue_document_save();
            s.editor.active.core.dirty = false;
        }
    }
}

pub fn execute_slash_cmd(s: &mut State, cmd_id: &str, tx: &mut Tx) {
    let slash = s.editor.active.slash.take();
    let (target_block, remove_range) = if let Some(sl) = slash {
        let end = s.editor.active.core.sel.head.offset.max(sl.trigger_offset);
        (sl.block, Some(sl.trigger_offset..end))
    } else {
        (s.editor.active.core.sel.head.block, None)
    };

    if cmd_id == "image" {
        if let Some(range) = remove_range {
            s.editor.active.core.with_undo(|core| {
                if let Some(t) = core.text_mut(target_block) {
                    let b = clamp_utf8(&t.text, range.start);
                    let e = clamp_utf8(&t.text, range.end);
                    t.replace(b..e, &RichText::default());
                    core.sel = DocSel::caret(DocPos::new(target_block, b));
                }
            });
        }
        sync_to_state(s);
        let options = FileDialogOptions {
            title: s.tr("选择图片").into(),
            ..Default::default()
        }
        .filter(s.tr("图片"), &["png", "jpg", "jpeg", "webp"]);
        let note_id = s.current.clone();
        let owner = s.editor.canvas_owner().map(str::to_owned);
        tx.open_file(options, move |s: &mut State, result, _| {
            if s.current != note_id {
                return;
            }
            if let Some(owner) = &owner {
                crate::canvas_document::activate(s, owner);
            }
            match result {
                Ok(Some(path)) => match std::fs::read(&path) {
                    Ok(bytes) => {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                        insert_image(s, target_block, &bytes, ext, "无法导入图片");
                    }
                    Err(e) => s.message = Some(format!("{}: {e}", s.tr("无法读取图片"))),
                },
                Err(e) => s.message = Some(format!("{}: {e}", s.tr("无法打开文件选择"))),
                _ => {}
            }
        });
        return;
    }

    s.editor.active.core.with_undo(|core| {
        if let Some(range) = remove_range
            && let Some(t) = core.text_mut(target_block)
        {
            let b = clamp_utf8(&t.text, range.start);
            let e = clamp_utf8(&t.text, range.end);
            t.replace(b..e, &RichText::default());
            core.sel = DocSel::caret(DocPos::new(target_block, b));
        }

        let is_empty_text = target_block != TITLE_ID
            && core.block(target_block).is_some_and(|b| {
                matches!(b.kind, DocBlockKind::Text) && b.text.is_empty() && b.children.is_empty()
            });

        let replace_or_after = |core: &mut DocCore, kind: DocBlockKind| {
            if is_empty_text {
                if matches!(kind, DocBlockKind::Divider | DocBlockKind::Table(_)) {
                    core.turn_into(&[target_block], DocBlockKind::Text);
                    if let Some(b) = core.block_mut(target_block) {
                        b.kind = kind;
                        b.text = RichText::default();
                    }
                    let next = core.doc.new_block(DocBlockKind::Text, "");
                    let nid = core.insert_block_after(target_block, next);
                    core.sel = DocSel::caret(DocPos::new(nid, 0));
                } else {
                    core.turn_into(&[target_block], kind);
                    core.sel = DocSel::caret(DocPos::new(target_block, 0));
                }
            } else if kind.is_container() {
                let child = core.doc.new_block(DocBlockKind::Text, "");
                let cid = child.id;
                let container = core.doc.new_container(kind, child);
                core.insert_block_after(target_block, container);
                core.sel = DocSel::caret(DocPos::new(cid, 0));
            } else if matches!(kind, DocBlockKind::Divider | DocBlockKind::Table(_)) {
                let nb = core.doc.new_block(kind, "");
                let after = core.insert_block_after(target_block, nb);
                let next = core.doc.new_block(DocBlockKind::Text, "");
                let nid = core.insert_block_after(after, next);
                core.sel = DocSel::caret(DocPos::new(nid, 0));
            } else {
                let nb = core.doc.new_block(kind, "");
                let nid = core.insert_block_after(target_block, nb);
                core.sel = DocSel::caret(DocPos::new(nid, 0));
            }
        };

        match cmd_id {
            "text" => {
                core.turn_into(&[target_block], DocBlockKind::Text);
            }
            "h1" => replace_or_after(core, DocBlockKind::Heading { level: 1 }),
            "h2" => replace_or_after(core, DocBlockKind::Heading { level: 2 }),
            "h3" => replace_or_after(core, DocBlockKind::Heading { level: 3 }),
            "bullet" => replace_or_after(core, DocBlockKind::Bullet),
            "ordered" => replace_or_after(core, DocBlockKind::Ordered { start: None }),
            "todo" => replace_or_after(core, DocBlockKind::Todo { done: false }),
            "quote" => {
                if is_empty_text {
                    core.turn_into(&[target_block], DocBlockKind::Quote);
                } else {
                    replace_or_after(core, DocBlockKind::Quote);
                }
            }
            "callout" => {
                let kind = DocBlockKind::Callout {
                    emoji: "💡".into(),
                    palette: "orange".into(),
                };
                if is_empty_text {
                    core.turn_into(&[target_block], kind);
                } else {
                    replace_or_after(core, kind);
                }
            }
            "mermaid" | "math" => replace_or_after(core, DocBlockKind::Code {
                lang: cmd_id.into(), wrap: true,
            }),
            "code" => replace_or_after(
                core,
                DocBlockKind::Code {
                    lang: "Plain Text".into(),
                    wrap: true,
                },
            ),
            "divider" => replace_or_after(core, DocBlockKind::Divider),
            "table" => replace_or_after(core, DocBlockKind::Table(Default::default())),
            "bold" => core.toggle_mark(DocMark::Bold),
            "italic" => core.toggle_mark(DocMark::Italic),
            "underline" => core.toggle_mark(DocMark::Underline),
            "strike" => core.toggle_mark(DocMark::Strike),
            "inline_code" => core.toggle_mark(DocMark::Code),
            _ => {}
        }
    });
    s.editor.active.focused = true;
    s.editor.active.goal_x = None;
    sync_to_state(s);
    tx.animated();
}

fn reveal_document_row(core: &DocCore, block: u64, tx: &mut Tx) {
    // Container descendants are mounted with their outer virtual row.
    let mut row = block;
    let mut ancestor = block;
    while let Some(parent) = core.parent_of(ancestor) {
        ancestor = parent.id;
        if parent.kind.is_container() {
            row = ancestor;
        }
    }
    tx.scroll_into_view(if row == TITLE_ID {
        "doc-header".to_owned()
    } else {
        format!("doc-blk-{row}")
    });
}

pub fn follow_outline(s: &mut State, heading: u64, tx: &mut Tx) {
    tx.blur();
    let session = &mut s.editor.active;
    session.commit_cell_edit();
    session.table_selection = None;
    session.popover = None;
    session.slash = None;
    session.goal_x = None;
    session.focused = true;
    session.typing = false;
    session.core.go_to_heading(heading);
    session.reset_caret_blink(s.clock);
    reveal_document_row(&session.core, heading, tx);
    reveal_text_caret(s, tx);
    sync_to_state(s);
}

fn reveal_text_caret(s: &State, tx: &mut Tx) {
    let session = &s.editor.active;
    if session.focused
        && session.editing_cell.is_none()
        && (session.core.preedit.is_some() || session.core.sel.is_collapsed())
        && session.core.text_of(session.core.sel.head.block).is_some()
    {
        tx.scroll_into_view_in(s.scroll_key(), "doc-caret");
    }
}

pub fn on_ime(s: &mut State, ev: &Ime, tx: &mut Tx) -> bool {
    if !crate::canvas_document::enabled(s) || !s.editor.active.focused {
        return false;
    }
    if s.editor.active.editing_cell.is_some() {
        return false;
    }
    s.editor.active.reset_caret_blink(s.clock);
    let overlays_before = (
        s.editor.active.slash.is_some(),
        !s.editor.active.core.sel.is_collapsed(),
    );
    match ev {
        Ime::Preedit(text, cursor) => {
            if text.is_empty() {
                if s.editor.active.core.preedit.take().is_none() {
                    tx.live_only();
                    return true;
                }
            } else {
                let core = &s.editor.active.core;
                let p = core.ordered().0;
                if s.editor.active.typing && core.preedit.as_ref().is_some_and(|previous| {
                    previous.block == p.block && previous.offset == p.offset
                        && previous.text == *text && previous.cursor == *cursor
                }) {
                    tx.live_only();
                    return true;
                }
                s.editor.active.core.preedit = Some(DocPreedit {
                    block: p.block,
                    offset: p.offset,
                    text: text.clone(),
                    cursor: *cursor,
                });
                s.editor.active.typing = true;
            }
            reveal_text_caret(s, tx);
            if overlays_before
                != (
                    s.editor.active.slash.is_some(),
                    !s.editor.active.core.sel.is_collapsed(),
                )
            {
                tx.animated();
            } else {
                tx.instant();
            }
            true
        }
        Ime::Commit(text) => {
            s.editor.active.core.preedit = None;
            if !text.is_empty() {
                if !s.editor.active.core.sel.is_collapsed() || s.editor.active.core.in_block_mode() {
                    s.editor.active.core.break_coalescing();
                }
                s.editor.active.core.insert_text(text);
                s.editor.active.typing = true;
                s.editor.active.goal_x = None;
                s.editor.active.maybe_open_slash(text);
                s.editor.active.refresh_slash_query();
                sync_to_state(s);
            }
            reveal_text_caret(s, tx);
            if overlays_before
                != (
                    s.editor.active.slash.is_some(),
                    !s.editor.active.core.sel.is_collapsed(),
                )
            {
                tx.animated();
            } else {
                tx.instant();
            }
            true
        }
        Ime::Disabled => {
            if s.editor.active.core.preedit.take().is_some() {
                tx.instant();
            } else {
                tx.live_only();
            }
            true
        }
        Ime::Enabled => {
            tx.live_only();
            true
        }
    }
}

fn insert_image(s: &mut State, target: u64, bytes: &[u8], ext: &str, error_label: &'static str) {
    let src = match s.save_doc_image_bytes(bytes, ext) {
        Ok(src) => src,
        Err(error) => {
            s.message = Some(format!("{}: {error}", s.tr(error_label)));
            return;
        }
    };
    s.editor.active.core.with_undo(|core| {
        let image = core.doc.new_block(DocBlockKind::Image { src, width: None, caption: String::new() }, "");
        let image_id = core.insert_block_after(target, image);
        let next = core.doc.new_block(DocBlockKind::Text, "");
        let next_id = core.insert_block_after(image_id, next);
        core.sel = DocSel::caret(DocPos::new(next_id, 0));
    });
    sync_to_state(s);
}

pub fn on_paste(s: &mut State, content: &ClipboardContent, tx: &mut Tx) {
    if !crate::canvas_document::enabled(s) || !s.editor.active.focused {
        return;
    }
    if s.editor.active.editing_cell.is_some() {
        return;
    }
    if let Some(sel) = s.editor.active.table_selection {
        crate::doc_tables::paste(
            s,
            sel.block,
            sel.anchor.0.min(sel.head.0),
            sel.anchor.1.min(sel.head.1),
            content.text(),
        );
        return;
    }
    if let Some(img) = content.image() {
        insert_image(s, s.editor.active.core.sel.head.block, img.png(), "png", "无法粘贴图片");
        return;
    }
    let txt = content.text();
    if !txt.is_empty() {
        if s.editor
            .active
            .core
            .is_code(s.editor.active.core.sel.head.block)
            || s.editor.active.core.sel.head.block == TITLE_ID
        {
            if let Err(error) = s.editor.active.core.paste_markdown_or_text(txt) {
                s.message = Some(format!("无法粘贴：{error}"));
                return;
            }
        } else if let Some(html) = content.html() {
            s.editor
                .active
                .core
                .paste_native(&TextEdit::from_html(html));
        } else {
            if let Err(error) = s.editor.active.core.paste_markdown_or_text(txt) {
                s.message = Some(format!("无法粘贴：{error}"));
                return;
            }
        }
        s.editor.active.typing = true;
        s.editor.active.goal_x = None;
        s.editor.active.slash = None;
        sync_to_state(s);
        reveal_text_caret(s, tx);
    }
}

pub fn key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    let before_pos = s.editor.active.core.sel.head;
    let overlays_before = (
        s.editor.active.slash.is_some(),
        s.editor.active.popover.is_some(),
        !s.editor.active.core.sel.is_collapsed(),
    );
    let handled = key_inner(s, e, tx);
    let overlays_after = (
        s.editor.active.slash.is_some(),
        s.editor.active.popover.is_some(),
        !s.editor.active.core.sel.is_collapsed(),
    );
    if handled && overlays_before != overlays_after {
        tx.animated();
    }
    if handled {
        s.editor.active.reset_caret_blink(s.clock);
    }
    let head = s.editor.active.core.sel.head.block;
    if handled && head != before_pos.block && s.mode == crate::model::WorkspaceMode::Document {
        reveal_document_row(&s.editor.active.core, head, tx);
    }
    if handled && (e.key == Key::Enter || s.editor.active.core.sel.head != before_pos) {
        reveal_text_caret(s, tx);
    }
    handled
}

fn key_inner(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if !crate::canvas_document::enabled(s) {
        return false;
    }
    if crate::doc_tables::key(s, e, tx) {
        return true;
    }
    if s.editor.active.editing_cell.is_some() {
        return false;
    }
    if matches!(s.editor.active.popover, Some(DocPopover::LinkMenu)) {
        if e.key == Key::Escape {
            s.editor.active.popover = None;
            tx.blur();
            return true;
        }
        if e.key == Key::Enter {
            apply_link(s, tx);
            return true;
        }
    }
    if tx.focused().is_some() || !s.editor.active.focused {
        return false;
    }

    if s.editor.active.core.preedit.is_some() {
        if e.key == Key::Escape {
            s.editor.active.core.preedit = None;
            return true;
        }
        return false;
    }

    if s.editor.active.popover.is_some() && e.key == Key::Escape {
        s.editor.active.popover = None;
        return true;
    }

    if s.editor.active.slash.is_some() {
        let cmds = s.editor.active.filtered_slash_cmds();
        let len = cmds.len().max(1);
        match e.key {
            Key::Up => {
                if !cmds.is_empty() {
                    let next = s
                        .editor
                        .active
                        .slash
                        .as_ref()
                        .map_or(0, |sl| sl.selected.checked_sub(1).unwrap_or(len - 1));
                    if let Some(slash) = s.editor.active.slash.as_mut() {
                        slash.selected = next;
                    }
                    if let Some(cmd) = cmds.get(next) {
                        tx.scroll_into_view(format!("slash-item-{}", cmd.id));
                    }
                }
                tx.instant();
                return true;
            }
            Key::Down => {
                if !cmds.is_empty() {
                    let next = s
                        .editor
                        .active
                        .slash
                        .as_ref()
                        .map_or(0, |sl| (sl.selected + 1) % len);
                    if let Some(slash) = s.editor.active.slash.as_mut() {
                        slash.selected = next;
                    }
                    if let Some(cmd) = cmds.get(next) {
                        tx.scroll_into_view(format!("slash-item-{}", cmd.id));
                    }
                }
                tx.instant();
                return true;
            }
            Key::Enter | Key::Tab if !e.mods.shift => {
                let idx = s.editor.active.slash.as_ref().map_or(0, |sl| sl.selected);
                let picked = cmds.get(idx).map(|c| c.id);
                if let Some(id) = picked {
                    execute_slash_cmd(s, id, tx);
                    return true;
                }
                s.editor.active.slash = None;
                tx.instant();
                if matches!(e.key, Key::Tab) {
                    return true;
                }
            }
            Key::Escape => {
                s.editor.active.slash = None;
                tx.instant();
                return true;
            }
            _ => {}
        }
    }

    if e.is_command('z') {
        if !e.mods.shift && !s.editor.active.core.can_undo() {
            return false;
        }
        let changed = if e.mods.shift {
            s.editor.active.core.redo()
        } else {
            s.editor.active.core.undo()
        };
        if changed {
            s.editor.active.goal_x = None;
            s.editor.active.slash = None;
            sync_to_state(s);
            tx.instant();
        }
        return true;
    }
    if e.is_command('y') {
        if s.editor.active.core.redo() {
            s.editor.active.goal_x = None;
            sync_to_state(s);
            tx.instant();
        }
        return true;
    }
    if e.is_command('a') {
        s.editor.active.core.select_all();
        s.editor.active.goal_x = None;
        s.editor.active.slash = None;
        tx.instant();
        return true;
    }
    if e.is_command('b') {
        s.editor.active.core.toggle_mark(DocMark::Bold);
        sync_to_state(s);
        tx.instant();
        return true;
    }
    if e.is_command('i') {
        s.editor.active.core.toggle_mark(DocMark::Italic);
        sync_to_state(s);
        tx.instant();
        return true;
    }
    if e.is_command('u') {
        s.editor.active.core.toggle_mark(DocMark::Underline);
        sync_to_state(s);
        tx.instant();
        return true;
    }
    if e.is_command('x') && e.mods.shift {
        s.editor.active.core.toggle_mark(DocMark::Strike);
        sync_to_state(s);
        tx.instant();
        return true;
    }
    if e.is_command('e') {
        s.editor.active.core.toggle_mark(DocMark::Code);
        sync_to_state(s);
        tx.instant();
        return true;
    }
    if e.is_command('c') {
        let txt = s.editor.active.core.selected_text();
        if !txt.is_empty() {
            tx.copy_rich(s.editor.active.core.clipboard_content());
        }
        return true;
    }
    if e.is_command('x') {
        let txt = s.editor.active.core.selected_text();
        if !txt.is_empty() {
            tx.copy_rich(s.editor.active.core.clipboard_content());
            s.editor.active.core.delete_selection();
            sync_to_state(s);
            tx.instant();
        }
        return true;
    }
    if e.is_command('v') {
        tx.request_paste();
        return true;
    }
    if e.is_command('d') {
        let bid = s.editor.active.core.sel.head.block;
        if let Some(nid) = s.editor.active.core.duplicate_block(bid) {
            s.editor.active.core.sel = DocSel::caret(DocPos::new(nid, 0));
            sync_to_state(s);
            tx.instant();
        }
        return true;
    }

    match &e.key {
        Key::Escape => {
            if !s.editor.active.core.sel.is_collapsed() || s.editor.active.core.in_block_mode() {
                let p = s.editor.active.core.sel.head;
                s.editor.active.core.sel = DocSel::caret(p);
                s.editor.active.core.clear_block_sel();
            } else {
                s.editor.active.focused = false;
            }
            s.editor.active.popover = None;
            s.editor.active.slash = None;
            tx.instant();
            true
        }
        Key::Enter => {
            s.editor.active.typing = true;
            s.editor.active.goal_x = None;
            s.editor.active.slash = None;
            if e.mods.cmd {
                s.editor.active.core.exit_block_below();
            } else if e.mods.shift {
                s.editor.active.core.soft_break();
            } else {
                s.editor.active.core.enter();
            }
            sync_to_state(s);
            tx.instant();
            true
        }
        Key::Backspace => {
            if e.mods.cmd {
                return false;
            }
            s.editor.active.typing = true;
            s.editor.active.goal_x = None;
            s.editor.active.core.backspace();
            s.editor.active.refresh_slash_query();
            sync_to_state(s);
            tx.instant();
            true
        }
        Key::Delete => {
            s.editor.active.typing = true;
            s.editor.active.goal_x = None;
            s.editor.active.core.delete_forward();
            s.editor.active.refresh_slash_query();
            sync_to_state(s);
            tx.instant();
            true
        }
        Key::Tab => {
            let p = s.editor.active.core.sel.head;
            if s.editor.active.core.is_code(p.block) {
                s.editor.active.core.insert_text("    ");
            } else if e.mods.shift {
                s.editor.active.core.outdent_one(p.block);
            } else {
                s.editor.active.core.indent_block(p.block);
            }
            s.editor.active.goal_x = None;
            sync_to_state(s);
            tx.instant();
            true
        }
        Key::Left => {
            s.editor.active.slash = None;
            if e.mods.cmd {
                s.editor.active.move_line_edge(false, e.mods.shift);
            } else {
                s.editor.active.goal_x = None;
                s.editor.active.core.move_h(false, e.mods.alt, e.mods.shift);
            }
            tx.instant();
            true
        }
        Key::Right => {
            s.editor.active.slash = None;
            if e.mods.cmd {
                s.editor.active.move_line_edge(true, e.mods.shift);
            } else {
                s.editor.active.goal_x = None;
                s.editor.active.core.move_h(true, e.mods.alt, e.mods.shift);
            }
            tx.instant();
            true
        }
        Key::Up => {
            s.editor.active.slash = None;
            if e.mods.cmd {
                s.editor.active.core.set_head(
                    DocPos::new(
                        if s.editor.active.core.include_title {
                            TITLE_ID
                        } else {
                            s.editor.active.core.units()[0].id
                        },
                        0,
                    ),
                    e.mods.shift,
                );
                s.editor.active.goal_x = None;
            } else {
                s.editor.active.move_vertical_2d(false, e.mods.shift);
            }
            tx.instant();
            true
        }
        Key::Down => {
            s.editor.active.slash = None;
            if e.mods.cmd {
                let us = s.editor.active.core.units();
                if let Some(last) = us.last() {
                    let end = s.editor.active.core.len_of(last.id);
                    s.editor
                        .active
                        .core
                        .set_head(DocPos::new(last.id, end), e.mods.shift);
                }
                s.editor.active.goal_x = None;
            } else {
                s.editor.active.move_vertical_2d(true, e.mods.shift);
            }
            tx.instant();
            true
        }
        Key::Home => {
            s.editor.active.move_line_edge(false, e.mods.shift);
            tx.instant();
            true
        }
        Key::End => {
            s.editor.active.move_line_edge(true, e.mods.shift);
            tx.instant();
            true
        }
        Key::Text(txt) if !e.mods.cmd && !e.mods.ctrl => {
            let clean: String = txt
                .chars()
                .filter(|&c| !c.is_control() && c != '\n' && c != '\r' && c != '\t')
                .collect();
            if clean.is_empty() {
                return false;
            }
            s.editor.active.typing = true;
            s.editor.active.goal_x = None;
            s.editor.active.core.insert_text(&clean);
            s.editor.active.maybe_open_slash(&clean);
            s.editor.active.refresh_slash_query();
            sync_to_state(s);
            tx.instant();
            true
        }
        _ => false,
    }
}

fn extend_drag_selection(session: &mut DocSession, point: [f32; 2]) {
    let Some([_, top, _, height]) = session.viewport_rect else {
        return;
    };
    if height <= 2. {
        return;
    }
    let head = session.hit_test_window(point[0], point[1].clamp(top + 1., top + height - 1.));
    session.core.set_head(head, true);
}

fn scroll_drag_selection(s: &State, d: &mut rune::Drag, tx: &mut Tx) {
    let Some([_, top, _, height]) = s.editor.active.viewport_rect else {
        return;
    };
    if height <= 2. || !s.editor.active.window_focused {
        return;
    }
    let dt = (s.clock as f32 - d.data).clamp(0., 0.05);
    d.data = s.clock as f32;
    let edge = 36_f32.min(height * 0.25);
    let y = (d.window_point()[1] - top).clamp(0., height);
    let speed = 840. * ((y - (height - edge)).max(0.) - (edge - y).max(0.)) / edge;
    let scroll = s.scroll_pos.get();
    let [x, current] = scroll.offset;
    if (speed < 0. && current > 0.) || (speed > 0. && current < scroll.max[1]) {
        tx.scroll_to_instant(
            s.scroll_key(),
            x,
            (current + speed * dt).clamp(0., scroll.max[1]),
        );
        tx.drag_repeat_after(std::time::Duration::from_millis(16));
    }
}

fn handle_doc_stage_drag(s: &mut State, d: &mut rune::Drag, tx: &mut Tx) {
    let before = (
        s.editor.active.popover.is_some(),
        s.editor.active.slash.is_some(),
        !s.editor.active.core.sel.is_collapsed() && s.editor.active.drag.is_none(),
    );
    handle_doc_stage_drag_inner(s, d, tx);
    let after = (
        s.editor.active.popover.is_some(),
        s.editor.active.slash.is_some(),
        !s.editor.active.core.sel.is_collapsed() && s.editor.active.drag.is_none(),
    );
    if before != after {
        tx.animated();
    }
}
fn handle_doc_stage_drag_inner(s: &mut State, d: &mut rune::Drag, tx: &mut Tx) {
    s.editor.active.reset_caret_blink(s.clock);
    match d.phase {
        Phase::Down => {
            d.data = s.clock as f32;
            tx.blur();
            s.editor.active.commit_cell_edit();
            s.editor.active.table_selection = None;
            s.editor.active.focused = true;
            s.editor.active.typing = false;
            s.editor.active.popover = None;
            s.editor.active.slash = None;
            let pt = d.window_point();
            if d.clicks == 1 && s.editor.active.is_below_last_block(pt[1]) {
                let pos = s.editor.active.core.ensure_trailing_empty_paragraph();
                sync_to_state(s);
                s.editor.active.core.clear_block_sel();
                s.editor.active.core.sel = DocSel::caret(pos);
                s.editor.active.core.pending_marks = None;
                s.editor.active.core.preedit = None;
                s.editor.active.goal_x = None;
                s.editor.active.drag = Some(DocDrag::Selecting { moved: false });
                tx.instant();
                return;
            }
            let pos = s.editor.active.hit_test_window(pt[0], pt[1]);
            if d.clicks == 2 {
                let rng = s
                    .editor
                    .active
                    .core
                    .text_of(pos.block)
                    .map(|t| word_at(&t.text, pos.offset))
                    .unwrap_or(0..0);
                s.editor.active.core.clear_block_sel();
                s.editor.active.core.sel = DocSel {
                    anchor: DocPos::new(pos.block, rng.start),
                    head: DocPos::new(pos.block, rng.end),
                };
                s.editor.active.drag = Some(DocDrag::Selecting { moved: false });
            } else if d.clicks >= 3 {
                let len = s.editor.active.core.len_of(pos.block);
                if s.editor.active.core.text_of(pos.block).is_some() {
                    s.editor.active.core.clear_block_sel();
                    s.editor.active.core.sel = DocSel {
                        anchor: DocPos::new(pos.block, 0),
                        head: DocPos::new(pos.block, len),
                    };
                } else {
                    s.editor.active.core.block_sel = Some((pos.block, pos.block));
                }
                s.editor.active.drag = Some(DocDrag::Selecting { moved: false });
            } else {
                s.editor.active.core.clear_block_sel();
                s.editor.active.core.sel = DocSel::caret(pos);
                s.editor.active.core.pending_marks = None;
                s.editor.active.core.preedit = None;
                s.editor.active.goal_x = None;
                s.editor.active.drag = Some(DocDrag::Selecting { moved: false });
            }
            tx.instant();
        }
        Phase::Move => {
            if let Some(DocDrag::Selecting { moved }) = &mut s.editor.active.drag
                && (*moved || d.dx.abs() > 2. || d.dy.abs() > 2.)
            {
                *moved = true;
                extend_drag_selection(&mut s.editor.active, d.window_point());
                scroll_drag_selection(s, d, tx);
                tx.instant();
            }
        }
        Phase::Up => {
            if let Some(DocDrag::Selecting { moved }) = s.editor.active.drag {
                if moved && !d.cancelled() {
                    extend_drag_selection(&mut s.editor.active, d.window_point());
                }
                s.editor.active.drag = None;
                s.editor.active.goal_x = None;
                tx.instant();
            }
        }
    }
}

pub fn render_editor(s: &State, note: &Note, width: f32, viewport_width: f32, height: f32) -> El {
    let available = (width - 144.).max(96.);
    let doc_col_w = s
        .editor
        .active
        .core
        .doc
        .page_width
        .max_px()
        .map_or(available, |max| available.min(max));
    let margin = ((width - doc_col_w) * 0.5).round();

    let mut header = col()
        .w(doc_col_w)
        .pt(36.)
        .child(render_title(&render_state(s), doc_col_w));

    let subdocs: Vec<&Note> = crate::page_tree::children_of(s, &note.notebook, Some(&note.id));
    if !subdocs.is_empty() {
        let mut chips = row().w_full().gap(6.).pt(6.).pb(8.).center_y();
        for child in subdocs.into_iter().take(6) {
            let cid = child.id.clone();
            chips = chips.child(
                row()
                    .key(format!("subdoc-chip-{cid}"))
                    .h(26.)
                    .px(9.)
                    .gap(5.)
                    .radius(6.)
                    .center_y()
                    .fill(c(crate::ui::editor_colors(s.theme()).inset))
                    .hover(c(s
                        .theme()
                        .choose(0xeef0f3, crate::ui::editor_colors(s.theme()).inset)))
                    .border(1., c(s.theme().choose(0x1f2329, 0xe6e7eb)).alpha(0.06))
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut State, tx| {
                        tx.blur();
                        s.select_note(&cid);
                        tx.animated();
                    })
                    .child(
                        s.notebook_icon_style.page_icon(
                            child.format, 12.5, s.theme().choose(0x646a73, 0xb3b6bf),
                        ),
                    )
                    .child(
                        label(
                            &child.title,
                            12.5,
                            480.,
                            s.theme().choose(0x373c43, 0xe6e7eb),
                        )
                        .truncate(),
                    ),
            );
        }
        header = header.child(chips);
    }

    // Rune's index-built rows create layout/text nodes only around the viewport.
    // Paths identify the source sibling list without searching all preceding blocks.
    let (rows, version) = s.editor.active.row_index.borrow_mut().rows(&s.editor.active.core.doc.blocks);
    let count = rows.len();
    let keys = rows.clone();
    let builders = rows.clone();
    let visible = rows.clone();
    let page_col = track_rect(
        col()
            .key("doc-page-column")
            .w(width)
            .px(margin)
            .cursor(Cursor::Text)
            .on_drag(handle_doc_stage_drag)
            .growth(GrowthSpec {
                spring: Spring::NONE,
                ..Default::default()
            })
            .virtual_rows(
                count + 2,
                96.,
                move |_: &State, i| {
                    if i == 0 {
                        "doc-header".into()
                    } else if i == count + 1 {
                        "doc-end".into()
                    } else {
                        keys[i - 1].key.clone().into()
                    }
                },
                move |s: &State, i| {
                    if i == 0 {
                        return header.clone();
                    }
                    if i == count + 1 {
                        return el().h(220.);
                    }
                    let row = &builders[i - 1];
                    let (siblings, idx) = row.resolve(&s.editor.active.core.doc.blocks);
                    render_block_node(
                        &render_state(s),
                        &siblings[idx],
                        row.list,
                        row.path.len() - 1,
                        doc_col_w,
                        false,
                    )
                },
            )
            .virtual_rows_version((&note.id, version))
            .on_visible_rows(move |s: &mut State, range| {
                let mut ids = std::collections::HashSet::new();
                if range.contains(&0) {
                    ids.insert(TITLE_ID);
                }
                for i in range {
                    if i == 0 || i > count {
                        continue;
                    }
                    let (list, idx) = visible[i - 1].resolve(&s.editor.active.core.doc.blocks);
                    collect_block_ids(&list[idx], &mut ids);
                }
                s.editor.active.block_rects.retain(|id, _| ids.contains(id));
                // Visible range excludes mounted overscan. Keep geometry for every live row;
                // the row layout handler owns the second reference until it is unmounted.
                s.editor
                    .active
                    .text_layouts
                    .get_mut()
                    .retain(|_, meta| std::rc::Rc::strong_count(meta) > 1);
                false
            }),
        |s, r| {
            s.editor.active.page_rect = Some(r);
        },
    );

    let scroll_pos = s.scroll_pos.clone();
    let doc_scroll = col()
        .key(s.scroll_key())
        .size(viewport_width, height)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .on_layout_tx(|s: &mut State, r, tx| {
            s.editor.active.viewport_rect = Some(r);
            tx.live_only();
        })
        .on_scroll_offset(move |pos| scroll_pos.set(pos))
        .on_pointer_move_local(|s: &mut State, p, tx| {
            let was_typing = s.editor.active.typing;
            let prev_hover = s.editor.active.hover_block;
            let locked = s.editor.active.drag.is_some()
                || matches!(s.editor.active.popover, Some(DocPopover::BlockMenu { .. }));
            if locked {
                s.editor.active.typing = false;
                if !was_typing {
                    tx.live_only();
                }
                return;
            }
            let page_r = s.editor.active.page_rect.or(s.editor.active.viewport_rect);
            let next_hover = if let Some([px, _, pw, _]) = page_r {
                if p.window_x >= px - 64. && p.window_x <= px + pw + 24. {
                    s.editor
                        .active
                        .block_rects
                        .iter()
                        .filter(|&(&bid, r)| {
                            bid != TITLE_ID
                                && r[3] > 0.
                                && p.window_y >= r[1]
                                && p.window_y <= r[1] + r[3]
                        })
                        .min_by(|(_, a), (_, b)| a[3].total_cmp(&b[3]))
                        .map(|(&bid, _)| bid)
                } else {
                    None
                }
            } else {
                None
            };
            s.editor.active.typing = false;
            s.editor.active.hover_block = next_hover;
            if was_typing || prev_hover != next_hover {
                tx.instant();
            } else {
                tx.live_only();
            }
        })
        .on_hover_change(|s: &mut State, on, tx| {
            if !on
                && s.editor.active.drag.is_none()
                && !matches!(s.editor.active.popover, Some(DocPopover::BlockMenu { .. }))
            {
                if s.editor.active.hover_block.take().is_some() {
                    tx.instant();
                } else {
                    tx.live_only();
                }
            } else {
                tx.live_only();
            }
        })
        .child(page_col);

    let mut root = el()
        .key("doc-editor-stage")
        .size(viewport_width, height)
        .child(doc_scroll.scroll_bar_styled(crate::ui::editor_scrollbar(s.theme())));

    if let Some(overlay) = render_overlays(s, width, height) {
        for item in overlay {
            root = root.child(item);
        }
    }

    root
}

fn block_selection_state(s: &RenderState<'_>, id: u64) -> Option<(bool, usize, usize)> {
    let core = &s.editor.active.core;
    if core.in_block_mode() {
        let ids = core.selected_blocks_ids();
        if ids.contains(&id) {
            return Some((true, 0, core.len_of(id)));
        }
        return None;
    }
    if core.sel.is_collapsed() {
        return None;
    }
    let (a, b) = core.ordered();
    let us = core.units();
    let ia = us.iter().position(|u| u.id == a.block)?;
    let ib = us.iter().position(|u| u.id == b.block)?;
    let i = us.iter().position(|u| u.id == id)?;
    if i < ia || i > ib {
        return None;
    }
    let len = core.len_of(id);
    if i == ia && i == ib {
        Some((false, a.offset.min(b.offset), a.offset.max(b.offset)))
    } else if i == ia {
        Some((true, a.offset.min(len), len))
    } else if i == ib {
        Some((true, 0, b.offset.min(len)))
    } else {
        Some((true, 0, len))
    }
}

// Composition replaces the selection only in the rendered text until the IME commits.
fn composition_text<'a>(
    s: &RenderState<'_>, id: u64, text: &'a RichText,
) -> (std::borrow::Cow<'a, RichText>, std::ops::Range<usize>) {
    if s.editor.active.core.preedit.is_some()
        && let Some((_, start, end)) = block_selection_state(s, id)
        && start < end
    {
        let mut visible = text.clone();
        visible.replace(start..end, &RichText::default());
        return (std::borrow::Cow::Owned(visible), start..end);
    }
    (std::borrow::Cow::Borrowed(text), 0..0)
}

fn build_display_and_spans(
    s: &RenderState<'_>,
    id: u64,
    rich_text: &RichText,
    placeholder: &str,
    placeholder_color: u32,
    base_size: f32,
    is_code: bool,
    _strike_all: bool,
) -> (String, Vec<Span>, bool, usize, usize) {
    let code_spans = |source: &str| {
        let Some(DocBlock {
            kind: DocBlockKind::Code { lang, .. },
            ..
        }) = s.editor.active.core.block(id)
        else {
            unreachable!("code text must belong to a code block");
        };
        crate::ui::code_spans(lang, source, s.theme().is_dark())
    };
    let pre = s
        .editor
        .active
        .core
        .preedit
        .as_ref()
        .filter(|p| p.block == id && !p.text.is_empty());

    if rich_text.is_empty() && pre.is_none() {
        let ph_sp = Span::new(0, placeholder.len()).color(c(placeholder_color));
        return (
            placeholder.to_owned(),
            if placeholder.is_empty() {
                Vec::new()
            } else {
                vec![ph_sp]
            },
            true,
            0,
            0,
        );
    }

    let (out, mut spans, placeholder, pre_start, pre_len) = if let Some(p) = pre {
        let off = p.offset.min(rich_text.text.len());
        let mut out = String::with_capacity(rich_text.text.len() + p.text.len());
        out.push_str(&rich_text.text[..off]);
        out.push_str(&p.text);
        out.push_str(&rich_text.text[off..]);
        let plen = p.text.len();

        let plen_u = plen as u32;
        let off_u = off as u32;
        let mut spans = if is_code {
            code_spans(&out)
        } else {
            rich_text
                .to_rune_spans(base_size, s.theme(), crate::ui::accent(s))
                .into_iter()
                .map(|mut sp| {
                    if sp.start >= off_u {
                        sp.start += plen_u;
                        sp.end += plen_u;
                    } else if sp.end > off_u {
                        sp.end += plen_u;
                    }
                    sp
                })
                .collect::<Vec<_>>()
        };
        spans.push(Span::new(off, off + plen).underline(c(crate::ui::accent(s))));
        (out, spans, false, off, plen)
    } else {
        let spans = if is_code {
            code_spans(&rich_text.text)
        } else {
            rich_text.to_rune_spans(base_size, s.theme(), crate::ui::accent(s))
        };
        (rich_text.text.clone(), spans, false, 0, 0)
    };
    if let Some(sl) = &s.editor.active.slash
        && sl.block == id
    {
        let end = s.editor.active.core.sel.head.offset;
        spans.push(
            Span::new(sl.trigger_offset, end)
                .color(c(crate::ui::accent(s)))
                .bg(c(crate::ui::accent_soft(s)), 3.),
        );
    }
    (out, spans, placeholder, pre_start, pre_len)
}

fn render_text_with_caret(
    s: &RenderState<'_>,
    id: u64,
    rich_text: &RichText,
    style: TextStyle,
    max_w: f32,
    text_color: u32,
    placeholder: &str,
    placeholder_color: u32,
    is_code: bool,
    strike_all: bool,
    caret_color: u32,
) -> El {
    let align = match s.editor.active.core.block(id).map(|b| b.align) {
        Some(DocAlign::Center) => Align::Center,
        Some(DocAlign::Right) => Align::End,
        _ => Align::Start,
    };
    let wraps = !matches!(
        s.editor.active.core.block(id).map(|b| &b.kind),
        Some(DocBlockKind::Code { wrap: false, .. })
    );
    let layout_width = wraps.then_some(max_w);
    let base_size = style.size;
    let raw_len = rich_text.len();
    let (visible_text, removed) = composition_text(s, id, rich_text);
    let rich_text = visible_text.as_ref();
    let placeholder = if removed.is_empty() { placeholder } else { "" };
    let (display_text, spans, is_placeholder, pre_start, pre_len) = build_display_and_spans(
        s,
        id,
        rich_text,
        placeholder,
        placeholder_color,
        base_size,
        is_code,
        strike_all,
    );

    let editing = s.editor.active.focused &&
        (s.editor.active.core.sel.head.block == id || block_selection_state(s, id).is_some());
    let mut inline_error = None;
    let inline_preview = if !is_code && !is_placeholder && pre_len == 0 && !editing {
        match crate::rich_render::inline_preview(s, rich_text, style.size, text_color, max_w) {
            Ok(preview) => preview,
            Err(e) => { inline_error = Some(e); None }
        }
    } else { None };
    let (display_text, spans) = if let Some(preview) = &inline_preview {
        let spans = spans.into_iter().filter_map(|mut span| {
            span.start = preview.raw_to_display(span.start as usize) as u32;
            span.end = preview.raw_to_display(span.end as usize) as u32;
            (span.start < span.end).then_some(span)
        }).collect::<Vec<_>>();
        (preview.text.clone(), spans)
    } else { (display_text, spans) };

    let existing = s.editor.active.text_layouts.borrow().get(&id).cloned();
    let drawn_rect = existing
        .as_ref()
        .map(|m| m.borrow().drawn_rect.clone())
        .unwrap_or_else(|| rune::Live::new([0.; 4]));
    let next_meta = TextBoxMeta {
        drawn_rect: drawn_rect.clone(),
        scale: if s.mode == crate::model::WorkspaceMode::Canvas {
            s.note().map_or(1., |n| n.zoom)
        } else {
            1.
        },
        align,
        rect: [0.; 4],
        display_text: display_text.clone(),
        spans: spans.clone(),
        style: style.clone(),
        max_width: layout_width,
        inline_preview: inline_preview.clone(),
        raw_len,
        preedit_start: if removed.is_empty() { pre_start } else { removed.start },
        preedit_len: pre_len,
        preedit_removed: removed.len(),
        is_placeholder,
    };
    let meta = match existing {
        Some(entry) => {
            *entry.borrow_mut() = next_meta;
            entry
        }
        None => std::rc::Rc::new(RefCell::new(next_meta)),
    };

    // Text changes independently of its box size. Refresh content on every build,
    // while Rune publishes the actual drawn position on every scroll/animation frame.
    s.editor
        .active
        .text_layouts
        .borrow_mut()
        .insert(id, meta.clone());
    let mut tel = rich(display_text.clone(), spans.clone())
        .key(format!("doc-text-{id}"))
        .font(style.clone())
        .align(align)
        .w(max_w)
        .color(c(text_color))
        .drawn_rect_into(&drawn_rect);
    let owner = s.card_id.map(str::to_owned);
    tel = tel.on_layout_tx(move |s: &mut State, _, tx| {
        // During a scroll Rune can finish a layout from the previous mounted set.
        // Only publish geometry for a card that still has an editor on screen.
        let session = match owner.as_deref() {
            Some(card) if s.canvas_visible.contains(card) || s.draft.as_deref() == Some(card)
                || s.selected.as_deref() == Some(card) => s.editor.card_mut(card),
            Some(_) => None,
            None => crate::canvas_document::session_for_block(s, id),
        };
        if let Some(session) = session {
            session.text_layouts.borrow_mut().insert(id, meta.clone());
        }
        tx.live_only();
    });

    if wraps {
        tel = tel.wrap();
    }
    if let Some(preview) = &inline_preview {
        tel = tel.inline_images(&preview.images);
    }

    if !is_placeholder {
        if strike_all {
            tel = tel.strike(
                0,
                display_text.len(),
                Some(c(s.theme().choose(0x8f959e, 0x9297a3))),
            );
        } else {
            for (r, m) in rich_text.iter_spans() {
                if m.strike {
                    let rs = if pre_len > 0 && r.start >= pre_start {
                        r.start + pre_len
                    } else {
                        r.start
                    };
                    let re = if pre_len > 0 && r.end >= pre_start {
                        r.end + pre_len
                    } else {
                        r.end
                    };
                    let (rs, re) = if let Some(p) = &inline_preview { (p.raw_to_display(rs), p.raw_to_display(re)) } else { (rs, re) };
                    tel = tel.strike(rs, re, None);
                }
            }
        }
    }

    if s.editor.active.focused && !is_placeholder && s.editor.active.core.preedit.is_none() {
        if let Some((_, raw_a, raw_b)) = block_selection_state(s, id) {
            let da = if raw_a <= pre_start {
                raw_a
            } else {
                raw_a + pre_len
            };
            let db = if raw_b <= pre_start {
                raw_b
            } else {
                raw_b + pre_len
            };
            if da < db {
                tel = tel.text_highlight(TextHighlight::new(
                    da..db,
                    c(crate::ui::accent(s)).alpha(0.18),
                ));
            }
        }
    }

    let mut container = el().key(format!("tb-{id}")).w(max_w).child(tel);
    if let Some(error) = inline_error {
        container = col().w(max_w).child(container).child(
            label(error, 11., 400., s.theme().choose(0xb84034, 0xff9385)).wrap());
    }

    if let Some(slash) = &s.editor.active.slash
        && slash.block == id
    {
        // Anchor to the trigger, not the advancing query caret. This node is measured
        // with the freshly rendered text, including wrapping, alignment and canvas zoom.
        let anchor = rune::text::doc_text::caret_rect(
            &display_text, &spans, style.clone(), layout_width, align, slash.trigger_offset,
        );
        container = container.child(el().key("doc-slash-anchor")
            .absolute(anchor[0], anchor[1])
            .size(1.8, anchor[3].max(style_lh(&style)))
            .motion(Spring::NONE).inert().a11y_hidden());
    }

    let core = &s.editor.active.core;
    let input_block = core.preedit.as_ref().map_or(core.sel.head.block, |pre| pre.block);
    // macOS enables composition only while the focused editor exposes an IME area.
    // A text selection still needs that area before the first phonetic keystroke.
    if s.editor.active.focused && !s.query.focused && s.editor.active.editing_cell.is_none()
        && input_block == id
    {
        let disp_caret = if let Some(p) = s
            .editor
            .active
            .core
            .preedit
            .as_ref()
            .filter(|p| p.block == id)
        {
            p.offset.min(rich_text.len())
                + p.cursor
                    .map(|(_, e)| e.min(p.text.len()))
                    .unwrap_or(p.text.len())
        } else if is_placeholder {
            0
        } else {
            let raw = s.editor.active.core.sel.head.offset.min(rich_text.len());
            if pre_len > 0 && raw > pre_start {
                raw + pre_len
            } else {
                raw
            }
        };
        let cr = rune::text::doc_text::caret_rect(
            &display_text,
            &spans,
            style.clone(),
            layout_width,
            align,
            disp_caret,
        );
        let lh = cr[3].max(style_lh(&style));
        let mut caret = el()
            .key("doc-caret")
            .drawn_rect_into(&s.editor.active.caret_drawn_rect)
            .absolute(cr[0].max(0.), cr[1].max(0.))
            .size(1.8, lh)
            .radius(0.9)
            .motion(Spring::NONE)
            .motion_intent(MotionIntent::TYPING)
            .ime_cursor();
        if core.preedit.is_some() {
            caret = caret.fill(c(caret_color));
        } else if core.sel.is_collapsed() && !core.in_block_mode() {
            caret = caret.fill(c(caret_color)).live_opacity(&s.editor.active.caret_opacity);
        }
        container = container.child(caret);
    }

    container
}

fn render_title(s: &RenderState<'_>, doc_col_w: f32) -> El {
    let style = font(30., 600.).lh(46.);
    let title_box = render_text_with_caret(
        s,
        TITLE_ID,
        &s.editor.active.core.doc.title,
        style,
        doc_col_w,
        crate::ui::theme_ink(s.theme()),
        s.tr("请输入标题"),
        s.theme().choose(0xbbbfc4, 0x9297a3),
        false,
        false,
        crate::ui::accent(s),
    );

    track_rect(el().key("doc-title").w(doc_col_w).pb(8.), |s, r| {
        s.editor.active.block_rects.insert(TITLE_ID, r);
    })
    .child(title_box)
}

// Structured document metrics, in logical points. Marker shapes have
// their own optical anchors; a wide ordered number only widens its own item.
const BODY_SIZE: f32 = 16.;
const BODY_LINE: f32 = 26.;
const LIST_MARKER: f32 = 24.;
const BLOCK_INDENT: f32 = 25.;
const CODE_LINE: f32 = 22.;

#[derive(Clone, Copy)]
struct ListMetrics {
    number: u32,
    gap: f32,
}

fn ordered_label(number: u32, depth: usize) -> String {
    match depth % 3 {
        0 => format!("{number}."),
        1 => format!(
            "{}.",
            (b'a' + ((number.saturating_sub(1)) % 26) as u8) as char
        ),
        _ => format!("{number})"),
    }
}

fn list_metrics(list: &[DocBlock]) -> Vec<ListMetrics> {
    let mut number = 0u32;
    list.iter().map(|block| {
        number = match block.kind {
            DocBlockKind::Ordered { start } => start.unwrap_or(number.saturating_add(1)),
            _ => 0,
        };
        ListMetrics { number, gap: 0. }
    }).collect()
}

#[derive(Default)]
struct DocumentRowIndex {
    signature: Option<u64>,
    rows: std::rc::Rc<Vec<DocumentRow>>,
}

impl DocumentRowIndex {
    fn rows(&mut self, blocks: &[DocBlock]) -> (std::rc::Rc<Vec<DocumentRow>>, u64) {
        use std::hash::{DefaultHasher, Hash, Hasher};
        // Index only the fields that affect paths, folding, numbering and block spacing.
        // Text and marks are read from the live document when a visible row is built.
        fn shape(blocks: &[DocBlock], hash: &mut DefaultHasher) {
            blocks.len().hash(hash);
            for block in blocks {
                block.id.hash(hash);
                std::mem::discriminant(&block.kind).hash(hash);
                match block.kind {
                    DocBlockKind::Heading { level } => (level, block.folded).hash(hash),
                    DocBlockKind::Ordered { start } => start.hash(hash),
                    _ => {}
                }
                if !block.kind.is_container() {
                    shape(&block.children, hash);
                }
            }
        }
        let mut hash = DefaultHasher::new();
        shape(blocks, &mut hash);
        let signature = hash.finish();
        if self.signature != Some(signature) {
            self.rows = std::rc::Rc::new(document_rows(blocks));
            self.signature = Some(signature);
        }
        (self.rows.clone(), signature)
    }
}

struct DocumentRow {
    key: String,
    list: ListMetrics,
    path: Vec<usize>,
}

impl DocumentRow {
    fn resolve<'a>(&self, blocks: &'a [DocBlock]) -> (&'a [DocBlock], usize) {
        let mut list = blocks;
        for &index in &self.path[..self.path.len() - 1] {
            list = &list[index].children;
        }
        (
            list,
            *self.path.last().expect("document row has a block path"),
        )
    }
}

fn document_rows(blocks: &[DocBlock]) -> Vec<DocumentRow> {
    fn walk(list: &[DocBlock], path: &mut Vec<usize>, out: &mut Vec<DocumentRow>) {
        let metrics = list_metrics(list);
        let mut hidden: Option<u8> = None;
        for (i, block) in list.iter().enumerate() {
            if let Some(level) = hidden {
                match block.kind {
                    DocBlockKind::Heading { level: next } if next <= level => hidden = None,
                    _ => continue,
                }
            }
            path.push(i);
            out.push(DocumentRow {
                key: format!("doc-blk-{}", block.id),
                path: path.clone(),
                list: metrics[i],
            });
            if let DocBlockKind::Heading { level } = block.kind
                && block.folded
            {
                hidden = Some(level);
            } else if !block.kind.is_container() && !block.children.is_empty() {
                walk(&block.children, path, out);
            }
            path.pop();
        }
    }
    let mut rows = Vec::new();
    walk(blocks, &mut Vec::new(), &mut rows);
    let mut previous_bottom: Option<f32> = None;
    for row in &mut rows {
        let (siblings, i) = row.resolve(blocks);
        let (top, bottom) = margins_for(&siblings[i].kind);
        row.list.gap = previous_bottom.map_or(0., |previous| previous.max(top));
        previous_bottom = Some(bottom);
    }
    rows
}

fn collect_block_ids(block: &DocBlock, ids: &mut std::collections::HashSet<u64>) {
    ids.insert(block.id);
    for child in &block.children {
        collect_block_ids(child, ids);
    }
}

fn render_block_list(
    s: &RenderState<'_>,
    list: &[DocBlock],
    depth: usize,
    doc_col_w: f32,
    in_container: bool,
) -> Vec<(u64, El)> {
    let mut out = Vec::new();
    let mut hide_below: Option<u8> = None;
    let mut metrics = list_metrics(list);
    let mut previous_bottom: Option<f32> = None;

    for (idx, b) in list.iter().enumerate() {
        if let Some(lvl) = hide_below {
            match b.kind {
                DocBlockKind::Heading { level } if level <= lvl => hide_below = None,
                _ => continue,
            }
        }
        let (top, bottom) = if s.in_canvas() {
            match b.kind {
                DocBlockKind::Heading { .. } => (12., 5.),
                DocBlockKind::Bullet | DocBlockKind::Ordered { .. } | DocBlockKind::Todo { .. } => (2., 2.),
                DocBlockKind::Table(_) | DocBlockKind::Code { .. } | DocBlockKind::Callout { .. } => (8., 8.),
                _ => (5., 5.),
            }
        } else {
            margins_for(&b.kind)
        };
        metrics[idx].gap = previous_bottom.map_or(if depth > 0 { if s.in_canvas() { 2. } else { 8. } } else { 0. }, |previous| previous.max(top));
        previous_bottom = Some(bottom);
        let row = render_block_node(s, b, metrics[idx], depth, doc_col_w, in_container);
        out.push((b.id, row));
        if let DocBlockKind::Heading { level } = b.kind
            && b.folded
        {
            hide_below = Some(level);
        } else if !b.kind.is_container() && !b.children.is_empty() {
            out.extend(render_block_list(
                s,
                &b.children,
                depth + 1,
                doc_col_w,
                in_container,
            ));
        }
    }
    out
}

fn margins_for(kind: &DocBlockKind) -> (f32, f32) {
    match kind {
        DocBlockKind::Heading { level: 1 } => (26., 10.),
        DocBlockKind::Heading { level: 2 } => (22., 8.),
        DocBlockKind::Heading { level: 3 } => (20., 8.),
        DocBlockKind::Heading { .. } => (18., 8.),
        DocBlockKind::Callout { .. } | DocBlockKind::Code { .. } => (16., 16.),
        DocBlockKind::Table(_) => (29.5, 22.),
        _ => (8., 8.),
    }
}

fn align_wrap(inner: El, avail_w: f32, align: DocAlign) -> El {
    match align {
        DocAlign::Left => inner.w(avail_w),
        DocAlign::Center => row().w(avail_w).justify_center().child(inner),
        DocAlign::Right => row().w(avail_w).justify_end().child(inner),
    }
}

fn render_block_node(
    s: &RenderState<'_>,
    b: &DocBlock,
    list: ListMetrics,
    depth: usize,
    doc_col_w: f32,
    in_container: bool,
) -> El {
    let bid = b.id;
    let indent = ((depth.min(6) as f32) * BLOCK_INDENT).min((doc_col_w - 96.).max(0.));
    let mt = list.gap;
    let avail_w = (doc_col_w - indent).max(1.);

    let mut wrapper = col()
        .key(format!("doc-blk-{bid}"))
        .w(doc_col_w)
        .pl(indent)
        .pt(mt);
    if !in_container {
        wrapper = track_rect(wrapper, move |s, r| {
            if let Some(session) = crate::canvas_document::session_for_block(s, bid) {
                session.block_rects.insert(bid, r);
            }
        });
    }

    if let Some((multi_block, _, _)) = block_selection_state(s, bid)
        && (multi_block || !b.kind.has_text())
        && !(matches!(b.kind, DocBlockKind::Image { .. })
            && s.editor.active.core.block_sel == Some((bid, bid)))
        && s.editor.active.focused
    {
        wrapper = wrapper.fill(c(crate::ui::accent(s)).alpha(0.08)).radius(6.);
    }

    wrapper = wrapper.child(render_block_body(s, b, list, depth, avail_w));
    if let Some(DocDrag::ReorderBlock {
        target_block: Some(tb),
        after,
        ..
    }) = &s.editor.active.drag
        && *tb == bid
    {
        let line = el()
            .key("doc-drop-indicator")
            .w(avail_w)
            .h(2.)
            .radius(1.)
            .fill(c(crate::ui::accent(s)))
            .inert();
        wrapper = wrapper.child(if *after {
            line.pin(Some(indent), None, None, Some(0.))
        } else {
            line.absolute(indent, 0.)
        });
    }
    if in_container {
        wrapper
    } else {
        attach_left_handle(s, b, wrapper, mt)
    }
}

fn render_block_body(
    s: &RenderState<'_>,
    b: &DocBlock,
    list: ListMetrics,
    depth: usize,
    avail_w: f32,
) -> El {
    let bid = b.id;
    let body_font = s.body_style();
    let body_line = style_lh(&body_font);
    let show_hint = b.text.is_empty()
        && !s.blank_canvas_input
        && s.editor.active.focused
        && s.editor.active.editing_cell.is_none()
        && !s.editor.active.core.in_block_mode()
        && s.editor.active.core.sel.is_collapsed()
        && s.editor.active.core.sel.head.block == bid;
    match &b.kind {
        DocBlockKind::Text => {
            let el_box = render_text_with_caret(
                s,
                bid,
                &b.text,
                body_font,
                avail_w,
                crate::ui::theme_ink(s.theme()),
                if show_hint {
                    s.tr("输入 / 或 、 插入模块")
                } else {
                    ""
                },
                s.theme().choose(0xc5c9d0, 0x9297a3),
                false,
                false,
                crate::ui::accent(s),
            );
            align_wrap(el_box, avail_w, b.align)
        }
        DocBlockKind::Heading { level } => {
            let (sz, lh, wt) = s.heading_style(*level);
            let ph = s.tr(match level { 1 => "一级标题", 2 => "二级标题", 3 => "三级标题", _ => "四级标题" });
            let tel = render_text_with_caret(
                s,
                bid,
                &b.text,
                font(sz, wt).lh(lh).tracking(-0.02 * sz),
                avail_w,
                crate::ui::theme_ink(s.theme()),
                ph,
                s.theme().choose(0xc5c9d0, 0x9297a3),
                false,
                false,
                crate::ui::accent(s),
            );
            if s.in_canvas() {
                return tel;
            }
            let fold_width = 18.;
            let gutter = 24.;
            let caret_btn = el()
                .key(format!("fold-{bid}"))
                .role("button")
                .focusable(true)
                .group_opacity(0., 1.)
                .expanded(!b.folded)
                .label(if b.folded {
                    s.tr("展开标题")
                } else {
                    s.tr("折叠标题")
                })
                .size(fold_width, 22.)
                .absolute(0., (lh - 22.) * 0.5)
                .on_press_tx(move |s: &mut State, tx| {
                    s.editor.active.core.toggle_fold(bid);
                    sync_to_state(s);
                    tx.instant();
                })
                .child(
                    el().size(fold_width, 22.)
                        .center()
                        .radius(4.)
                        .hover(c(crate::ui::theme_ink(s.theme())).alpha(0.06))
                        .cursor(Cursor::Pointer)
                        .on_drag(move |s: &mut State, d, tx| {
                            block_control_drag(s, bid, true, d, tx)
                        })
                        .child(
                            ico(
                                if b.folded {
                                    &CHEVRON_RIGHT
                                } else {
                                    &CHEVRON_DOWN
                                },
                                12.,
                            )
                            .color(c(s.theme().choose(0x8f959e, 0x9297a3))),
                        ),
                );
            // Keep the gap between the text and its disclosure in the same hover target.
            el().w(avail_w + gutter)
                .ml(-gutter)
                .hover_group_focus()
                .child(tel.ml(gutter))
                .child(caret_btn)
        }
        DocBlockKind::Bullet => {
            let dot = el().key(format!("doc-marker-{bid}"));
            let (dot, size) = match depth % 3 {
                0 => (
                    dot.size(5.5, 5.5)
                        .radius(2.75)
                        .fill(c(crate::ui::accent(s))),
                    5.5,
                ),
                1 => (
                    dot.size(5.5, 5.5)
                        .radius(2.75)
                        .border(1., c(crate::ui::accent(s))),
                    5.5,
                ),
                _ => (dot.size(5., 5.).fill(c(crate::ui::accent(s))), 5.),
            };
            let marker = el()
                .size(LIST_MARKER, body_line)
                .child(dot.absolute(4., (body_line - size) * 0.5));
            let txt_w = (avail_w - LIST_MARKER).max(1.);
            let tel = render_text_with_caret(
                s,
                bid,
                &b.text,
                body_font,
                txt_w,
                crate::ui::theme_ink(s.theme()),
                if show_hint { s.tr("列表项") } else { "" },
                s.theme().choose(0xc5c9d0, 0x9297a3),
                false,
                false,
                crate::ui::accent(s),
            );
            row().w(avail_w).min_h(body_line).child(marker).child(tel)
        }
        DocBlockKind::Ordered { .. } => {
            let number = ordered_label(list.number, depth);
            let width = (rune::text::measure(&number, &[], body_font, None).0 + 4.).max(18.) + 6.;
            let marker = el().size(width, body_line).child(
                text(number)
                    .key(format!("doc-marker-{bid}"))
                    .font(body_font)
                    .color(c(crate::ui::accent(s)))
                    .w(width - 6.)
                    .h(body_line)
                    .px(2.),
            );
            let txt_w = (avail_w - width).max(1.);
            let tel = render_text_with_caret(
                s,
                bid,
                &b.text,
                body_font,
                txt_w,
                crate::ui::theme_ink(s.theme()),
                if show_hint {
                    s.tr("编号列表项")
                } else {
                    ""
                },
                s.theme().choose(0xc5c9d0, 0x9297a3),
                false,
                false,
                crate::ui::accent(s),
            );
            row().w(avail_w).min_h(body_line).child(marker).child(tel)
        }
        DocBlockKind::Todo { done } => {
            let done = *done;
            let mut kit = crate::ui::kit(s);
            kit.radius.sm = if s.in_canvas() { 5. } else { 3. };
            kit.c.border_weak = c(s.theme().choose(0x969ba4, 0x606570));
            kit.c.std_border = c(crate::ui::accent(s));
            kit.c.surface_weak = c(crate::ui::accent(s));
            kit.c.surface_hover = c(crate::ui::accent_soft(s));
            kit.c.std_icon = c(s
                .theme()
                .choose(0xffffff, crate::ui::editor_colors(s.theme()).raised));
            kit.motion.control = rune::Spring::NONE;
            if s.in_canvas() {
                let sage = c(s.theme().choose(0x54725d, 0xa3c8a9));
                kit.c.std_border = sage;
                kit.c.surface_weak = sage;
            }
            let box_el = el()
                .key(format!("todo-chk-{bid}"))
                .role("checkbox")
                .checked(done)
                .focusable(true)
                .label(if done {
                    s.tr("标记为未完成")
                } else {
                    s.tr("标记为完成")
                })
                .size(LIST_MARKER, body_line)
                .instant_changes()
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut State, tx| {
                    crate::canvas_document::activate_block(s, bid);
                    s.editor.active.core.toggle_todo(bid);
                    sync_to_state(s);
                    tx.instant();
                })
                .child(
                    rune_kit::Checkbox::new(done)
                        .build(&kit)
                        .key(format!("doc-marker-{bid}"))
                        .absolute(0., (body_line - 16.) * 0.5)
                        .size(16., 16.)
                        .instant_changes()
                        .inert()
                        .a11y_hidden(),
                );
            let txt_w = (avail_w - LIST_MARKER).max(1.);
            let tel = render_text_with_caret(
                s,
                bid,
                &b.text,
                body_font,
                txt_w,
                if s.in_canvas() && done { crate::ui::theme_muted(s.theme()) } else { crate::ui::theme_ink(s.theme()) },
                if show_hint { s.tr("待办事项") } else { "" },
                s.theme().choose(0xc5c9d0, 0x9297a3),
                false,
                s.in_canvas() && done,
                crate::ui::accent(s),
            );
            row().w(avail_w).min_h(body_line).child(box_el).child(tel)
        }
        DocBlockKind::Quote => {
            let inset = if s.in_canvas() { 31. } else { 14. };
            let inner_w = (avail_w - inset).max(1.);
            let children = render_block_list(s, &b.children, 0, inner_w, true);
            let mut inner_col = col().w(inner_w);
            for (_, c_el) in children {
                inner_col = inner_col.child(c_el);
            }
            if s.in_canvas() {
                let plum = c(s.theme().choose(0x8e719c, 0xc6a8d1));
                return row().w(avail_w).py(5.).pl(2.).gap(9.).child(
                    el().w(20.).h(21.).center().child(ico(&QUOTE, 17.).color(plum)).inert()
                ).child(inner_col);
            }
            let base = c(s.theme().choose(0xc4cad4, 0x555a64)).to_oklab();
            let tint = c(crate::ui::accent(s)).to_oklab();
            let bar = rune::Color::from_oklab(std::array::from_fn(|i| {
                base[i] + (tint[i] - base[i]) * 0.30
            }));
            row()
                .w(avail_w)
                .py(2.)
                .gap(11.)
                .child(el().w(3.).radius(1.5).fill(bar).h_full())
                .child(inner_col)
        }
        DocBlockKind::Callout { emoji, palette } => {
            let (bg_hex, border_hex) = callout_palette_colors(palette, s.theme());
            let (pad_x, pad_y, gap) = if s.in_canvas() { (10., 6., 6.) } else { (17., 17., 12.) };
            let inner_w = (avail_w - 2. * pad_x - 24. - gap).max(1.);
            let children = render_block_list(s, &b.children, 0, inner_w, true);
            let mut inner_col = col().w(inner_w);
            for (_, c_el) in children {
                inner_col = inner_col.child(c_el);
            }
            let emoji_btn = el()
                .key(format!("callout-icon-{bid}"))
                .role("button")
                .label(s.tr("高亮块样式"))
                .size(24., 26.)
                .radius(4.)
                .center()
                .hover(c(crate::ui::theme_ink(s.theme())).alpha(0.06))
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut State, _| {
                    crate::canvas_document::activate_block(s, bid);
                    s.editor.active.popover = Some(DocPopover::CalloutStyle { block_id: bid });
                })
                .child(
                    text(emoji.clone())
                        .font(font(18., 400.).lh(22.))
                        .color(c(crate::ui::theme_ink(s.theme()))),
                );

            let strip = row()
                .w(avail_w)
                .px(pad_x).py(pad_y)
                .gap(gap)
                .radius(if s.in_canvas() { 5. } else { 10. })
                .fill(c(bg_hex))
                .child(emoji_btn)
                .child(inner_col);
            if s.in_canvas() { strip.border_sides([0., 0., 0., 3.], c(border_hex).mix(c(crate::ui::theme_ink(s.theme())), 0.14)) }
            else { strip.border(1., c(border_hex)) }
        }
        DocBlockKind::Code { lang, wrap } => {
            let rich_kind = crate::rich_render::language(lang);
            let editing = s.editor.active.focused
                && !matches!(
                    s.editor.active.popover,
                    Some(DocPopover::BlockMenu { .. } | DocPopover::AlignMenu)
                )
                && (s.editor.active.core.sel.head.block == bid
                    || block_selection_state(s, bid).is_some());
            if rich_kind.is_some() && !editing {
                return crate::rich_render::block_preview(s, b, avail_w);
            }
            let is_math = rich_kind == Some(true);
            let wrap_on = if is_math { true } else { *wrap };
            let copied = s
                .editor
                .active
                .copied_block
                .is_some_and(|(id, at)| id == bid && at.elapsed().as_secs_f32() < 1.6);
            let code_txt = b.text.text.clone();
            let colors = crate::ui::editor_colors(s.theme());
            let code_paper = if s.in_canvas() { s.theme().choose(0xf4f4fb, 0x292a35) } else { colors.inset };
            let code_header = if s.in_canvas() { s.theme().choose(0xe9eaf6, 0x343544) } else { colors.inset };
            let code_line = if s.in_canvas() { 19. } else { CODE_LINE };
            let code_style = TextStyle::new(if s.in_canvas() { 13. } else { 14. }, 400.)
                .family(Family::Named(crate::platform::CODE_FONT))
                .lh(code_line);
            let placeholder = if is_math {
                s.tr("请输入 LaTeX 公式")
            } else {
                s.tr("输入代码…")
            };
            let (visible_code, _) = composition_text(s, bid, &b.text);
            let (display, spans, _, _, _) = build_display_and_spans(
                s,
                bid,
                &visible_code,
                placeholder,
                s.theme().choose(0x7b8393, 0x9297a3),
                code_style.size,
                true,
                false,
            );
            let lang_label = match rich_kind {
                Some(true) => s.tr("数学公式").to_owned(),
                Some(false) => "Mermaid".to_owned(),
                None => lang.clone(),
            };
            let copy_ok_hex = s.theme().choose(0x2ea121, 0x72c79b);
            let muted_btn = s.theme().choose(0x646a73, 0xb3b6bf);
            let mut header = row()
                .w_full()
                .h(32.)
                .px(12.)
                .gap(4.)
                .center_y()
                .fill(c(code_header))
                .border_sides([0., 0., 1., 0.], c(colors.border).alpha(0.72))
                .child(
                    row()
                        .key(format!("code-lang-{bid}"))
                        .px(8.)
                        .h(22.)
                        .radius(6.)
                        .center_y()
                        .gap(4.)
                        .hover(c(crate::ui::theme_ink(s.theme())).alpha(0.06))
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s: &mut State, _| {
                            crate::canvas_document::activate_block(s, bid);
                            s.editor.active.popover = Some(DocPopover::CodeLang { block_id: bid });
                        })
                        .child(label(
                            lang_label,
                            12.,
                            520.,
                            s.theme().choose(0x525864, 0xb3b6bf),
                        ))
                        .child(
                            ico(&CHEVRON_DOWN, 11.).color(c(s.theme().choose(0x7b8393, 0x9297a3))),
                        ),
                )
                .child(el().grow(1.));

            if !is_math {
                header = header.child(
                    el().key(format!("code-wrap-{bid}"))
                        .px(8.)
                        .h(22.)
                        .radius(6.)
                        .center()
                        .fill(if wrap_on {
                            c(crate::ui::accent(s)).alpha(0.10)
                        } else {
                            rune::Color::TRANSPARENT
                        })
                        .hover(c(crate::ui::theme_ink(s.theme())).alpha(0.06))
                        .cursor(Cursor::Pointer)
                        .on_press_tx(move |s: &mut State, _| {
                            crate::canvas_document::activate_block(s, bid);
                            s.editor.active.core.with_undo(|core| {
                                if let Some(bl) = core.block_mut(bid)
                                    && let DocBlockKind::Code { wrap, .. } = &mut bl.kind
                                {
                                    *wrap = !*wrap;
                                }
                            });
                            sync_to_state(s);
                        })
                        .child(label(
                            if wrap_on {
                                s.tr("自动换行")
                            } else {
                                s.tr("横向滚动")
                            },
                            11.5,
                            500.,
                            if wrap_on {
                                crate::ui::accent(s)
                            } else {
                                muted_btn
                            },
                        )),
                );
            }

            header = header.child(
                row()
                    .key(format!("code-copy-{bid}"))
                    .px(8.)
                    .h(22.)
                    .gap(4.)
                    .radius(6.)
                    .center_y()
                    .hover(c(crate::ui::theme_ink(s.theme())).alpha(0.06))
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut State, tx| {
                        crate::canvas_document::activate_block(s, bid);
                        tx.copy(code_txt.clone());
                        s.editor.active.copied_block = Some((bid, Instant::now()));
                    })
                    .child(ico(if copied { &CHECK } else { &COPY }, 11.5).color(c(if copied {
                        copy_ok_hex
                    } else {
                        muted_btn
                    })))
                    .child(label(
                        if copied {
                            s.tr("已复制")
                        } else {
                            s.tr("复制")
                        },
                        11.5,
                        500.,
                        if copied { copy_ok_hex } else { muted_btn },
                    )),
            );

            if rich_kind.is_some() {
                header = header.child(
                    row()
                        .key(format!("code-preview-{bid}"))
                        .h(22.)
                        .px(9.)
                        .gap(4.)
                        .radius(6.)
                        .center_y()
                        .fill(c(crate::ui::accent_soft(s)))
                        .hover(c(crate::ui::accent(s)).alpha(0.18))
                        .cursor(Cursor::Pointer)
                        .role("button")
                        .label(s.tr("预览"))
                        .on_press_tx(move |s: &mut State, tx| {
                            crate::canvas_document::activate_block(s, bid);
                            s.editor.active.focused = false;
                            tx.blur();
                            tx.instant();
                        })
                        .child(ico(&CHECK, 11.5).color(c(crate::ui::accent(s))))
                        .child(label(s.tr("预览"), 11.5, 520., crate::ui::accent(s))),
                );
            }

            let body = if is_math {
                let viewport_w = (avail_w - 2. - 32.).max(1.);
                let layout = rune::text::doc_text::measure_lines(
                    &display,
                    &spans,
                    code_style,
                    Some(viewport_w),
                    Align::Start,
                );
                let code_h = layout.height.max(code_line);
                let code_editor = render_text_with_caret(
                    s,
                    bid,
                    &b.text,
                    code_style,
                    viewport_w,
                    crate::ui::theme_ink(s.theme()),
                    placeholder,
                    s.theme().choose(0x7b8393, 0x9297a3),
                    true,
                    false,
                    crate::ui::accent(s),
                );
                el().w_full()
                    .px(16.)
                    .pt(10.)
                    .pb(14.)
                    .child(
                        el().key(format!("code-viewport-{bid}"))
                            .size(viewport_w, code_h)
                            .child(code_editor),
                    )
            } else {
                let line_count = display.split('\n').count();
                let gutter_w = rune::text::measure(&line_count.to_string(), &[], code_style, None)
                    .0
                    .max(31.);
                let viewport_w = (avail_w - 2. - 8. - gutter_w - 20. - 16.).max(1.);
                let layout = rune::text::doc_text::measure_lines(
                    &display,
                    &spans,
                    code_style,
                    wrap_on.then_some(viewport_w),
                    Align::Start,
                );
                let code_w = if wrap_on {
                    viewport_w
                } else {
                    viewport_w.max(layout.width + 2.)
                };
                let code_h = layout.height.max(code_line);
                let mut gutter = el()
                    .key(format!("code-gutter-{bid}"))
                    .size(gutter_w, code_h);
                let mut number = 1;
                for line in &layout.lines {
                    if line.start == 0 || display.as_bytes().get(line.start - 1) == Some(&b'\n') {
                        gutter = gutter.child(
                            text(number.to_string())
                                .key(format!("code-line-{bid}-{number}"))
                                .font(code_style)
                                .color(c(s.theme().choose(0x8e95a3, 0x767a84)))
                                .align(Align::End)
                                .size(gutter_w, code_line)
                                .absolute(0., line.top),
                        );
                        number += 1;
                    }
                }
                let code_editor = render_text_with_caret(
                    s,
                    bid,
                    &b.text,
                    code_style,
                    code_w,
                    crate::ui::theme_ink(s.theme()),
                    placeholder,
                    s.theme().choose(0x7b8393, 0x9297a3),
                    true,
                    false,
                    crate::ui::accent(s),
                );
                row()
                    .w_full()
                    .pt(8.)
                    .pb(if s.in_canvas() { 8. } else { 18. })
                    .pl(8.)
                    .pr(16.)
                    .gap(20.)
                    .child(gutter)
                    .child(
                        el().key(format!("code-viewport-{bid}"))
                            .size(viewport_w, code_h)
                            .scroll_x()
                            .child(code_editor),
                    )
            };

            col()
                .w(avail_w)
                .padding(1.)
                .radius(10.)
                .border(1., c(colors.border))
                .fill(c(code_paper))
                .clip_rounded(10.)
                .child(header)
                .child(body)
        }
        DocBlockKind::Divider => row().w(avail_w).h(if s.in_canvas() { 18. } else { 22. }).center_y().child(
            el().w_full()
                .h(1.)
                .fill(c(crate::ui::editor_colors(s.theme()).border)),
        ),
        DocBlockKind::Image { src, width, caption } => {
            image_block::render(s, bid, src, *width, caption, avail_w, b.align)
        }
        DocBlockKind::Table(table) => crate::doc_tables::render(
            s,
            bid,
            table,
            avail_w,
            s.editor.active.focused && block_selection_state(s, bid).is_some(),
        ),
    }
}

fn kind_handle_spec(s: &RenderState<'_>, b: &DocBlock) -> (bool, &'static rune::Icon, u32) {
    match &b.kind {
        DocBlockKind::Text => {
            if b.text.is_empty() {
                (true, &PLUS, s.theme().choose(0x8f959e, 0x9297a3))
            } else {
                (false, &TYPE, s.theme().choose(0x5f6876, 0xb3b6bf))
            }
        }
        DocBlockKind::Heading { level: 1 } => (false, &HEADING_1, crate::ui::accent(s)),
        DocBlockKind::Heading { level: 2 } => (false, &HEADING_2, crate::ui::accent(s)),
        DocBlockKind::Heading { .. } => (false, &HEADING_3, crate::ui::accent(s)),
        DocBlockKind::Bullet => (false, &LIST, crate::ui::accent(s)),
        DocBlockKind::Ordered { .. } => (false, &LIST_ORDERED, crate::ui::accent(s)),
        DocBlockKind::Todo { .. } => (false, &CHECK_SQUARE, crate::ui::accent(s)),
        DocBlockKind::Quote => (false, &QUOTE, s.theme().choose(0x8f959e, 0x9297a3)),
        DocBlockKind::Callout { .. } => (false, &BULB, 0xff811a),
        DocBlockKind::Code { .. } => (false, &CODE, crate::ui::accent(s)),
        DocBlockKind::Divider => (false, &MINUS, s.theme().choose(0x8f959e, 0x9297a3)),
        DocBlockKind::Image { .. } => (false, &IMAGE, crate::ui::accent(s)),
        DocBlockKind::Table(_) => (false, &TABLE, crate::ui::accent(s)),
    }
}

/// Keep sibling indices absolute so a heading at the end of a subsection
/// also checks the next sibling outside that subsection.
fn reorder_targets(
    list: &[DocBlock],
    range: std::ops::Range<usize>,
    source: u64,
    source_level: Option<u8>,
    targets: &mut HashMap<u64, ReorderRow>,
) -> Option<u64> {
    let mut last = None;
    let mut i = range.start;
    while i < range.end {
        let b = &list[i];
        let end = crate::doc_core::section_end(list, i);
        if b.id != source {
            let mut bottom = b.id;
            if !b.folded
                && !b.kind.is_container()
                && let Some(child) = reorder_targets(
                    &b.children,
                    0..b.children.len(),
                    source,
                    source_level,
                    targets,
                )
            {
                bottom = child;
            }
            let mut anchor_index = i;
            let mut marker = bottom;
            if matches!(b.kind, DocBlockKind::Heading { .. }) {
                if !b.folded
                    && let Some(body) =
                        reorder_targets(list, i + 1..end, source, source_level, targets)
                {
                    bottom = body;
                }
                if source_level.is_some() {
                    anchor_index = end - 1;
                    marker = bottom;
                }
            }
            let fits = |index| {
                source_level.is_none_or(|level| {
                    crate::doc_core::section_fits_before(level, list.get(index))
                })
            };
            targets.insert(
                b.id,
                ReorderRow {
                    before: fits(i),
                    below: fits(anchor_index + 1).then_some((list[anchor_index].id, marker)),
                },
            );
            last = Some(bottom);
        }
        i = end;
    }
    last
}

fn block_control_drag(s: &mut State, bid: u64, fold: bool, d: &mut rune::Drag, tx: &mut Tx) {
    let pt = d.window_point();
    if d.phase == Phase::Down {
        crate::canvas_document::activate_block(s, bid);
        let session = &mut s.editor.active;

        session.focused = true;
        session.typing = false;
        session.popover = None;
        session.slash = None;
        session.drag = Some(DocDrag::ReorderBlock {
            block_id: bid,
            target_block: None,
            after: false,
            moved: false,
            pointer: pt,
            targets: Default::default(),
        });
        tx.instant();
        return;
    }
    let session = &mut s.editor.active;
    let Some(DocDrag::ReorderBlock {
        block_id,
        targets,
        moved,
        pointer,
        target_block,
        after,
    }) = &mut session.drag
    else {
        return;
    };
    if *block_id != bid {
        return;
    }
    if !*moved && d.dx.abs().max(d.dy.abs()) > 4. && !d.cancelled() {
        let Some(block) = session.core.block(bid) else {
            session.drag = None;
            return;
        };
        let level = match block.kind {
            DocBlockKind::Heading { level } => Some(level),
            _ => None,
        };
        reorder_targets(
            &session.core.doc.blocks,
            0..session.core.doc.blocks.len(),
            bid,
            level,
            std::rc::Rc::make_mut(targets),
        );
        *moved = true;
    }
    *pointer = pt;
    let inside =
        |r: [f32; 4]| pt[0] >= r[0] && pt[0] < r[0] + r[2] && pt[1] >= r[1] && pt[1] < r[1] + r[3];
    let mut drop = None;
    if *moved && session.viewport_rect.is_some_and(inside) && session.page_rect.is_some_and(inside)
    {
        for (&id, r) in &session.block_rects {
            if pt[1] < r[1] || pt[1] >= r[1] + r[3] {
                continue;
            }
            let Some(row) = targets.get(&id) else {
                continue;
            };
            drop = if pt[1] >= r[1] + r[3] * 0.5 {
                row.below.map(|(anchor, marker)| (anchor, marker, true))
            } else {
                row.before.then_some((id, id, false))
            };
            break;
        }
    }
    *target_block = drop.map(|(_, marker, _)| marker);
    *after = drop.is_some_and(|(_, _, below)| below);
    if d.phase == Phase::Up {
        let was_drag = *moved;
        session.drag = None;
        if !d.cancelled() {
            if was_drag {
                if let Some((anchor, _, after)) = drop
                    && session.core.move_block(bid, anchor, after)
                {
                    session.commit_cell_edit();
                    session.table_selection = None;
                    session.goal_x = None;
                    session.hover_block = None;
                    tx.blur();
                    sync_to_state(s);
                }
            } else if fold {
                session.core.toggle_fold(bid);
                sync_to_state(s);
            } else {
                session.popover = Some(DocPopover::BlockMenu { block_id: bid });
            }
        }
    }
    tx.instant();
}

fn attach_left_handle(s: &RenderState<'_>, b: &DocBlock, wrapper: El, mt: f32) -> El {
    let bid = b.id;
    let is_hovered = s.editor.active.hover_block == Some(bid);
    let is_menu = matches!(
        s.editor.active.popover,
        Some(DocPopover::BlockMenu { block_id }) if block_id == bid
    );
    let is_dragging_this = matches!(
        s.editor.active.drag,
        Some(DocDrag::ReorderBlock { block_id, .. }) if block_id == bid
    );
    if s.blank_canvas_input || (!is_hovered && !is_menu && !is_dragging_this) {
        return wrapper;
    }
    if s.editor.active.typing || matches!(s.editor.active.drag, Some(DocDrag::Selecting { .. })) {
        return wrapper;
    }

    let (is_empty_add, icon, icon_color) = kind_handle_spec(s, b);
    let handle_top = match &b.kind {
        DocBlockKind::Divider => mt + if s.in_canvas() { (18. - 26.) * 0.5 } else { -2. },
        DocBlockKind::Callout { .. } => mt + if s.in_canvas() { 6. } else { 17. },
        DocBlockKind::Code { lang, .. } => {
            let rich = crate::rich_render::language(lang).is_some();
            let editing = s.editor.active.focused
                && (s.editor.active.core.sel.head.block == bid
                    || block_selection_state(s, bid).is_some());
            if rich && !editing {
                mt + 12.
            } else {
                mt + 4.
            }
        }
        _ => {
            let line = match b.kind {
                DocBlockKind::Heading { level } => s.heading_style(level).1,
                _ => style_lh(&s.body_style()),
            };
            mt + (line - 26.) * 0.5
        }
    };
    let embedded = s.mode == crate::model::WorkspaceMode::Canvas;
    let heading = matches!(b.kind, DocBlockKind::Heading { .. });
    let handle_width = if embedded { CARD_GRIP_WIDTH } else { 42. };
    let handle_gap = match (embedded, heading) {
        (true, _) => 2.,
        (false, true) => 30.,
        (false, false) => 6.,
    };
    let handle_left = -(handle_width + handle_gap);
    let handle = if is_empty_add {
        el().key(format!("doc-handle-{bid}"))
            .absolute(if embedded { handle_left } else { -34. }, handle_top)
            .size(if embedded { handle_width } else { 28. }, 26.)
            .radius(6.)
            .center()
            .fill(c(crate::ui::editor_colors(s.theme()).raised))
            .border(1., c(crate::ui::editor_colors(s.theme()).border))
            .shadow(1., 3., c(0x1f2329).alpha(0.06))
            .hover(c(s
                .theme()
                .choose(0xf2f3f5, crate::ui::editor_colors(s.theme()).inset)))
            .cursor(Cursor::Pointer)
            .on_press_tx(move |s: &mut State, _| {
                crate::canvas_document::activate_block(s, bid);
                s.editor.active.focused = true;
                s.editor.active.typing = false;
                s.editor.active.core.sel = DocSel::caret(DocPos::new(bid, 0));
                s.editor.active.core.insert_text("/");
                s.editor.active.maybe_open_slash("/");
                sync_to_state(s);
            })
            .child(ico(&PLUS, 14.).color(c(s.theme().choose(0x8f959e, 0x9297a3))))
    } else {
        row()
            .key(format!("doc-handle-{bid}"))
            .absolute(handle_left, handle_top)
            .size(handle_width, 26.)
            .px(if embedded {
                (CARD_GRIP_WIDTH - 13.) * 0.5
            } else {
                4.
            })
            .gap(2.)
            .radius(6.)
            .center_y()
            .fill(c(crate::ui::editor_colors(s.theme()).raised))
            .border(1., c(crate::ui::editor_colors(s.theme()).border))
            .shadow(1., 3., c(0x1f2329).alpha(0.06))
            .hover(c(crate::ui::editor_colors(s.theme()).inset))
            .cursor(Cursor::Grab)
            .on_drag(move |s: &mut State, d, tx| block_control_drag(s, bid, false, d, tx))
            .children((!embedded).then(|| {
                el().size(18., 18.)
                    .radius(4.)
                    .center()
                    .fill(c(icon_color).alpha(0.10))
                    .child(ico(icon, 12.).color(c(icon_color)))
            }))
            .child(ico(&GRIP_VERTICAL, 13.).color(c(s.theme().choose(0x8f959e, 0x9297a3))))
    };

    wrapper.child(handle)
}

fn render_overlays(s: &State, pane_w: f32, pane_h: f32) -> Option<Vec<El>> {
    let vp = s.editor.active.viewport_rect?;
    let mut overlays = Vec::new();

    if let Some(sl) = &s.editor.active.slash {
        overlays.push(crate::insert_menu::render(
            s,
            "doc-slash-menu",
            &sl.query,
            sl.selected,
            pane_h,
            execute_slash_cmd,
        ));
    }

    if let Some(pop) = &s.editor.active.popover {
        if let Some(pop_el) = render_popover(s, pop, vp, pane_w, pane_h) {
            overlays.push(pop_el);
        }
    }

    if s.editor.active.focused
        && !s.editor.active.core.sel.is_collapsed()
        && s.editor.active.drag.is_none()
        && s.editor.active.slash.is_none()
        && !matches!(
            s.editor.active.popover,
            Some(
                DocPopover::BlockMenu { .. }
                    | DocPopover::CalloutStyle { .. }
                    | DocPopover::CodeLang { .. }
            )
        )
        && let Some(ar) = s.editor.active.selection_anchor_window_rect()
    {
        if let Some(tb) = render_selection_toolbar(s, ar, vp, pane_w, pane_h) {
            overlays.push(tb);
        }
    }

    if let Some(DocDrag::ReorderBlock {
        block_id,
        moved: true,
        pointer,
        ..
    }) = &s.editor.active.drag
        && let Some(b) = s.editor.active.core.block(*block_id)
    {
        let (_, icon, icon_color) = kind_handle_spec(&render_state(s), b);
        let snippet = {
            let txt = match &b.kind {
                DocBlockKind::Divider => s.tr("分割线").to_owned(),
                DocBlockKind::Image { caption, .. } => {
                    if caption.is_empty() {
                        s.tr("图片").to_owned()
                    } else {
                        caption.clone()
                    }
                }
                DocBlockKind::Table(_) => s.tr("表格").to_owned(),
                _ => {
                    let t = b.text.text.trim();
                    if t.is_empty() {
                        s.tr("空块").to_owned()
                    } else {
                        t.chars().take(28).collect()
                    }
                }
            };
            txt
        };
        let gx = (pointer[0] - vp[0] + 14.).clamp(8., (pane_w - 230.).max(8.));
        let gy = (pointer[1] - vp[1] - 16.).clamp(8., (pane_h - 40.).max(8.));
        overlays.push(
            row()
                .key("doc-drag-ghost")
                .absolute(gx, gy)
                .max_w(220.)
                .h(32.)
                .px(10.)
                .gap(7.)
                .radius(8.)
                .center_y()
                .fill(c(crate::ui::editor_colors(s.theme()).raised).alpha(0.96))
                .border(
                    1.,
                    c(s.theme()
                        .choose(0xd0d7e2, crate::ui::editor_colors(s.theme()).border)),
                )
                .shadow(4., 16., c(0x1f2329).alpha(0.14))
                .inert()
                .child(ico(icon, 13.).color(c(icon_color)))
                .child(label(snippet, 12.5, 500., s.theme().choose(0x1f2329, 0xe6e7eb)).truncate()),
        );
    }

    Some(overlays)
}

const TOOLBAR_WIDTH: f32 = crate::inline_format::WIDTH;
fn toolbar_position(s: &State, ar: [f32; 4], vp: [f32; 4], w: f32, h: f32) -> [f32; 2] {
    let focus = s.editor.active.caret_window_rect().unwrap_or(ar);
    let x = (focus[0] - vp[0] - TOOLBAR_WIDTH / 2.).clamp(8., (w - TOOLBAR_WIDTH - 8.).max(8.));
    let top = ar[1].min(focus[1]) - vp[1] - 48.;
    let y = if top >= 8. {
        top
    } else {
        ar[1].max(focus[1]) + focus[3] - vp[1] + 8.
    };
    [x, y.clamp(8., (h - 48.).max(8.))]
}
fn dropdown_position(
    s: &State,
    vp: [f32; 4],
    w: f32,
    h: f32,
    mw: f32,
    mh: f32,
) -> Option<[f32; 2]> {
    if let Some(ar) = s.editor.active.selection_anchor_window_rect() {
        let [x, y] = toolbar_position(s, ar, vp, w, h);
        return Some([
            x.clamp(8., (w - mw - 8.).max(8.)),
            if y + 48. + mh < h - 8. {
                y + 48.
            } else {
                (y - mh - 8.).max(8.)
            },
        ]);
    }
    let br = *s
        .editor
        .active
        .block_rects
        .get(&s.editor.active.core.sel.head.block)?;
    let x = (br[0] - vp[0]).clamp(8., (w - mw - 8.).max(8.));
    let y = (br[1] - vp[1] + 28.).clamp(8., (h - mh - 8.).max(8.));
    Some([x, y])
}
fn dismiss_popover(s: &mut State, tx: &mut Tx) {
    s.editor.active.popover = None;
    s.editor.active.link_input.focused = false;
    tx.blur();
    tx.animated();
}
fn menu_surface(s: &State, key: &str, x: f32, y: f32, w: f32) -> El {
    crate::overlays::surface(
        s,
        col()
            .key(key)
            .absolute(x, y)
            .w(w)
            .padding(8.)
            .gap(2.)
            .role("menu")
            .focus_group(),
        12.,
    )
    .on_key(|s: &mut State, e, tx| {
        if e.key == Key::Escape {
            dismiss_popover(s, tx);
            true
        } else {
            false
        }
    })
}
fn menu_action(s: &State, key: &str, title: &str, icon: &'static Icon) -> El {
    row()
        .key(key)
        .w_full()
        .h(32.)
        .px(8.)
        .gap(10.)
        .radius(6.)
        .center_y()
        .hover(c(crate::ui::editor_colors(s.theme()).inset))
        .press(c(crate::ui::accent_soft(s)))
        .cursor(Cursor::Pointer)
        .role("button")
        .label(title)
        .focusable(true)
        .focus_ring(2., 0., c(crate::ui::accent(s)))
        .child(ico(icon, 16.).color(c(crate::ui::theme_muted(s.theme()))))
        .child(
            label(title, 13., 450., crate::ui::theme_ink(s.theme()))
                .min_w(0.)
                .truncate(),
        )
}
pub fn open_link(s: &mut State, tx: &mut Tx) {
    crate::inline_format::open(s, crate::inline_format::Menu::Link, tx);
}
fn apply_link(s: &mut State, tx: &mut Tx) {
    crate::inline_format::apply_link(s, tx);
}
fn render_selection_toolbar(
    s: &State,
    ar: [f32; 4],
    vp: [f32; 4],
    pane_w: f32,
    pane_h: f32,
) -> Option<El> {
    let core = &s.editor.active.core;
    if core.sel.head.block == TITLE_ID && core.sel.anchor.block == TITLE_ID {
        return None;
    }
    if core.is_code(core.sel.head.block) {
        return None;
    }
    Some(crate::inline_format::toolbar(
        s,
        toolbar_position(s, ar, vp, pane_w, pane_h),
    ))
}

fn render_popover(
    s: &State,
    pop: &DocPopover,
    vp: [f32; 4],
    pane_w: f32,
    pane_h: f32,
) -> Option<El> {
    match pop {
        DocPopover::BlockMenu { block_id } => {
            let bid = *block_id;
            let br = *s.editor.active.block_rects.get(&bid)?;
            let x = (br[0] - vp[0] - 28.).clamp(8., (pane_w - 247. - 8.).max(8.));
            let y = (br[1] - vp[1] + 28.).clamp(8., (pane_h - 410.).max(8.));
            let mut menu = menu_surface(s, "doc-block-menu", x, y, 247.).child(
                label(
                    s.tr("转换为"),
                    11.,
                    500.,
                    s.theme().choose(0x8f959e, 0x9297a3),
                )
                .px(8.)
                .py(4.),
            );
            let choices = [
                (s.tr("正文"), &TYPE, DocBlockKind::Text),
                (
                    s.tr("一级标题"),
                    &HEADING_1,
                    DocBlockKind::Heading { level: 1 },
                ),
                (
                    s.tr("二级标题"),
                    &HEADING_2,
                    DocBlockKind::Heading { level: 2 },
                ),
                (
                    s.tr("三级标题"),
                    &HEADING_3,
                    DocBlockKind::Heading { level: 3 },
                ),
                (s.tr("无序列表"), &LIST, DocBlockKind::Bullet),
                (
                    s.tr("有序列表"),
                    &LIST_ORDERED,
                    DocBlockKind::Ordered { start: None },
                ),
                (
                    s.tr("待办"),
                    &CHECK_SQUARE,
                    DocBlockKind::Todo { done: false },
                ),
                (s.tr("引用"), &QUOTE, DocBlockKind::Quote),
                (
                    s.tr("高亮块"),
                    &BULB,
                    DocBlockKind::Callout {
                        emoji: "💡".into(),
                        palette: "orange".into(),
                    },
                ),
                (
                    s.tr("代码块"),
                    &CODE,
                    DocBlockKind::Code {
                        lang: "Plain Text".into(),
                        wrap: true,
                    },
                ),
            ];
            let source = &s.editor.active.core.block(bid)?.kind;
            let choices: Vec<_> = choices
                .into_iter()
                .filter(|(_, _, kind)| {
                    source.has_text() || source.is_container() || kind.is_container()
                })
                .collect();
            for chunk in choices.chunks(4) {
                let mut grid = row().gap(4.).px(4.).mb(4.);
                for (title, icon, kind) in chunk {
                    let kind = kind.clone();
                    grid = grid.child(
                        el().key(format!("block-type-{title}"))
                            .size(53., 34.)
                            .center()
                            .radius(5.)
                            .fill(c(crate::ui::editor_colors(s.theme()).inset))
                            .hover(c(crate::ui::accent_soft(s)))
                            .cursor(Cursor::Pointer)
                            .role("button")
                            .label(*title)
                            .help(*title)
                            .on_press(move |s: &mut State| {
                                s.editor.active.core.turn_into(&[bid], kind.clone());
                                s.editor.active.popover = None;
                                sync_to_state(s);
                            })
                            .child(ico(icon, 18.).color(c(crate::ui::theme_muted(s.theme())))),
                    );
                }
                menu = menu.child(grid);
            }
            menu = menu.child(
                el().w_full().h(1.).my(3.).fill(c(s
                    .theme()
                    .choose(0xeef0f3, crate::ui::editor_colors(s.theme()).inset))),
            );
            for (key, title, icon) in [
                ("color", s.tr("颜色与高亮"), &TYPE),
                ("align", s.tr("段落对齐"), &ALIGN_LEFT),
                ("copy", s.tr("复制"), &COPY),
                ("cut", s.tr("剪切"), &SCISSORS),
                ("duplicate", s.tr("创建副本"), &COPY),
                ("add", s.tr("在下方添加"), &PLUS),
                ("delete", s.tr("删除块"), &TRASH_2),
            ] {
                menu = menu.child(
                    menu_action(s, &format!("block-{key}"), title, icon).on_press_tx(
                        move |s: &mut State, tx| {
                            if matches!(key, "color" | "align") {
                                let len = s
                                    .editor
                                    .active
                                    .core
                                    .block(bid)
                                    .map_or(0, |b| b.text.text.len());
                                s.editor.active.core.sel = DocSel {
                                    anchor: DocPos::new(bid, 0),
                                    head: DocPos::new(bid, len),
                                };
                                s.editor.active.popover = Some(if key == "color" {
                                    DocPopover::ColorMenu
                                } else {
                                    DocPopover::AlignMenu
                                });
                                return;
                            }
                            if matches!(key, "copy" | "cut") {
                                if let Some(b) = s.editor.active.core.block(bid) {
                                    tx.copy(crate::doc_core::block_plain_text(b));
                                }
                            }
                            match key {
                                "cut" | "delete" => s.editor.active.core.delete_block(bid),
                                "duplicate" => {
                                    s.editor.active.core.duplicate_block(bid);
                                }
                                "add" => s.editor.active.core.with_undo(|core| {
                                    let b = core.doc.new_block(DocBlockKind::Text, "");
                                    let id = core.insert_block_after(bid, b);
                                    core.set_head(DocPos::new(id, 0), false);
                                }),
                                _ => {}
                            }
                            s.editor.active.popover = None;
                            sync_to_state(s);
                        },
                    ),
                );
            }
            Some(menu)
        }
        DocPopover::AlignMenu
        | DocPopover::MoreMenu
        | DocPopover::LinkMenu
        | DocPopover::HeadingMenu
        | DocPopover::ColorMenu => {
            let menu =
                crate::inline_format::Menu::from_document(pop).expect("selection format menu");
            let [mw, mh] = menu.size();
            let position = dropdown_position(s, vp, pane_w, pane_h, mw, mh)?;
            Some(crate::inline_format::popover(s, menu, position))
        }
        DocPopover::CalloutStyle { block_id } => {
            let bid = *block_id;
            let br = s.editor.active.block_rects.get(&bid).copied().unwrap_or([
                vp[0] + 80.,
                vp[1] + 120.,
                300.,
                48.,
            ]);
            let lx = (br[0] - vp[0] + 12.).clamp(12., (pane_w - 272.).max(12.));
            let ly = (br[1] - vp[1] + 48.).clamp(12., (pane_h - 156.).max(12.));
            let mut emojis = row().w_full().gap(4.);
            for &em in CALLOUT_EMOJIS {
                emojis = emojis.child(
                    el().key(format!("callout-emoji-{em}"))
                        .size(26., 26.)
                        .role("button")
                        .label(em)
                        .focusable(true)
                        .radius(6.)
                        .center()
                        .hover(c(s
                            .theme()
                            .choose(0xf2f4f7, crate::ui::editor_colors(s.theme()).inset)))
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut State| {
                            s.editor.active.core.with_undo(|core| {
                                if let Some(b) = core.block_mut(bid)
                                    && let DocBlockKind::Callout { emoji, .. } = &mut b.kind
                                {
                                    *emoji = em.to_owned();
                                }
                            });
                            sync_to_state(s);
                        })
                        .child(
                            text(em)
                                .font(font(15., 400.).lh(18.))
                                .color(c(s.theme().choose(0x1f2329, 0xe6e7eb))),
                        ),
                );
            }
            let mut swatches = row().w_full().gap(6.);
            for &(pal_key, tip) in CALLOUT_PALETTES {
                let (fill_h, border_h) = callout_palette_colors(pal_key, s.theme());
                swatches = swatches.child(
                    el().key(format!("callout-palette-{pal_key}"))
                        .size(24., 24.)
                        .role("button")
                        .label(s.tr(tip))
                        .focusable(true)
                        .radius(12.)
                        .fill(c(fill_h))
                        .border(1.5, c(border_h))
                        .cursor(Cursor::Pointer)
                        .help(s.tr(tip))
                        .on_press(move |s: &mut State| {
                            s.editor.active.core.with_undo(|core| {
                                if let Some(b) = core.block_mut(bid)
                                    && let DocBlockKind::Callout { palette, .. } = &mut b.kind
                                {
                                    *palette = pal_key.to_owned();
                                }
                            });
                            s.editor.active.popover = None;
                            sync_to_state(s);
                        }),
                );
            }
            Some(
                menu_surface(s, "doc-callout-popover", lx, ly, 260.)
                    .padding(12.)
                    .gap(10.)
                    .child(label(
                        s.tr("图标"),
                        11.,
                        500.,
                        s.theme().choose(0x8f959e, 0x9297a3),
                    ))
                    .child(emojis)
                    .child(label(
                        s.tr("背景配色"),
                        11.,
                        500.,
                        s.theme().choose(0x8f959e, 0x9297a3),
                    ))
                    .child(swatches),
            )
        }
        DocPopover::CodeLang { block_id } => {
            let bid = *block_id;
            let br = s.editor.active.block_rects.get(&bid).copied().unwrap_or([
                vp[0] + 80.,
                vp[1] + 120.,
                300.,
                60.,
            ]);
            let lx = (br[0] - vp[0] + 12.).clamp(12., (pane_w - 192.).max(12.));
            let ly = (br[1] - vp[1] + 40.).clamp(12., (pane_h - 312.).max(12.));
            let mut menu = menu_surface(s, "doc-codelang-popover", lx, ly, 180.);
            for &lang_name in CODE_LANGS {
                menu = menu.child(
                    row()
                        .key(format!("code-language-{lang_name}"))
                        .role("button")
                        .label(lang_name)
                        .focusable(true)
                        .w_full()
                        .h(30.)
                        .px(8.)
                        .radius(6.)
                        .center_y()
                        .hover(c(s
                            .theme()
                            .choose(0xf2f4f7, crate::ui::editor_colors(s.theme()).inset)))
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut State| {
                            s.editor.active.core.with_undo(|core| {
                                if let Some(b) = core.block_mut(bid)
                                    && let DocBlockKind::Code { lang, .. } = &mut b.kind
                                {
                                    *lang = lang_name.to_owned();
                                }
                            });
                            s.editor.active.popover = None;
                            sync_to_state(s);
                        })
                        .child(label(
                            lang_name,
                            12.,
                            480.,
                            s.theme().choose(0x1f2329, 0xe6e7eb),
                        )),
                );
            }
            Some(menu)
        }
    }
}

const CARD_GRIP_WIDTH: f32 = 18.;
// Keep the grip past the card's 4 pt inner drag edge.
pub const CARD_TEXT_INSET: f32 = 5. + CARD_GRIP_WIDTH + 2.;

fn canvas_heading_colors(blocks: &[DocBlock], theme: crate::model::Theme) -> (rune::Color, rune::Color) {
    let (light, dark, paper) = blocks.iter().find_map(|block| match block.kind {
        DocBlockKind::Todo { .. } => Some((0x466951, 0x9cc5a6, 0xe6efe6)),
        DocBlockKind::Table(_) => Some((0x396c73, 0x95cad0, 0xe0eeed)),
        DocBlockKind::Code { .. } => Some((0x5c6397, 0xb1b9ed, 0xe8eafa)),
        DocBlockKind::Quote | DocBlockKind::Callout { .. } => Some((0x92673e, 0xdab78f, 0xf5eadb)),
        DocBlockKind::Image { .. } => Some((0x946677, 0xd4a4b5, 0xf4e6ec)),
        _ => None,
    }).unwrap_or((0x786494, 0xc1aed7, 0xeee8f5));
    let ink = c(theme.choose(light, dark));
    let paper = if theme.is_dark() { c(0x2b2a30).mix(ink, 0.10) } else { c(paper) };
    (ink, paper)
}

/// The same block tree as a structured page, inside a movable canvas boundary.
pub fn render_embedded(s: &State, b: &crate::model::Block) -> El {
    let Some(session) = crate::canvas_document::session(s, &b.id) else {
        return label(
            s.tr("无法读取笔记"),
            14.,
            400.,
            crate::ui::theme_muted(s.theme()),
        );
    };
    let view = RenderState {
        state: s,
        editor: SessionView { active: session },
        card_id: Some(&b.id),
        blank_canvas_input: s.draft.as_deref() == Some(&b.id),
    };
    let width = (b.w - CARD_TEXT_INSET - 14.).max(1.);
    let owner = b.id.clone();
    let layout_owner = owner.clone();
    let hover_owner = owner.clone();
    let leave_owner = owner.clone();
    let (visible_ids, mut rows): (Vec<_>, Vec<_>) =
        render_block_list(&view, &session.core.doc.blocks, 0, width, false)
            .into_iter()
            .unzip();
    if !view.blank_canvas_input
        && session.core.doc.blocks.first().is_some_and(|b| matches!(b.kind, DocBlockKind::Heading { .. }))
        && let Some(heading) = rows.first_mut()
    {
        let (ink, paper) = canvas_heading_colors(&session.core.doc.blocks, s.theme());
        let surface = c(s.theme().choose(0xffffff, 0x2b2a30));
        let title = std::mem::replace(heading, el());
        *heading = col().w(b.w - 2.).ml(1. - CARD_TEXT_INSET).mt(-8.)
            .pl(CARD_TEXT_INSET - 1.).pr(13.).pt(9.).pb(8.)
            .corners([9., 9., 0., 0.])
            .gradient(paper, paper.mix(surface, 0.32))
            .border_sides([0., 0., 0.7, 0.], ink.alpha(0.20))
            .child(title);
    }
    col()
        .key(format!("body-{}", b.id))
        .w(b.w)
        .pl(CARD_TEXT_INSET)
        .pr(14.)
        .py(9.)
        .min_h(b.min_h.max(40.))
        .cursor(Cursor::Text)
        .children(rows)
        .on_drag(move |s: &mut State, drag, tx| {
            if drag.phase == Phase::Down {
                crate::canvas_document::activate(s, &owner);
            }
            handle_doc_stage_drag(s, drag, tx);
        })
        .on_pointer_move_local(move |s: &mut State, p, tx| {
            let Some(session) = crate::canvas_document::session_mut(s, &hover_owner) else {
                return;
            };
            if session.drag.is_some() {
                tx.live_only();
                return;
            }
            let next = visible_ids.iter().copied().find(|id| {
                session
                    .block_rects
                    .get(id)
                    .is_some_and(|r| p.window_y >= r[1] && p.window_y < r[1] + r[3])
            });
            if session.hover_block != next || session.typing {
                session.hover_block = next;
                session.typing = false;
            } else {
                tx.live_only();
            }
        })
        .on_hover_change(move |s: &mut State, inside, tx| {
            if !inside
                && let Some(session) = crate::canvas_document::session_mut(s, &leave_owner)
                && session.hover_block.take().is_some()
            {
                tx.instant();
            } else {
                tx.live_only();
            }
        })
        .on_drawn_layout_tx(move |s: &mut State, r, tx| {
            if let Some(session) = crate::canvas_document::session_mut(s, &layout_owner) {
                session.page_rect = Some(r);
            }
            tx.live_only();
        })
}

pub fn canvas_overlays(s: &State) -> Option<El> {
    if s.mode != crate::model::WorkspaceMode::Canvas
        || s.editor.canvas_owner().is_none()
        || !s.editor.active.focused
    {
        return None;
    }
    let vp = s.editor.active.viewport_rect?;
    Some(
        el().key("canvas-document-overlays")
            .absolute(vp[0], vp[1])
            .size(vp[2], vp[3])
            .children(render_overlays(s, vp[2], vp[3])?),
    )
}
