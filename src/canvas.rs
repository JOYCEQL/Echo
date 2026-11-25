use crate::{
    icons::*,
    model::State,
    ui::{c, label},
};
use rune::{Cursor, El, Phase, col, el, row};
use rune_kit::ScrollBar;
pub const HEADER: f32 = 0.;
// Saved card coordinates use the original canvas origin, independent of toolbar height.
pub const ORIGIN_Y: f32 = 72.;
pub fn page_size(s: &State, cx: &rune::Cx<State>, width: f32, height: f32) -> [f32; 2] {
    let zoom = s.note().map_or(1., |n| n.zoom);
    let mut w = width / zoom;
    let mut h = (height - HEADER) * 1.5 / zoom;
    if let Some(n) = s.note() {
        for b in &n.blocks {
            w = w.max(b.x + b.w + 64. / zoom);
            let bh = cx
                .layout_size(&format!("block-{}", b.id))
                .map_or(b.h, |size| size[1]);
            h = h.max(b.y - ORIGIN_Y + bh + 240.);
        }
    }
    [w, h]
}
pub(crate) fn canvas_options(s: &State, width: f32) -> El {
    el().key("canvas-more").size(24., 24.).radius(7.).center()
        .role("button").label(s.tr("画布选项")).help(s.tr("画布选项")).focusable(true)
        .cursor(Cursor::Pointer).hover(crate::ui::nav_hover(s))
        .child(ico(&MORE_HORIZONTAL, 17.).color(c(crate::ui::chrome_muted(s))))
        .on_press_tx(move |s: &mut State, tx| {
            let Some(note) = s.note() else { return };
            let id = note.id.clone();
            let enabled = note.blocks.iter().any(|b| s.draft.as_deref() != Some(&b.id));
            let mut menu = rune::PopupMenu::new().item(rune::PopupItem::new("rename", s.tr("重命名")));
            if enabled { menu = menu.item(rune::PopupItem::new("arrange", s.tr("一键铺平"))); }
            menu = menu.separator().item(rune::PopupItem::new("export", s.tr("导出 Markdown")));
            tx.popup_menu(menu, "canvas-more", move |s: &mut State, picked, tx| {
                match picked.as_deref() {
                    Some("rename") => { s.begin_rename(crate::model::RenameTarget::Note(id.clone())); s.focus_page_rename(&id, tx); }
                    Some("arrange") => {
                        s.arrange_blocks(width, tx);
                    }
                    Some("export") => {
                        let Some(note) = s.note() else { return };
                        let options = rune::FileDialogOptions { title: s.tr("导出 Markdown").into(),
                            file_name: Some(format!("{}.md", note.title)), ..Default::default() }.filter("Markdown", &["md"]);
                        tx.save_file(options, |s: &mut State, result, _| match result {
                            Ok(Some(path)) => s.export_markdown(&path),
                            Err(err) => s.message = Some(format!("{}: {err}", s.tr("无法导出 Markdown"))),
                            _ => {},
                        });
                    }
                    _ => {},
                }
            });
        })
}
pub fn view(s: &State, cx: &rune::Cx<State>, width: f32, height: f32) -> El {
    let mut canvas = el()
        .key("canvas")
        .on_drawn_layout_tx(|s: &mut State, r, tx| {
            s.canvas_viewport_rect = Some(r);
            s.editor.set_canvas_viewport(r);
            tx.live_only();
        })
        .size(width, height)
        .fill(c(crate::ui::page_surface(s)))
        .clip_rounded(10.);
    if let Some(n) = s.note() {
        let zoom = n.zoom;
        let [pw, ph] = page_size(s, cx, width, height);
        let scroll = s.scroll_pos.clone();
        let waker = s.window_waker.clone();
        let bounds = [
            12.,
            ORIGIN_Y + 32.,
            width / zoom - 32.,
            (height - HEADER) / zoom + ORIGIN_Y - 24.,
        ];
        let background = el()
            .key("page-background")
            .absolute(0., 0.)
            .size(pw, ph)
            .cursor(Cursor::Text)
            .on_drag(move |s: &mut State, d, tx| {
                if d.phase == Phase::Up && !d.cancelled() && d.dx.abs() + d.dy.abs() < 4. {
                    s.create_in_canvas(d.x, d.y + ORIGIN_Y, bounds);
                    tx.blur();
                    tx.instant();
                }
            });
        let mut page = el()
            .key("page")
            .on_drawn_layout_tx(|s: &mut State, rect, tx| {
                let Some(viewport) = s.canvas_viewport_rect else { tx.live_only(); return; };
                let offset = [viewport[0] - rect[0], viewport[1] - rect[1]];
                if !crate::canvas_document::refresh_visible(s, [viewport[2], viewport[3]], offset) { tx.live_only(); }
            })
            .size(pw, ph)
            .absolute(0., 0.)
            .origin(0., 0.)
            .scale(zoom)
            .child(background);
        // Active cards own their visible borders even when another transparent editor overlaps.
        // Reorder only the view: saved card order and keyed drag targets stay unchanged.
        let layer = |b: &crate::model::Block| {
            if s.selected.as_deref() == Some(&b.id) { 2 }
            else if crate::containers::is_focused(s, b) { 1 }
            else { 0 }
        };
        for b in (0..=2).flat_map(|z| n.blocks.iter().filter(move |b| layer(b) == z)) {
            if s.canvas_visible.contains(&b.id) || s.draft.as_deref() == Some(&b.id) || s.selected.as_deref() == Some(&b.id) {
                page = page.child(crate::containers::view(s, b, bounds));
            } else {
                // Keep the target key and exact saved geometry so Rune can reveal any card,
                // including a search result or keyboard target outside the mounted window.
                page = page.child(el().key(format!("block-{}", b.id))
                    .absolute(b.x, (b.y - ORIGIN_Y).max(8.)).size(b.w, b.h).inert().a11y_hidden());
            }
        }
        let extent = el()
            .key("page-extent")
            .size(pw * zoom, ph * zoom)
            .child(page);
        let viewport = el()
            .key(s.scroll_key())
            .absolute(0., HEADER)
            .size(width, height - HEADER)
            .scroll_xy()
            .clip_rounded(0.)
            .scroll_margin(20.)
            .on_scroll_offset(move |pos| {
                if scroll.get() != pos {
                    scroll.set(pos);
                    if let Some(wake) = &waker { wake(); }
                }
            })
            .on_wheel(move |s: &mut State, e, tx| {
                s.editor.active.slash = None; s.editor.active.popover = None;
                s.editor.active.slash = None;
                s.editor.active.popover = None;
                if e.mods.cmd {
                    s.zoom_at((e.dy * 0.002).exp(), [e.x, e.y], tx);
                    true
                } else {
                    false
                }
            })
            .on_magnify(|s: &mut State, e, tx| {
                s.editor.active.slash = None; s.editor.active.popover = None;
                s.editor.active.slash = None;
                s.editor.active.popover = None;
                s.zoom_at((1. + e.delta).max(0.1), [e.x, e.y], tx);
                true
            })
            .on_drop_files(|s: &mut State, paths, tx| {
                let (documents, images): (Vec<_>, Vec<_>) = paths.iter().partition(|p| crate::import::is_markdown(p));
                for path in images { s.import_image(path); }
                if !documents.is_empty() {
                    let book = s.book.clone();
                    crate::import::import_files(s, &book, documents.into_iter().map(|p| p.as_path()), tx);
                    return;
                }
                if let Some(id) = &s.selected {
                    tx.scroll_into_view(format!("block-{id}"));
                }
            })
            .accepts_extensions(&["png", "jpg", "jpeg", "md", "markdown"])
            .child(extent)
            .scroll_bars_styled(crate::ui::editor_scrollbar(s.theme()));
        canvas = canvas
            .child(viewport)
            .child(zoom_control(s, n.zoom, width, height));
        if n.blocks.is_empty() {
            canvas = canvas.child(
                el().key("canvas-start-hint").absolute(24., HEADER + 64.).w(width - 48.).center().inert()
                    .child(label(s.tr("点击任意位置开始书写"), 13., 400., s.theme().choose(0x8b8f97, 0x9297a3)).inert()),
            );
        }
    } else {
        canvas = canvas.child(
            col()
                .absolute(80., 280.)
                .w(width - 160.)
                .center()
                .gap(14.)
                .child(
                    el().size(64., 64.)
                        .radius(16.)
                        .fill(c(s.theme().choose(0xf1f3f8, crate::ui::editor_colors(s.theme()).inset)))
                        .border(1., c(s.theme().choose(0xe4e8f0, crate::ui::editor_colors(s.theme()).border)))
                        .center()
                        .child(ico(&LAYERS, 30.).color(c(s.theme().choose(0x6e7787, 0xb3b6bf)))),
                )
                .child(label(s.tr("给灵感留一点空间"), 22., 560., s.theme().choose(0x3b4252, 0xe6e7eb)).mt(4.))
                .child(label(
                    s.tr(if s.book_order.is_empty() { "创建空间，开始记录" } else { "在左侧新建一篇笔记，开启自由画布记录" }),
                    15.,
                    400.,
                    s.theme().choose(0x7d8594, 0x9297a3),
                )),
        );
    }
    canvas
}
fn zoom_control(s: &State, zoom: f32, width: f32, height: f32) -> El {
    const W: f32 = 126.;
    const H: f32 = crate::ui::BOTTOM_CONTROL_HEIGHT;
    const INSET: f32 = crate::ui::BOTTOM_CONTROL_INSET;
    let center = [width / 2., (height - HEADER) / 2.];
    let pct = (zoom * 100.).round();
    let kit = crate::ui::kit(s);
    let zoom_roll = rune_kit::RollingText::new("zoom-roll", format!("{pct}%"))
        .value(pct as f64)
        .font(crate::ui::font(13., 500.).lh(18.))
        .color(c(s.theme().choose(0x4b5262, 0xb3b6bf)))
        .build(&kit);
    row()
        .key("zoom-control")
        .absolute(width - W - 16., height - H - INSET)
        .size(W, H)
        .px(3.)
        .center_y()
        .radius(10.)
        .fill(c(s.theme().choose(0xffffff, crate::ui::editor_colors(s.theme()).raised)).alpha(0.95))
        .border(1., c(s.theme().choose(0xdde2ea, crate::ui::editor_colors(s.theme()).border)))
        .shadow(2., 8., c(0x1f2633).alpha(0.07))
        .on_press(|_: &mut State| {})
        .child(
            el().key("zoom-out")
                .size(30., 30.)
                .press_scale(0.92)
                .center()
                .radius(7.)
                .hover(c(s.theme().choose(0xf0f2f6, crate::ui::editor_colors(s.theme()).inset)))
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut State, tx| {
                    s.zoom_at(1. / 1.1, center, tx);
                    tx.instant();
                })
                .help(s.tr("缩小"))
                .child(ico(&MINUS, 15.).color(c(s.theme().choose(0x4b5262, 0xb3b6bf)))),
        )
        .child(el().size(1., 14.).fill(c(s.theme().choose(0xe6e9ef, crate::ui::editor_colors(s.theme()).border))))
        .child(
            row()
                .key("zoom-reset")
                .size(58., 30.)
                .press_scale(0.94)
                .center()
                .radius(7.)
                .hover(c(s.theme().choose(0xf0f2f6, crate::ui::editor_colors(s.theme()).inset)))
                .cursor(Cursor::Pointer)
                .on_press_tx(|s: &mut State, tx| {
                    if let Some(n) = s.note_mut() {
                        n.zoom = 1.;
                        n.pan = [0., 0.];
                    }
                    s.pending_scroll = Some([0., 0.]);
                    s.save();
                    tx.instant();
                })
                .help(s.tr("重置视图 · ⌘0"))
                .child(zoom_roll),
        )
        .child(el().size(1., 14.).fill(c(s.theme().choose(0xe6e9ef, crate::ui::editor_colors(s.theme()).border))))
        .child(
            el().key("zoom-in")
                .size(30., 30.)
                .press_scale(0.92)
                .center()
                .radius(7.)
                .hover(c(s.theme().choose(0xf0f2f6, crate::ui::editor_colors(s.theme()).inset)))
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut State, tx| {
                    s.zoom_at(1.1, center, tx);
                    tx.instant();
                })
                .help(s.tr("放大"))
                .child(ico(&PLUS, 15.).color(c(s.theme().choose(0x4b5262, 0xb3b6bf)))),
        )
}
