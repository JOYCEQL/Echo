//! Shared page navigation for both document and canvas hosts.
use crate::{
    icons::*,
    model::{NavDrag, Note, RenameTarget, State},
    ui::{c, label, rename_input},
};
use rune::{Cursor, El, Phase, col, el, row};
fn nav_drop_slot(s: &State, key: &str, inset: f32, width: f32) -> El {
    el().key(key)
        .absolute(inset, -2.)
        .size((width - inset - 6.).max(0.), 2.)
        .radius(1.)
        .fill(c(crate::ui::accent(s)))
        .motion(rune::Spring::NONE)
        .transition(rune::Transition::none())
        .raise()
        .inert()
        .child(
            el().absolute(-2., -2.)
                .size(6., 6.)
                .radius(3.)
                .fill(c(crate::ui::accent(s))),
        )
}

pub(crate) fn render_nav_drag_ghost(
    s: &State,
    width: f32,
    height: f32,
    ink: u32,
    muted: u32,
) -> Option<El> {
    let (pointer, title, marker, hint) = match s.nav_drag.as_ref()? {
        NavDrag::Book {
            source_idx,
            pointer,
            ..
        } => {
            let book = s.book_at(*source_idx);
            (
                *pointer,
                s.book_name(book).to_owned(),
                crate::notebook_icons::render(s, book, 20., s.book == book, muted),
                s.tr("松开以调整顺序").to_owned(),
            )
        }
        NavDrag::Note {
            note_id,
            pointer,
            drop_into_book,
            valid_drop,
            ..
        } => {
            let note = s.notes.iter().find(|n| &n.id == note_id)?;
            let title = if note.title.trim().is_empty() {
                s.tr("无标题文档").to_owned()
            } else {
                note.title.clone()
            };
            let hint = if let Some(book) = drop_into_book {
                format!("{} {}", s.tr("移动到"), s.book_name(book))
            } else if *valid_drop {
                s.tr("松开以调整顺序").to_owned()
            } else {
                s.tr("拖至空间或列表内").to_owned()
            };
            (
                *pointer,
                title,
                crate::navigation::page_marker(s, note.format),
                hint,
            )
        }
    };
    let ghost_width = 236_f32.min((width - 16.).max(0.));
    Some(
        row()
            .key("nav-drag-ghost")
            .absolute(
                (pointer[0] + 14.).clamp(8., (width - ghost_width - 8.).max(8.)),
                (pointer[1] + 16.).clamp(8., (height - 60.).max(8.)),
            )
            .size(ghost_width, 52.)
            .px(12.)
            .gap(9.)
            .radius(10.)
            .center_y()
            .fill(
                c(s.theme()
                    .choose(0xffffff, crate::ui::editor_colors(s.theme()).raised))
                .alpha(0.98),
            )
            .border(1., c(ink).alpha(0.10))
            .shadow(6., 18., c(0x000000).alpha(0.16))
            .motion(rune::Spring::NONE)
            .transition(rune::Transition::none())
            .raise()
            .inert()
            .child(marker)
            .child(
                col()
                    .grow(1.)
                    .min_w(0.)
                    .gap(3.)
                    .child(label(title, 13., 540., ink).truncate())
                    .child(label(hint, 11., 450., muted).truncate()),
            ),
    )
}

fn note_inset(depth: usize) -> f32 {
    depth.min(6) as f32 * 14.
}
struct PageRow {
    index: usize,
    depth: usize,
    sibling_index: usize,
    siblings: std::rc::Rc<Vec<String>>,
    has_children: bool,
}
pub(crate) fn page_tree(s: &State, width: f32) -> El {
    use std::{
        collections::{HashMap, HashSet},
        rc::Rc,
    };
    let ids: HashSet<&str> = s
        .notes
        .iter()
        .filter(|n| n.notebook == s.book)
        .map(|n| n.id.as_str())
        .collect();
    let mut children: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (index, note) in s
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.notebook == s.book)
    {
        let parent = note.parent_id.as_deref().filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(index);
    }
    fn flatten(
        s: &State,
        children: &HashMap<Option<&str>, Vec<usize>>,
        parent: Option<&str>,
        depth: usize,
        out: &mut Vec<PageRow>,
    ) {
        let Some(indices) = children.get(&parent) else {
            return;
        };
        let siblings = Rc::new(indices.iter().map(|&i| s.notes[i].id.clone()).collect());
        for (sibling_index, &index) in indices.iter().enumerate() {
            let note = &s.notes[index];
            let has_children = children.contains_key(&Some(note.id.as_str()));
            out.push(PageRow {
                index,
                depth,
                sibling_index,
                siblings: Rc::clone(&siblings),
                has_children,
            });
            if has_children && depth < 6 && s.tree_expanded.contains(&note.id) {
                flatten(s, children, Some(&note.id), depth + 1, out);
            }
        }
    }
    let mut rows = Vec::new();
    flatten(s, &children, None, 0, &mut rows);
    let rows = Rc::new(rows);
    let keys = rows.clone();
    let visible_rows = rows.clone();
    let count = rows.len();
    col()
        .key("book-items")
        .w_full()
        .growth(rune::GrowthSpec {
            spring: rune::Spring::NONE,
            ..Default::default()
        })
        .on_visible_rows(move |s: &mut State, range| {
            let visible: std::collections::HashSet<String> = visible_rows[range.start..range.end]
                .iter()
                .map(|r| format!("note:{}", s.notes[r.index].id))
                .collect();
            s.nav_row_rects
                .retain(|key, _| !key.starts_with("note:") || visible.contains(key));
            false
        })
        .virtual_rows(
            count,
            crate::navigation::PAGE_ROW_PITCH,
            move |s: &State, i| format!("page-row-{}", s.notes[keys[i].index].id).into(),
            move |s: &State, i| {
                let row = &rows[i];
                render_page_row(s, &s.notes[row.index], row, width)
            },
        )
}

pub(crate) fn children_of<'a>(s: &'a State, book: &str, parent_id: Option<&str>) -> Vec<&'a Note> {
    let children: Vec<_> = s
        .notes
        .iter()
        .filter(|n| {
            if n.notebook != book {
                return false;
            }
            match parent_id {
                Some(pid) => n.parent_id.as_deref() == Some(pid),
                None => match n.parent_id.as_deref() {
                    None => true,
                    Some(pid) => !s
                        .notes
                        .iter()
                        .any(|other| other.id == pid && other.notebook == book),
                },
            }
        })
        .collect();
    children
}

fn render_page_row(s: &State, note: &Note, meta: &PageRow, width: f32) -> El {
    let book = note.notebook.as_str();
    let parent_id = note.parent_id.as_deref();
    let depth = meta.depth;
    let sib_idx = meta.sibling_index;
    let ink = crate::ui::nav_ink(s);
    let muted = crate::ui::nav_muted(s);
    let subtle = crate::ui::nav_subtle(s);
    let insert_before = matches!(&s.nav_drag,
            Some(NavDrag::Note { book: drag_b, parent_id: drag_p, source_idx,
                target_idx, drop_into_book: None, valid_drop: true, .. })
            if drag_b == book && drag_p.as_deref() == parent_id
                && *target_idx == sib_idx && *target_idx != *source_idx
                && *target_idx != source_idx + 1);

    let id = note.id.clone();
    let row_key = format!("note-{id}");
    let active = s.current == id;
    let is_dragging_note = matches!(
        &s.nav_drag,
        Some(NavDrag::Note { note_id, .. }) if note_id == &id
    );
    let has_children = meta.has_children;
    let expanded = s.tree_expanded.contains(&id);
    let target = RenameTarget::Note(id.clone());
    let editing = s.renaming.as_ref() == Some(&target);

    let toggle_el = if has_children {
        let toggle_id = id.clone();
        el().key(format!("tree-toggle-{id}"))
            .absolute(2., (crate::navigation::PAGE_ROW_HEIGHT - crate::navigation::PAGE_DISCLOSURE) / 2.)
            .role("button")
            .focusable(true)
            .label(if expanded {
                s.tr("收起子页面")
            } else {
                s.tr("展开子页面")
            })
            .expanded(expanded)
            .size(
                crate::navigation::PAGE_DISCLOSURE,
                crate::navigation::PAGE_DISCLOSURE,
            )
            .radius(4.)
            .center()
            .hover(crate::ui::nav_hover(s))
            .cursor(Cursor::Pointer)
            .on_press_tx(move |s: &mut State, tx| {
                s.toggle_tree(&toggle_id);
                tx.animated();
            })
            .child(
                ico(
                    if expanded {
                        &CHEVRON_DOWN
                    } else {
                        &CHEVRON_RIGHT
                    },
                    11.5,
                )
                .color(c(subtle)),
            )
    } else {
        el().absolute(0., 0.).size(0., 0.).inert()
    };

    let layout_note_key = format!("note:{id}");
    let mut item_row = row()
        .key(row_key)
        .instant_changes()
        .motion(rune::Spring::NONE)
        .w(width - note_inset(depth))
        .ml(note_inset(depth))
        .h(crate::navigation::PAGE_ROW_HEIGHT)
        .pl(crate::navigation::PAGE_ROW_PADDING)
        .pr(4.)
        .gap(crate::navigation::PAGE_GAP)
        .radius(7.)
        .center_y()
        .opacity(if is_dragging_note { 0.35 } else { 1. })
        .fill(if active {
            crate::ui::nav_selection(s)
        } else {
            rune::Color::TRANSPARENT
        })
        .border(
            if active && crate::ui::nav_selection_border(s).is_some() { 0.75 } else { 0. },
            crate::ui::nav_selection_border(s).unwrap_or(rune::Color::TRANSPARENT),
        )
        .hover(if active {
            crate::ui::nav_selection(s)
        } else {
            crate::ui::nav_hover(s)
        })
        .on_drawn_layout_tx(move |s: &mut State, r, tx| {
            s.nav_row_rects.insert(layout_note_key.clone(), r);
            tx.live_only();
        })
        .child(toggle_el);

    if editing {
        item_row = item_row.child(
            row()
                .grow(1.)
                .min_w(0.)
                .gap(crate::navigation::PAGE_TITLE_GAP)
                .center_y()
                .child(
                    crate::navigation::page_marker(s, note.format).key(format!("page-marker-{id}")),
                )
                .child(rename_input(s).grow(1.).min_w(0.)),
        );
    } else {
        let select_id = id.clone();
        let key_id = id.clone();
        let drag_id = id.clone();
        let drag_book = book.to_owned();
        let drag_parent = parent_id.map(str::to_owned);
        let siblings = meta.siblings.clone();
        let menu_id = id.clone();
        let main_area = row()
            .grow(1.)
            .min_w(0.)
            .h_full()
            .gap(crate::navigation::PAGE_TITLE_GAP)
            .center_y()
            .cursor(Cursor::Pointer)
            .focusable(true)
            .role("button")
            .label(&note.title)
            .child(crate::navigation::page_marker_for(s, note.format, active).key(format!("page-marker-{id}")))
            .child(
                label(
                    if note.title.trim().is_empty() {
                        s.tr("未命名页面")
                    } else {
                        &note.title
                    },
                    13.,
                    if depth == 0 && has_children {
                        550.
                    } else {
                        450.
                    },
                    ink,
                )
                .truncate()
                .grow(1.)
                .min_w(0.),
            )
            .on_press_tx(move |s: &mut State, tx| {
                if std::mem::take(&mut s.just_dragged_nav) {
                    return;
                }
                s.select_note(&select_id);
                tx.animated();
            })
            .on_key(move |s: &mut State, e, tx| {
                if e.key == rune::Key::Enter {
                    s.begin_rename(RenameTarget::Note(key_id.clone()));
                    tx.focus("rename-field", false);
                    true
                } else {
                    false
                }
            })
            .on_drag(move |s: &mut State, d, tx| {
                match d.phase {
                    Phase::Down => {
                        s.nav_drag = None;
                        s.just_dragged_nav = false;
                    }
                    Phase::Move
                        if !s.just_dragged_nav
                            && (s.nav_drag.is_some() || d.dx.hypot(d.dy) > 5.) =>
                    {
                        let pt = d.window_point();
                        let (drop_into_book, target_idx, valid_drop) =
                            crate::navigation::note_drop_target(
                                s,
                                &drag_book,
                                drag_parent.as_deref(),
                                &siblings,
                                pt,
                            );
                        s.nav_drag = Some(NavDrag::Note {
                            note_id: drag_id.clone(),
                            book: drag_book.clone(),
                            parent_id: drag_parent.clone(),
                            source_idx: sib_idx,
                            target_idx,
                            drop_into_book,
                            valid_drop,
                            pointer: pt,
                        });
                    }
                    Phase::Up => {
                        if let Some(NavDrag::Note {
                            note_id,
                            book,
                            parent_id,
                            ..
                        }) = s.nav_drag.take()
                        {
                            s.just_dragged_nav = true;
                            let (drop_into_book, target_idx, valid_drop) =
                                crate::navigation::note_drop_target(
                                    s,
                                    &book,
                                    parent_id.as_deref(),
                                    &siblings,
                                    d.window_point(),
                                );
                            if !d.cancelled() && valid_drop {
                                if let Some(target) = drop_into_book {
                                    s.move_note_to_book(&note_id, &target);
                                } else {
                                    s.reorder_note(&note_id, &book, parent_id, target_idx);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                if s.nav_drag.is_some() || s.just_dragged_nav {
                    tx.instant();
                } else {
                    tx.animated();
                }
            });
        let more_id = id.clone();
        let more_key = format!("page-more-{id}");
        let more_anchor = more_key.clone();
        item_row = item_row
            .child(main_area)
            .child(
                el().key(more_key)
                    .size(20., 28.)
                    .radius(5.)
                    .center()
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .role("button")
                    .label(s.tr("页面选项"))
                    .help(s.tr("重命名、移动或删除页面"))
                    .hover(crate::ui::nav_hover(s))
                    .group_opacity(0., 1.)
                    .child(ico(&MORE_HORIZONTAL, 16.).color(c(muted)))
                    .on_press_tx(move |s: &mut State, tx| {
                        crate::navigation::page_menu(s, &more_id, more_anchor.clone(), tx)
                    }),
            )
            .on_secondary_press(move |s: &mut State, press, tx| {
                crate::navigation::page_menu(s, &menu_id, press, tx);
            });
    }

    if insert_before {
        item_row = item_row.child(nav_drop_slot(
            s,
            "nav-drop-indicator",
            18.,
            width - note_inset(depth),
        ));
    }

    if matches!(&s.nav_drag, Some(NavDrag::Note { book: b, parent_id: p, source_idx, target_idx, drop_into_book: None, valid_drop: true, .. })
        if b == book && p.as_deref() == parent_id && *target_idx == meta.siblings.len() && sib_idx + 1 == meta.siblings.len() && *source_idx + 1 < meta.siblings.len())
    {
        item_row = item_row.child(
            nav_drop_slot(s, "nav-drop-indicator", 18., width - note_inset(depth))
                .absolute(18., crate::navigation::PAGE_ROW_HEIGHT),
        );
    }
    col().w_full().h(crate::navigation::PAGE_ROW_PITCH).child(item_row)
}
