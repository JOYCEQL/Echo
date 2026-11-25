use crate::{
    doc_core::PageWidth,
    icons::*,
    model::{Note, State},
    ui::{c, label},
};
use rune::{Cursor, Cx, El, FileDialogOptions, PopupItem, PopupMenu, ScrollSpec, col, el, row};

pub const HEADER: f32 = 0.;
pub const OUTLINE_WIDTH: f32 = 188.;

pub fn view(s: &State, _cx: &mut Cx<State>, w: f32, h: f32) -> El {
    let nav_width = crate::panes::visible_navigation_width(s, w);
    crate::panes::workspace_shell(
        s,
        editor_pane(s, w - nav_width, h - crate::ui::editor_top(s)),
        w,
        h,
        crate::panes::Boundary::Documents,
    )
}

fn editor_pane(s: &State, width: f32, height: f32) -> El {
    let page_bg = crate::ui::page_surface(s);
    let Some(note) = s.note() else {
        return col()
            .key("canvas-pane")
            .size(width, height)
            .fill(c(page_bg))
            .clip_rounded(10.)
            .child(
                col()
                    .w_full()
                    .grow(1.)
                    .center()
                    .gap(8.)
                    .child(ico(&FILE_TEXT, 28.).color(c(s.theme().choose(0xc5ccd8, 0x9297a3))))
                    .child(label(
                        s.tr("暂无打开的文档"),
                        15.5,
                        520.,
                        s.theme().choose(0x646a73, 0xb3b6bf),
                    ))
                    .child(label(
                        s.tr("在左侧选择一篇文档，或按 ⌘N 新建文档"),
                        13.,
                        400.,
                        s.theme().choose(0x8f959e, 0x9297a3),
                    )),
            );
    };

    let doc = &s.editor.active.core.doc;
    let open = s.document_outline_visible();
    let headings = open.then(|| doc.headings());
    let has_headings = match &headings {
        Some(rows) => !rows.is_empty(),
        None => doc.has_headings(),
    };
    // Leave 520 pt for writing and the editor's two 72 pt margins when pinning.
    let pinned_width = if open && has_headings && width >= 520. + 144. + 144. {
        (width - 520. - 144.).min(OUTLINE_WIDTH)
    } else {
        0.
    };
    let editor_width = width - pinned_width;
    let editor = crate::doc_editor::render_editor(s, note, editor_width, width, height - HEADER);
    let mut pane = col()
        .key("canvas-pane")
        .size(width, height)
        .fill(c(page_bg))
        .clip_rounded(10.)
        .child(editor);
    if has_headings {
        const TOP: f32 = 54.;
        const RIGHT: f32 = 12.;
        if let Some(headings) = &headings {
            let floating = pinned_width == 0.;
            let rail_width = if floating { OUTLINE_WIDTH } else { pinned_width };
            pane = pane.child(outline_rail(s, headings, rail_width, height - TOP - 16., floating)
                .absolute(width - rail_width - RIGHT, TOP));
        }
        pane = pane.child(header_icon_btn(s, "doc-outline-toggle",
            if open { &CHEVRON_RIGHT } else { &LIST },
            s.tr(if open { "隐藏目录大纲" } else { "显示目录大纲" }))
            .expanded(open).keeps_text_selection().absolute(width - RIGHT - 24. - 6., TOP + 8.)
            .on_press_tx(|s: &mut State, tx| { s.toggle_document_outline(); tx.instant(); }));
    }
    pane
}

fn header_icon_btn(
    s: &State,
    key: &'static str,
    icon_ref: &'static rune::Icon,
    help: &'static str,
) -> El {
    el().key(key)
        .size(24., 24.)
        .radius(7.)
        .center()
        .hover(crate::ui::nav_hover(s))
        .cursor(Cursor::Pointer)
        .role("button")
        .focusable(true)
        .label(help)
        .help(help)
        .child(ico(icon_ref, 16.).color(c(crate::ui::chrome_muted(s))))
}

pub(crate) fn document_options(s: &State, note: &Note) -> El {
    let cur_book = note.notebook.clone();
    let cur_id = note.id.clone();
    let actions = row().gap(4.).center_y().child(
        header_icon_btn(s, "doc-more", &MORE_HORIZONTAL, s.tr("文档选项")).on_press_tx(
            move |s: &mut State, tx| {
                let id = cur_id.clone();
                let book = cur_book.clone();
                let width = s.editor.active.core.doc.page_width;
                let mut menu = PopupMenu::new()
                    .item(PopupItem::new(
                        "standard",
                        if width == PageWidth::Standard {
                            s.tr("✓ 标准宽度")
                        } else {
                            s.tr("标准宽度")
                        },
                    ))
                    .item(PopupItem::new(
                        "wide",
                        if width == PageWidth::Wide {
                            s.tr("✓ 宽屏")
                        } else {
                            s.tr("宽屏")
                        },
                    ))
                    .item(PopupItem::new(
                        "full",
                        if width == PageWidth::Full {
                            s.tr("✓ 全宽")
                        } else {
                            s.tr("全宽")
                        },
                    ))
                    .separator()
                    .item(PopupItem::new("new", s.tr("新建子页面")))
                    .item(PopupItem::new("export", s.tr("导出 Markdown")));
                let ancestors = s.note_breadcrumbs(&id);
                if ancestors.len() > 1 {
                    menu = menu.separator();
                    for (ancestor_id, title) in ancestors.iter().take(ancestors.len() - 1) {
                        let title = if s.language.is_english() {
                            format!("Go to {title}")
                        } else {
                            format!("返回 {title}")
                        };
                        menu = menu.item(PopupItem::new(format!("ancestor:{ancestor_id}"), title));
                    }
                }
                tx.popup_menu(
                    menu,
                    "doc-more",
                    move |s: &mut State, picked, tx| match picked.as_deref() {
                        Some("standard" | "wide" | "full") => {
                            s.editor.active.core.doc.page_width = match picked.as_deref() {
                                Some("wide") => PageWidth::Wide,
                                Some("full") => PageWidth::Full,
                                _ => PageWidth::Standard,
                            };
                            s.editor.active.core.dirty = true;
                            crate::doc_editor::sync_to_state(s);
                        }
                        Some(target) if target.starts_with("ancestor:") => {
                            s.select_note(&target[9..]);
                            tx.instant();
                        }
                        Some("new") => {
                            crate::navigation::new_page(s, &book, Some(id.clone()), tx);
                            tx.blur();
                        }
                        Some("export") => {
                            let Some(note) = s.note() else { return };
                            let options = FileDialogOptions {
                                title: s.tr("导出 Markdown").into(),
                                file_name: Some(format!("{}.md", note.title)),
                                ..Default::default()
                            }
                            .filter("Markdown", &["md"]);
                            tx.save_file(options, |s: &mut State, result, _| match result {
                                Ok(Some(path)) => s.export_markdown(&path),
                                Err(err) => {
                                    s.message =
                                        Some(format!("{}: {err}", s.tr("无法导出 Markdown")))
                                }
                                _ => {}
                            });
                        }
                        _ => {}
                    },
                );
            },
        ),
    );

    actions
}

fn outline_rail(s: &State, headings: &[(u64, u8, String)], width: f32, max_height: f32, floating: bool) -> El {
    let mut list = col().w_full().gap(2.);

    for (idx, (block_id, level, text)) in headings.iter().enumerate() {
        let indent = ((level.saturating_sub(1)).min(3) as f32) * 10.;
        let bid = *block_id;
        list = list.child(
            row()
                .key(format!("outline-item-{idx}"))
                .w_full()
                .min_h(26.)
                .pl(10. + indent)
                .pr(8.)
                .py(3.)
                .radius(6.)
                .center_y()
                .border_sides(
                    [0., 0., 0., 1.5],
                    c(crate::ui::editor_colors(s.theme()).border),
                )
                .hover(c(crate::ui::editor_colors(s.theme()).inset))
                .cursor(Cursor::Pointer)
                .role("button")
                .label(text)
                .focusable(true)
                .on_press_tx(move |s: &mut State, tx| crate::doc_editor::follow_outline(s, bid, tx))
                .child(
                    label(
                        text.clone(),
                        12.5,
                        if *level == 1 { 540. } else { 440. },
                        if *level == 1 {
                            s.theme().choose(0x646a73, 0xb3b6bf)
                        } else {
                            s.theme().choose(0x8f959e, 0x9297a3)
                        },
                    )
                    .truncate()
                    .min_w(0.)
                    .grow(1.),
                ),
        );
    }

    let height = (52. + headings.len() as f32 * 28.).min(max_height);
    let mut rail = col()
        .key("doc-outline-pane")
        .size(width, height)
        .px(14.)
        .pt(40.)
        .pb(8.)
        .child(
            el().w_full()
                .h_full()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(list),
        );
    rail = rail.child(label(s.tr("大纲"), 12., 550., crate::ui::theme_muted(s.theme()))
        .absolute(14., 8.).h(24.).center_y().inert());
    if floating {
        rail = rail.radius(8.).fill(c(crate::ui::page_surface(s)))
            .border(1., c(crate::ui::editor_colors(s.theme()).border))
            .shadow(3., 12., rune::Color::BLACK.alpha(0.12));
    }
    rail
}
