//! Shared notebook rail and page navigation for both editing modes.
use crate::{
    icons::*,
    model::{NavDrag, RenameTarget, State},
    ui::{c, label},
};
use rune::{Cursor, El, Phase, PopupAnchor, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, row};

// AppKit keeps its native button pitch: the three controls need a full 84 pt rail.
pub const RAIL_WIDTH: f32 = 84.;
// Shared page-navigation grid: disclosure gutter, marker and title stay aligned.
pub const PAGE_GUTTER: f32 = 4.;
pub const PAGE_ROW_PADDING: f32 = 16.;
pub const PAGE_DISCLOSURE: f32 = 12.;
pub const PAGE_GAP: f32 = 2.;
pub const PAGE_MARKER: f32 = 24.;
pub const PAGE_TITLE_GAP: f32 = 10.;
pub const PAGE_ROW_HEIGHT: f32 = 44.;
pub const PAGE_ROW_PITCH: f32 = PAGE_ROW_HEIGHT + 2.;
// Center the 28pt search target on the page-icon column.
const PAGE_HEADER_LEFT: f32 = PAGE_GUTTER + PAGE_ROW_PADDING + PAGE_MARKER / 2. - 14.;
const PAGE_HEADER_RIGHT: f32 = 8.;


pub fn page_marker(s: &State, format: crate::doc_core::NoteFormat) -> El {
    page_marker_for(s, format, false)
}
pub fn page_marker_for(s: &State, format: crate::doc_core::NoteFormat, active: bool) -> El {
    let document = format == crate::doc_core::NoteFormat::Document;
    let marker = match s.notebook_icon_style {
        crate::notebook_icons::Style::Solid => {
            let color = if active { crate::ui::nav_ink(s) } else { crate::ui::nav_subtle(s) };
            el().size(PAGE_MARKER, PAGE_MARKER).min_w(PAGE_MARKER).min_h(PAGE_MARKER).center().inert()
                .child(s.notebook_icon_style.page_icon(format, 17.5, color))
        }
        crate::notebook_icons::Style::Monochrome => {
            el().size(PAGE_MARKER, PAGE_MARKER).min_w(PAGE_MARKER).min_h(PAGE_MARKER).center().inert()
                .child(s.notebook_icon_style.page_icon(format, 21., crate::ui::nav_muted(s)))
        }
        _ => s.page_icons.render(format, PAGE_MARKER),
    };
    marker.help(s.tr(if document { "结构化文档" } else { "自由画布" }))
}
const RAIL_ACTION_SIZE: f32 = crate::ui::BOTTOM_CONTROL_HEIGHT;
pub(crate) const RAIL_GLYPH_SIZE: f32 = 16.;

pub(crate) fn rail_action(key: &str, title: &str) -> El {
    el().key(key).size(RAIL_ACTION_SIZE, RAIL_ACTION_SIZE)
        .radius(8.).center().label(title).help(title)
}
fn icon_button(s: &State, key: &str, title: &str, glyph: &'static rune::Icon) -> El {
    rail_action(key, title).cursor(Cursor::Pointer).focusable(true).role("button")
        .hover(crate::ui::nav_hover(s))
        .child(ico(glyph, RAIL_GLYPH_SIZE).key(format!("{key}-glyph")).color(c(crate::ui::nav_muted(s))))
}
pub fn search_button(s: &State) -> El {
    icon_button(s, "open-search", s.tr("搜索笔记 · ⌘K"), &SEARCH)
        .size(28., 28.)
        .help(s.tr("搜索标题或正文…"))
        .on_press_tx(crate::library_ui::open_search)
}
fn pages_header(s: &State) -> El {
    let mut header = row().key("pages-header").w_full().h(crate::ui::CHROME_HEIGHT)
        .pl(PAGE_HEADER_LEFT).pr(PAGE_HEADER_RIGHT).gap(6.).center_y().window_drag();
    if !s.sidebar_collapsed {
        header = header.child(search_button(s));
        header = header.child(if matches!(s.renaming, Some(RenameTarget::Book(_))) {
            crate::ui::rename_input(s).grow(1.).min_w(0.)
        } else { el().grow(1.) });
        header = header.child(sidebar_toggle(s));
    }
    header
}
pub fn sidebar_toggle(s: &State) -> El {
    el().key("toggle-sidebar").size(28., 28.).radius(7.).center()
        .cursor(Cursor::Pointer).focusable(true).role("button")
        .label(if s.sidebar_collapsed { s.tr("展开侧边栏 · ⌘\\") } else { s.tr("收起侧边栏 · ⌘\\") })
        .hover(crate::ui::nav_hover(s))
        .child(ico(&SIDEBAR_LEFT, 19.).color(c(crate::ui::nav_muted(s))))
        .keeps_text_selection()
        .on_press_tx(|s: &mut State, tx| { s.toggle_sidebar(); tx.animated(); })
}
pub fn new_page(s: &mut State, book: &str, parent: Option<String>, tx: &mut Tx) {
    crate::library_ui::open_new_page(s, book, parent, tx);
}
/// Native presence motion for a destination, without suppressing its own popovers.
pub(crate) fn switch_surface(key: String, content: El) -> El {
    el().key(key).w_full()
        .transition(rune::Transition::opacity().springs(rune::SpringPair::new(
            rune::Spring::new(0.18, 0.), rune::Spring::new(0.10, 0.))))
        .child(content)
}
pub fn rail(s: &State, width: f32, height: f32) -> El {
    let mut books = col().key("notebook-items").w_full().gap(6.);
    for i in 0..s.book_order.len() {
        let book = s.book_at(i).to_owned();
        let active = s.book == book;
        let select = book.clone();
        let menu_book = book.clone();
        let layout_book = book.clone();
        let drop_target = matches!(&s.nav_drag, Some(NavDrag::Note { drop_into_book: Some(b), .. }) if b == &book);
        let dragging =
            matches!(&s.nav_drag, Some(NavDrag::Book { source_idx, .. }) if *source_idx == i);
        let mut item = col()
            .key(format!("book-{i}"))
            .w_full()
            .h(58.)
            .gap(3.)
            .center()
            .radius(10.)
            .instant_changes()
            .motion(rune::Spring::NONE)
            .cursor(Cursor::Pointer)
            .role("button")
            .focusable(true)
            .label(s.book_name(&book))
            .help(if s.language.is_english() { format!("{} · Right-click to manage", s.book_name(&book)) } else { format!("{} · 右键管理空间", s.book_name(&book)) })
            .border(
                if drop_target {
                    1.5
                } else if active && crate::ui::nav_selection_border(s).is_some() {
                    0.75
                } else {
                    0.
                },
                if drop_target {
                    c(crate::ui::accent(s)).alpha(0.65)
                } else {
                    crate::ui::nav_selection_border(s).unwrap_or(rune::Color::TRANSPARENT)
                },
            )
            .fill(if drop_target {
                c(crate::ui::accent(s)).alpha(0.13)
            } else if active {
                crate::ui::nav_selection(s)
            } else {
                rune::Color::TRANSPARENT
            })
            .hover(if active {
                crate::ui::nav_selection(s)
            } else {
                crate::ui::nav_hover(s)
            })
            .opacity(if dragging { 0.4 } else { 1. })
            .child(crate::notebook_icons::render(s, &book, 30., active, crate::ui::nav_muted(s)))
            .child(
                label(s.book_name(&book), 11.5, 450., crate::ui::nav_ink(s))
                    .w_full()
                    .align(rune::Align::Center)
                    .truncate(),
            )
            .on_layout_tx(move |s: &mut State, r, tx| {
                s.nav_row_rects.insert(format!("book:{layout_book}"), r);
                s.nav_row_rects.insert(format!("book-{i}"), r);
                tx.live_only();
            })
            .on_press_tx(move |s: &mut State, tx| {
                if std::mem::take(&mut s.just_dragged_nav) {
                    return;
                }
                s.select_book(&select);
                s.search_open = false;
                tx.focus(format!("book-{i}"), false);
                tx.animated();
            })
            .on_key(move |s: &mut State, e, tx| {
                if e.key == rune::Key::Enter {
                    s.begin_rename(RenameTarget::Book(i));
                    tx.focus("rename-field", false);
                    true
                } else {
                    false
                }
            })
            .on_secondary_press(move |s: &mut State, press, tx| {
                let b = menu_book.clone();
                let menu = PopupMenu::new()
                    .item(PopupItem::new("new", s.tr("新建页面")))
                    .item(PopupItem::new("rename", s.tr("重命名空间")))
                    .separator()
                    .item(PopupItem::new("up", s.tr("上移空间")).enabled(i > 0))
                    .item(PopupItem::new("down", s.tr("下移空间")).enabled(i + 1 < s.book_order.len()))
                    .separator().item(PopupItem::new("delete", s.tr("删除空间")));
                tx.popup_menu(menu, press, move |s: &mut State, picked, tx| match picked {
                    Some("new") => new_page(s, &b, None, tx),
                    Some("rename") => {
                        s.select_book(&b);
                        s.begin_rename(RenameTarget::Book(i));
                        tx.focus("rename-field", false);
                    }
                    Some("delete") => crate::library_ui::open_delete_notebook(s, &b, tx),
                    Some("up") => {
                        s.move_book(i, i - 1);
                    }
                    Some("down") => {
                        s.move_book(i, i + 2);
                    }
                    _ => {}
                });
            })
            .on_drag(move |s: &mut State, d, tx| {
                match d.phase {
                    Phase::Down => {
                        s.nav_drag = None;
                        s.just_dragged_nav = false;
                    }
                    Phase::Move if d.dy.abs() > 5. => {
                        let pt = d.window_point();
                        let target_idx = (0..s.book_order.len())
                            .filter(|j| {
                                s.nav_row_rects
                                    .get(&format!("book-{j}"))
                                    .is_some_and(|r| pt[1] > r[1] + r[3] / 2.)
                            })
                            .count();
                        s.nav_drag = Some(NavDrag::Book {
                            source_idx: i,
                            target_idx,
                            pointer: pt,
                        });
                    }
                    Phase::Up => {
                        if let Some(NavDrag::Book {
                            source_idx,
                            target_idx,
                            ..
                        }) = s.nav_drag.take()
                        {
                            s.just_dragged_nav = true;
                            if !d.cancelled() {
                                s.move_book(source_idx, target_idx);
                            }
                        }
                    }
                    _ => {}
                }
                if s.nav_drag.is_some() || s.just_dragged_nav { tx.instant(); }
                else { tx.animated(); }
            });
        if let Some(NavDrag::Book {
            target_idx,
            source_idx,
            ..
        }) = &s.nav_drag
        {
            if *target_idx == i && *target_idx != *source_idx && *target_idx != source_idx + 1 {
                item = item.child(
                    el().key("nav-drop-indicator")
                        .absolute(0., 0.)
                        .size(width - 20., 2.)
                        .fill(c(crate::ui::accent(s)))
                        .inert(),
                );
            }
        }
        books = books.child(item);
    }
    let action_left = (width - 20. - RAIL_ACTION_SIZE) / 2.;
    books = books.child(icon_button(s, "new-notebook", s.tr("新建空间"), &PLUS)
        .ml(action_left)
        .on_press_tx(crate::library_ui::open_new_notebook));
    col()
        .key("sidebar")
        .size(width, height)
        .px(10.)
        .pb(0.)
        .child(el().w_full().h(crate::ui::CHROME_HEIGHT).window_drag())
        .child(
            col()
                .key("notebook-scroll")
                .w_full()
                .grow(1.)
                .min_h(0.)
                .mt(6.)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .on_layout_tx(|s: &mut State, rect, tx| {
                    s.nav_row_rects.insert("notebooks-viewport".into(), rect); tx.live_only();
                })
                .child(books),
        )
        .child(col().w(RAIL_ACTION_SIZE).ml(action_left).mt(8.).gap(2.)
            .children(s.updater.status.visible_in_sidebar().then(|| crate::update::update_button(s)))
            .child(icon_button(s, "settings", s.tr("设置 · ⌘,"), &SETTINGS)
                .keeps_text_selection()
                .on_press_tx(State::open_settings)))
}
pub fn pages(s: &State, width: f32, height: f32) -> El {
    if s.book_order.is_empty() {
        return col().key("note-list").size(width,height).gap(16.)
            .child(pages_header(s))
            .child(label(s.tr("创建空间，开始记录"),13.,400.,crate::ui::nav_muted(s)).wrap())
            .child(row().key("empty-new-notebook").h(36.).gap(8.).center_y().role("button").focusable(true)
                .cursor(Cursor::Pointer).label(s.tr("新建空间"))
                .child(ico(&PLUS,16.).color(c(crate::ui::nav_muted(s))))
                .child(label(s.tr("新建空间"),13.,500.,crate::ui::nav_ink(s)))
                .on_press_tx(crate::library_ui::open_new_notebook));
    }
    use std::hash::{Hash, Hasher};
    // Only navigation metadata contributes; typing in a page's body must not rebuild
    // the page tree. A compact key also catches external file renames/reordering.
    let mut version = std::collections::hash_map::DefaultHasher::new();
    (width.to_bits(), height.to_bits(), &s.book, &s.current, &s.book_order,
        s.language.is_english(), s.theme().is_dark(), s.notebook_icon_style.key()).hash(&mut version);
    s.sidebar_collapsed.hash(&mut version);
    for note in &s.notes {
        (&note.id, &note.title, &note.notebook, &note.parent_id, std::mem::discriminant(&note.format)).hash(&mut version);
    }
    let mut expanded: Vec<_> = s.tree_expanded.iter().collect();
    expanded.sort(); expanded.hash(&mut version);
    format!("{:?}{:?}", s.nav_drag, s.renaming).hash(&mut version);
    (&s.name_input.text, s.name_input.focused).hash(&mut version);
    serde_json::to_string(&s.sidebar_material).expect("appearance serializes").hash(&mut version);
    let tree_key = format!("notebook-content-{}", s.book);
    let content = rune::memo(&tree_key, version.finish(), || {
        crate::page_tree::page_tree(s, width - PAGE_GUTTER * 2.).key(&tree_key).instant_changes()
            .transition(rune::Transition::opacity().springs(rune::SpringPair::new(
                rune::Spring::new(0.18, 0.), rune::Spring::new(0.10, 0.))))
    });
    let offset = s.list_offset.clone();
    let tree = col().key("notes-scroll")
        .w_full().grow(1.).min_h(0.).px(PAGE_GUTTER).pt(6.).pb(12.)
        .scroll(ScrollSpec { stick_bottom: false })
        .on_layout_tx(|s: &mut State, rect, tx| {
            s.nav_row_rects.insert("notes-viewport".into(), rect); tx.live_only();
        })
        .on_scroll_offset(move |p| offset.set(p.offset))
        .child(content);
    col().key("note-list").size(width, height)
        .child(pages_header(s))
        .child(tree)
}

pub fn page_menu(s: &mut State, note_id: &str, anchor: impl Into<PopupAnchor>, tx: &mut Tx) {
    s.finish_edit();
    let Some(note) = s.notes.iter().find(|n| n.id == note_id) else {
        return;
    };
    let id = note.id.clone();
    let book = note.notebook.clone();
    let parent = note.parent_id.clone();
    let siblings = crate::page_tree::children_of(s, &book, parent.as_deref());
    let idx = siblings
        .iter()
        .position(|n| n.id == id)
        .expect("page belongs to its sibling list");
    let count = siblings.len();
    let mut menu = PopupMenu::new()
        .item(PopupItem::new("new-child", s.tr("新建子页面")))
        .item(PopupItem::new("rename", s.tr("重命名")))
        .item(PopupItem::new("export", s.tr("导出 Markdown")))
        .item(PopupItem::new("up", s.tr("上移页面")).enabled(idx > 0))
        .item(PopupItem::new("down", s.tr("下移页面")).enabled(idx + 1 < count));
    if parent.is_some() {
        menu = menu.item(PopupItem::new("promote", s.tr("移至上一级")));
    }
    menu = menu.separator().item(PopupItem::new("trash", s.tr("移至回收站")));
    tx.popup_menu(menu, anchor, move |s: &mut State, picked, tx| {
        match picked {
            Some("new-child") => new_page(s, &book, Some(id.clone()), tx),
            Some("rename") => {
                s.begin_rename(RenameTarget::Note(id.clone()));
                tx.focus("rename-field", false);
            }
            Some("export") => {
                s.select_note(&id);
                let note = s.note().expect("selected export page");
                let options = rune::FileDialogOptions {
                    title: s.tr("导出 Markdown").into(),
                    file_name: Some(format!("{}.md", note.title)),
                    ..Default::default()
                }
                .filter("Markdown", &["md"]);
                tx.save_file(options, |s: &mut State, result, _| match result {
                    Ok(Some(path)) => s.export_markdown(&path),
                    Err(err) => s.message = Some(format!("{}: {err}", s.tr("无法导出 Markdown"))),
                    _ => {}
                });
            }
            Some("up") => {
                s.reorder_note(&id, &book, parent.clone(), idx - 1);
            }
            Some("down") => {
                s.reorder_note(&id, &book, parent.clone(), idx + 2);
            }
            Some("promote") => {
                s.promote_note(&id);
            }
            Some("trash") => {
                s.select_note(&id);
                s.trash_note();
            }
            _ => {}
        }
        tx.instant();
    });
}

/// A drop is actionable only within a visible notebook tile or the page viewport.
pub fn note_drop_target(s: &State, book: &str, parent: Option<&str>, siblings: &[String], pt: [f32; 2]) -> (Option<String>, usize, bool) {
    let contains = |r: &[f32; 4]| pt[0] >= r[0] && pt[0] <= r[0]+r[2] && pt[1] >= r[1] && pt[1] <= r[1]+r[3];
    for i in 0..s.book_order.len() {
        let target = s.book_at(i);
        if target == book && parent.is_none() { continue; }
        if s.nav_row_rects.get("notebooks-viewport").is_some_and(contains)
            && s.nav_row_rects.get(&format!("book:{target}")).is_some_and(contains) {
            return (Some(target.to_owned()), 0, true);
        }
    }
    let valid = s.nav_row_rects.get("notes-viewport").is_some_and(contains);
    let mut slot = 0;
    if valid {
        for (index, id) in siblings.iter().enumerate() {
            if let Some(r) = s.nav_row_rects.get(&format!("note:{id}")) {
                slot = index;
                if pt[1] <= r[1] + r[3]/2. { break; }
                slot = index + 1;
            }
        }
    }
    (None, slot, valid)
}
