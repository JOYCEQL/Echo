//! Structured document tables: native multiline inputs and document-level history.
use crate::{
    doc_core::{DocBlockKind, DocPos},
    doc_editor::sync_to_state,
    model::State,
    table_data::TableData,
    ui::{c, font, label},
};
use rune::{
    Align, Cursor, El, Key, KeyEvent, Phase, PopupItem, PopupMenu, TextEdit, Tx, col, el, row,
    text_input,
};

#[derive(Clone, Copy, Debug)]
pub struct CellSelection {
    pub block: u64,
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}
impl CellSelection {
    fn contains(self, r: usize, c: usize) -> bool {
        r >= self.anchor.0.min(self.head.0)
            && r <= self.anchor.0.max(self.head.0)
            && c >= self.anchor.1.min(self.head.1)
            && c <= self.anchor.1.max(self.head.1)
    }
}
fn data(s: &State, id: u64) -> Option<&TableData> {
    match &s.editor.active.core.block(id)?.kind {
        DocBlockKind::Table(t) => Some(t),
        _ => None,
    }
}
fn mutate(s: &mut State, id: u64, f: impl FnOnce(&mut TableData)) {
    crate::canvas_document::activate_block(s, id);
    s.editor.active.commit_cell_edit();
    s.editor.active.core.with_undo(|core| {
        if let Some(b) = core.block_mut(id)
            && let DocBlockKind::Table(t) = &mut b.kind
        {
            f(t);
        }
    });
    sync_to_state(s);
}
/// Keep cell source in Markdown while letting Rune select and edit the visible text.
pub(super) fn cell_input(source: &str) -> TextEdit {
    if !source.contains('`') {
        return TextEdit::new(source);
    }
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut end = 0;
    for (event, range) in pulldown_cmark::Parser::new(source).into_offset_iter() {
        if let pulldown_cmark::Event::Code(code) = event {
            text.push_str(&source[end..range.start]);
            let start = text.len();
            text.push_str(&code);
            marks.push(rune::MarkRange::new(start..text.len(), rune::Mark::Code));
            end = range.end;
        }
    }
    text.push_str(&source[end..]);
    let mut input = TextEdit::new(text);
    input.set_marks(marks);
    input
}

pub(super) fn cell_markdown(input: &TextEdit) -> String {
    let mut source = String::new();
    let mut end = 0;
    for mark in input.marks().iter().filter(|m| m.mark == rune::Mark::Code) {
        source.push_str(&input.text[end..mark.start]);
        let code = &input.text[mark.start..mark.end];
        // Keep native paragraph breaks outside code spans. CommonMark turns
        // newlines inside a span into spaces when the table is opened again.
        for (i, part) in code.split('\n').enumerate() {
            if i > 0 { source.push('\n'); }
            if part.is_empty() { continue; }
            let longest = part.split(|c| c != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest + 1);
            let pad = part.starts_with('`') || part.ends_with('`')
                || (part.starts_with(' ') && part.ends_with(' ') && !part.trim_matches(' ').is_empty());
            source.push_str(&fence);
            if pad { source.push(' '); }
            source.push_str(part);
            if pad { source.push(' '); }
            source.push_str(&fence);
        }
        end = mark.end;
    }
    source.push_str(&input.text[end..]);
    source
}

fn activate_cell(s: &mut State, id: u64, r: usize, c: usize) -> bool {
    crate::canvas_document::activate_block(s, id);
    s.editor.active.commit_cell_edit();
    let Some(value) = data(s, id)
        .and_then(|t| t.rows.get(r))
        .and_then(|row| row.get(c))
        .cloned()
    else {
        return false;
    };
    let input_key = format!("{}:{id}:{r}:{c}", s.editor.active.note_id);
    s.editor
        .active
        .cell_inputs
        .insert(input_key.clone(), cell_input(&value));
    s.editor.active.editing_cell = Some((id, r, c, input_key));
    s.editor.active.table_selection = Some(CellSelection {
        block: id,
        anchor: (r, c),
        head: (r, c),
    });
    s.editor.active.core.set_head(DocPos::new(id, 0), false);
    s.editor.active.focused = true;
    s.editor.active.popover = None;
    true
}
pub fn begin(s: &mut State, id: u64, r: usize, c: usize, tx: &mut Tx) {
    if activate_cell(s, id, r, c) {
        tx.focus(format!("doc-cell-{id}-{r}-{c}"), false);
    }
}
fn extend_cells(s: &mut State, id: u64, r: usize, c: usize, shift: bool, tx: &mut Tx) -> bool {
    if !shift {
        return false;
    }
    crate::canvas_document::activate_block(s, id);
    if !s
        .editor
        .active
        .table_selection
        .is_some_and(|sel| sel.block == id)
    {
        return false;
    }
    let editing_here = s
        .editor
        .active
        .editing_cell
        .as_ref()
        .is_some_and(|(bid, row, col, _)| (*bid, *row, *col) == (id, r, c));
    if editing_here {
        return false;
    }
    s.editor.active.commit_cell_edit();
    s.editor.active.table_selection.as_mut().unwrap().head = (r, c);
    tx.blur();
    true
}
pub fn key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if let Some((id, r, c, _)) = &s.editor.active.editing_cell {
        let (id, r, c) = (*id, *r, *c);
        if e.is_command('z') {
            s.editor.active.commit_cell_edit();
            if e.mods.shift {
                s.editor.active.core.redo();
            } else {
                s.editor.active.core.undo();
            }
            sync_to_state(s);
            tx.blur();
            return true;
        }
        if e.key == Key::Escape || (e.key == Key::Enter && e.mods.cmd) {
            s.editor.active.commit_cell_edit();
            sync_to_state(s);
            tx.blur();
            return true;
        }
        if e.key == Key::Tab {
            s.editor.active.commit_cell_edit();
            let Some(t) = data(s, id) else { return true };
            let cols = t.cols();
            let len = t.rows.len() * cols;
            let next = if e.mods.shift {
                (r * cols + c).saturating_sub(1)
            } else {
                r * cols + c + 1
            };
            if next == len {
                mutate(s, id, |t| t.rows.push(vec![String::new(); cols]));
            }
            begin(s, id, next / cols, next % cols, tx);
            return true;
        }
        return false;
    }
    let Some(sel) = s.editor.active.table_selection else {
        return false;
    };
    if e.is_command('c') || e.is_command('x') {
        if let Some(t) = data(s, sel.block) {
            tx.copy(copy_region(t, sel));
        }
        if e.is_command('x') {
            clear(s, sel);
        }
        return true;
    }
    if e.is_command('v') {
        tx.read_clipboard(move |s: &mut State, content, _| {
            paste(
                s,
                sel.block,
                sel.anchor.0.min(sel.head.0),
                sel.anchor.1.min(sel.head.1),
                content.text(),
            )
        });
        return true;
    }
    if matches!(e.key, Key::Left | Key::Right | Key::Up | Key::Down) {
        let Some(t) = data(s, sel.block) else {
            return false;
        };
        let mut head = sel.head;
        match e.key {
            Key::Left => head.1 = head.1.saturating_sub(1),
            Key::Right => head.1 = (head.1 + 1).min(t.cols() - 1),
            Key::Up => head.0 = head.0.saturating_sub(1),
            Key::Down => head.0 = (head.0 + 1).min(t.rows.len() - 1),
            _ => {}
        }
        s.editor.active.table_selection = Some(CellSelection {
            block: sel.block,
            anchor: if e.mods.shift { sel.anchor } else { head },
            head,
        });
        return true;
    }
    if let Key::Text(value) = &e.key
        && !e.mods.cmd
        && !e.mods.ctrl
    {
        begin(s, sel.block, sel.head.0, sel.head.1, tx);
        if let Some((_, _, _, key)) = &s.editor.active.editing_cell {
            s.editor
                .active
                .cell_inputs
                .get_mut(key)
                .expect("active cell editor")
                .insert(value);
        }
        s.editor.active.flush_cell_edit();
        sync_to_state(s);
        return true;
    }
    if e.key == Key::Backspace || e.key == Key::Delete {
        clear(s, sel);
        return true;
    }
    if e.key == Key::Enter {
        begin(s, sel.block, sel.head.0, sel.head.1, tx);
        return true;
    }
    if e.key == Key::Escape {
        s.editor.active.table_selection = None;
        return true;
    }
    false
}
fn clear(s: &mut State, sel: CellSelection) {
    mutate(s, sel.block, |t| {
        for (r, row) in t.rows.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                if sel.contains(r, c) {
                    v.clear();
                }
            }
        }
    });
}
fn copy_region(t: &TableData, sel: CellSelection) -> String {
    t.rows
        .iter()
        .enumerate()
        .filter(|(r, _)| *r >= sel.anchor.0.min(sel.head.0) && *r <= sel.anchor.0.max(sel.head.0))
        .map(|(_, row)| {
            row.iter()
                .enumerate()
                .filter(|(c, _)| {
                    *c >= sel.anchor.1.min(sel.head.1) && *c <= sel.anchor.1.max(sel.head.1)
                })
                .map(|(_, v)| v.replace('"', "\"\""))
                .map(|v| {
                    if v.contains(['\n', '\t', '"']) {
                        format!("\"{v}\"")
                    } else {
                        v
                    }
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![vec![String::new()]];
    let mut quoted = false;
    let mut chars = text.trim_end_matches(['\r', '\n']).chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                rows.last_mut().unwrap().last_mut().unwrap().push('"');
            }
            '"' if quoted => quoted = false,
            '"' if rows.last().unwrap().last().unwrap().is_empty() => quoted = true,
            '\t' if !quoted => rows.last_mut().unwrap().push(String::new()),
            '\n' if !quoted => rows.push(vec![String::new()]),
            '\r' if !quoted => {}
            _ => rows.last_mut().unwrap().last_mut().unwrap().push(ch),
        }
    }
    rows
}
pub fn paste(s: &mut State, id: u64, r: usize, c: usize, text: &str) {
    if text.is_empty() {
        return;
    }
    let values = parse_tsv(text);
    mutate(s, id, |t| {
        let cols = t.cols().max(c + values.iter().map(Vec::len).max().unwrap());
        for row in &mut t.rows {
            row.resize(cols, String::new());
        }
        t.widths.resize(cols, 1.);
        t.align.resize(cols, 0);
        while t.rows.len() < r + values.len() {
            t.rows.push(vec![String::new(); cols]);
        }
        for (ri, row) in values.into_iter().enumerate() {
            for (ci, v) in row.into_iter().enumerate() {
                t.rows[r + ri][c + ci] = v;
            }
        }
    });
}
fn sizes(t: &TableData, w: f32, min_col: f32) -> Vec<f32> {
    let weights: Vec<f32> = (0..t.cols())
        .map(|i| t.widths.get(i).copied().unwrap_or(1.).max(0.01))
        .collect();
    let total: f32 = weights.iter().sum();
    weights.iter().map(|v| (w * v / total).max(min_col)).collect()
}
pub fn render(
    s: &crate::doc_editor::RenderState<'_>,
    id: u64,
    t: &TableData,
    width: f32,
    table_selected: bool,
) -> El {
    let compact = s.in_canvas();
    let cell_line = if compact { 19. } else { 22. };
    let min_col = if compact { 72. } else { 96. };
    let widths = sizes(t, width - 2., min_col);
    let total: f32 = widths.iter().sum();
    let mut grid = col()
        .w(total)
        .border(if compact { 0.7 } else { 1. }, c(crate::ui::editor_colors(s.theme()).border));
    for (r, values) in t.rows.iter().enumerate() {
        let mut line = row().w(total);
        for (ci, value) in values.iter().enumerate() {
            let active = s
                .editor
                .active
                .editing_cell
                .as_ref()
                .is_some_and(|(bid, rr, cc, _)| (*bid, *rr, *cc) == (id, r, ci));
            let selected = s
                .editor
                .active
                .table_selection
                .is_some_and(|sel| sel.block == id && sel.contains(r, ci));
            let style = font(if compact { 13. } else { 14. }, if r == 0 { 500. } else { 400. }).lh(cell_line);
            let align = match t.align.get(ci) {
                Some(1) => Align::Center,
                Some(2) => Align::End,
                _ => Align::Start,
            };
            let mut cell = col()
                .key(format!("doc-cell-box-{id}-{r}-{ci}"))
                .w(widths[ci])
                .min_h(if compact { 28. } else { 42. })
                .px(if compact { 8. } else { 12. })
                .py(if compact { 4. } else { 9. })
                .fill(c(if table_selected || (selected && !active) {
                    crate::ui::accent_soft(s)
                } else if compact && r == 0 {
                    s.theme().choose(0xe5eeeb, 0x303e3a)
                } else if compact && r % 2 == 0 {
                    s.theme().choose(0xf5f8f6, 0x2d3431)
                } else if r == 0 {
                    crate::ui::editor_colors(s.theme()).inset
                } else {
                    crate::ui::editor_colors(s.theme()).page
                }))
                .border_sides(
                    [
                        if r == 0 { 1. } else { 0. },
                        if compact { 0. } else { 1. },
                        if compact { 0.6 } else { 1. },
                        if ci == 0 { 1. } else { 0. },
                    ],
                    c(crate::ui::editor_colors(s.theme()).border),
                )
                .cursor(Cursor::Text)
                .on_press_tx(move |s: &mut State, tx| {
                    if !extend_cells(s, id, r, ci, tx.mods().shift, tx) {
                        begin(s, id, r, ci, tx);
                    }
                })
                .on_secondary_press(move |s: &mut State, press, tx| {
                    crate::canvas_document::activate_block(s, id);
                    s.editor.active.commit_cell_edit();
                    tx.blur();
                    let menu = PopupMenu::new()
                        .item(PopupItem::new(
                            "select",
                            s.tr("选择单元格（Shift 点击扩选）"),
                        ))
                        .separator()
                        .item(PopupItem::new("row-before", s.tr("在上方插入行")))
                        .item(PopupItem::new("row-after", s.tr("在下方插入行")))
                        .item(PopupItem::new("col-before", s.tr("在左侧插入列")))
                        .item(PopupItem::new("col-after", s.tr("在右侧插入列")))
                        .separator()
                        .item(PopupItem::new("del-row", s.tr("删除行")))
                        .item(PopupItem::new("del-col", s.tr("删除列")));
                    tx.popup_menu(menu, press, move |s: &mut State, picked, _| {
                        let Some(action) = picked else { return };
                        if action == "select" {
                            s.editor.active.table_selection = Some(CellSelection {
                                block: id,
                                anchor: (r, ci),
                                head: (r, ci),
                            });
                            return;
                        }
                        mutate(s, id, |t| match action {
                            "row-before" | "row-after" => t.rows.insert(
                                r + usize::from(action == "row-after"),
                                vec![String::new(); t.cols()],
                            ),
                            "col-before" | "col-after" => {
                                let pos = ci + usize::from(action == "col-after");
                                let cols = t.cols();
                                t.widths.resize(cols, 1.);
                                t.align.resize(cols, 0);
                                for row in &mut t.rows {
                                    row.insert(pos, String::new());
                                }
                                t.widths.insert(pos, 1.);
                                t.align.insert(pos, 0);
                            }
                            "del-row" if t.rows.len() > 1 => {
                                t.rows.remove(r);
                            }
                            "del-col" if t.cols() > 1 => {
                                let cols = t.cols();
                                t.widths.resize(cols, 1.);
                                t.align.resize(cols, 0);
                                for row in &mut t.rows {
                                    row.remove(ci);
                                }
                                t.widths.remove(ci);
                                t.align.remove(ci);
                            }
                            _ => {}
                        });
                        s.editor.active.table_selection = None;
                    });
                });
            let input_key = format!("{}:{id}:{r}:{ci}", s.editor.active.note_id);
            let idle;
            let ed = if active {
                &s.editor.active.cell_inputs[&input_key]
            } else {
                idle = cell_input(value);
                &idle
            };
            let field = text_input(ed, style)
                .align(align)
                .wrap_anywhere()
                .key(format!("doc-cell-{id}-{r}-{ci}"))
                .label(if s.language.is_english() {
                    format!("Row {}, column {}", r + 1, ci + 1)
                } else {
                    format!("第 {} 行，第 {} 列", r + 1, ci + 1)
                })
                .tab_index(-1)
                .w_full()
                .min_h(cell_line)
                .color(c(crate::ui::theme_ink(s.theme())))
                .caret(c(crate::ui::accent(s)), c(crate::ui::accent(s)).alpha(0.32))
                .on_input_key(key)
                .on_submit(|s: &mut State, _| {
                    if let Some((_, _, _, key)) = &s.editor.active.editing_cell {
                        s.editor
                            .active
                            .cell_inputs
                            .get_mut(key)
                            .expect("live cell editor")
                            .insert("\n");
                    }
                    s.editor.active.flush_cell_edit();
                    sync_to_state(s);
                })
                .on_edit(move |s: &mut State| {
                    crate::canvas_document::session_for_block(s, id)
                        .expect("editing mounted cell")
                        .cell_inputs
                        .get_mut(&input_key)
                        .expect("cell editor retained through native blur")
                })
                .on_input_change(|s: &mut State, _| {
                    s.editor.active.flush_cell_edit();
                    sync_to_state(s);
                })
                .on_input_paste(move |s: &mut State, content, tx| {
                    if content.text().contains('\t') {
                        paste(s, id, r, ci, content.text());
                        begin(s, id, r, ci, tx);
                        true
                    } else {
                        false
                    }
                });
            cell = cell.child(if active {
                field
            } else {
                field.edit_on_pointer(move |s: &mut State, shift, tx| {
                    if extend_cells(s, id, r, ci, shift, tx) {
                        return false;
                    }
                    activate_cell(s, id, r, ci)
                })
            });
            if ci + 1 < widths.len() {
                let initial = widths.clone();
                cell = cell.child(
                    el().key(format!("doc-table-resize-{id}-{r}-{ci}"))
                        .absolute(widths[ci] - 8., 0.)
                        .w(8.)
                        .h_full()
                        .cursor(Cursor::ResizeColumn)
                        .on_drag(move |s: &mut State, d, tx| match d.phase {
                            Phase::Down => {
                                crate::canvas_document::activate_block(s, id);
                                s.editor.active.commit_cell_edit();
                                s.editor.active.core.with_undo(|_| {});
                                s.editor.active.table_resize = Some((id, ci, initial.clone()));
                                tx.blur();
                            }
                            Phase::Move => {
                                if let Some((bid, col, ws)) = &s.editor.active.table_resize {
                                    let mut next = ws.clone();
                                    let delta = d.dx.clamp(min_col - ws[*col], ws[*col + 1] - min_col);
                                    next[*col] += delta;
                                    next[*col + 1] -= delta;
                                    if let Some(b) = s.editor.active.core.block_mut(*bid)
                                        && let DocBlockKind::Table(t) = &mut b.kind
                                    {
                                        t.widths = next;
                                        s.editor.active.core.dirty = true;
                                    }
                                }
                            }
                            Phase::Up => {
                                if d.cancelled()
                                    && let Some((bid, _, original)) =
                                        s.editor.active.table_resize.take()
                                    && let Some(b) = s.editor.active.core.block_mut(bid)
                                    && let DocBlockKind::Table(t) = &mut b.kind
                                {
                                    t.widths = original;
                                }
                                s.editor.active.table_resize = None;
                                sync_to_state(s);
                            }
                        }),
                );
            }
            line = line.child(cell);
        }
        grid = grid.child(line);
    }
    col()
        .w(width)
        .gap(4.)
        .child(el().w(width).scroll_x().child(grid))
        .child(
            row()
                .gap(4.)
                .child(
                    el().key(format!("doc-table-add-row-{id}"))
                        .px(10.)
                        .h(26.)
                        .radius(5.)
                        .center()
                        .hover(c(s
                            .theme()
                            .choose(0xf2f3f5, crate::ui::editor_colors(s.theme()).inset)))
                        .cursor(Cursor::Pointer)
                        .role("button")
                        .label(s.tr("添加行"))
                        .on_press_tx(move |s: &mut State, tx| {
                            crate::canvas_document::activate_block(s, id);
                            mutate(s, id, |t| t.rows.push(vec![String::new(); t.cols()]));
                            tx.blur();
                        })
                        .child(label(
                            s.tr("＋ 行"),
                            12.,
                            450.,
                            s.theme().choose(0x8f959e, 0x9297a3),
                        )),
                )
                .child(
                    el().key(format!("doc-table-add-col-{id}"))
                        .px(10.)
                        .h(26.)
                        .radius(5.)
                        .center()
                        .hover(c(s
                            .theme()
                            .choose(0xf2f3f5, crate::ui::editor_colors(s.theme()).inset)))
                        .cursor(Cursor::Pointer)
                        .role("button")
                        .label(s.tr("添加列"))
                        .on_press_tx(move |s: &mut State, tx| {
                            crate::canvas_document::activate_block(s, id);
                            mutate(s, id, |t| {
                                let cols = t.cols();
                                t.widths.resize(cols, 1.);
                                t.align.resize(cols, 0);
                                for row in &mut t.rows {
                                    row.push(String::new());
                                }
                                t.widths.push(1.);
                                t.align.push(0);
                            });
                            tx.blur();
                        })
                        .child(label(
                            s.tr("＋ 列"),
                            12.,
                            450.,
                            s.theme().choose(0x8f959e, 0x9297a3),
                        )),
                ),
        )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_cell_codes_survive_native_edits_and_reopening() {
        let mut edit = cell_input("应用 `Echo.app` 或 ``a`b``。");
        assert_eq!(edit.text, "应用 Echo.app 或 a`b。");
        assert_eq!(edit.marks().len(), 2);
        edit.set_caret("应用 Echo".len());
        edit.insert("桌面");
        let saved = cell_markdown(&edit);
        assert_eq!(saved, "应用 `Echo桌面.app` 或 ``a`b``。");
        let restored = cell_input(&saved);
        assert_eq!(restored.text, edit.text);
        assert_eq!(restored.marks(), edit.marks());
        let literal = r"字面上的 \` 和未闭合的 `";
        assert_eq!(cell_input(literal).text, literal);
    }

    #[test]
    fn multiline_clipboard_roundtrip() {
        let t = TableData {
            rows: vec![vec![
                "第一行\n第二行".into(),
                "a\tb".into(),
                "他说\"你好\"".into(),
            ]],
            widths: vec![],
            align: vec![],
        };
        let text = copy_region(
            &t,
            CellSelection {
                block: 1,
                anchor: (0, 0),
                head: (0, 2),
            },
        );
        assert_eq!(parse_tsv(&text), t.rows);
    }
    #[test]
    fn unquoted_literal_quotes_survive() {
        assert_eq!(
            parse_tsv("他说\"你好\"\t完成"),
            vec![vec!["他说\"你好\"", "完成"]]
        );
    }
    #[test]
    fn minimum_column_width() {
        let w = sizes(&TableData::default(), 180., 96.);
        assert!(w.iter().all(|w| *w >= 96.));
    }
}
