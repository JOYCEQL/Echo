use super::*;

pub(super) struct Resize {
    block: u64,
    start: f32,
    preview: f32,
}

fn select(s: &mut State, id: u64, tx: &mut Tx) {
    crate::canvas_document::activate_block(s, id);
    tx.blur();
    let session = &mut s.editor.active;
    session.commit_cell_edit();
    session.focused = true;
    session.typing = false;
    session.table_selection = None;
    session.slash = None;
    session.popover = None;
    session.goal_x = None;
    session.core.set_head(DocPos::new(id, 0), false);
    session.core.block_sel = Some((id, id));
    sync_to_state(s);
    tx.instant();
}

fn resize(
    s: &mut State,
    id: u64,
    width: f32,
    available: f32,
    ratio: f32,
    [sx, sy]: [f32; 2],
    d: &rune::Drag,
) {
    if d.phase == Phase::Down {
        s.editor.active.image_resize = Some(Resize {
            block: id,
            start: width,
            preview: width,
        });
        return;
    }
    let zoom = if s.mode == crate::model::WorkspaceMode::Canvas {
        s.note().map_or(1., |note| note.zoom)
    } else {
        1.
    };
    let session = &mut s.editor.active;
    let Some(r) = session.image_resize.as_mut().filter(|r| r.block == id) else {
        return;
    };
    // Project pointer travel onto the corner diagonal in document coordinates.
    let delta = (sx * d.dx + ratio * sy * d.dy) / ((1. + ratio * ratio) * zoom);
    r.preview = (r.start + delta).clamp(r.start.min(64.), available);
    if d.phase == Phase::Up {
        let next = r.preview;
        let changed = (next - r.start).abs() >= 0.5;
        session.image_resize = None;
        if changed && !d.cancelled() {
            session.core.with_undo(|core| {
                if let Some(DocBlock {
                    kind: DocBlockKind::Image { width, .. },
                    ..
                }) = core.block_mut(id)
                {
                    *width = Some(next);
                }
            });
            sync_to_state(s);
        }
    }
}

fn toolbar(s: &RenderState<'_>, id: u64, align: DocAlign) -> El {
    let colors = crate::ui::editor_colors(s.theme());
    let accent = c(crate::ui::accent(s));
    let mut bar = row()
        .padding(3.)
        .gap(2.)
        .radius(8.)
        .fill(c(colors.raised))
        .border(1., c(colors.border))
        .shadow(2., 6., c(0x141414).alpha(0.14));
    for (key, title, icon, target) in [
        ("left", "左对齐", &ALIGN_LEFT, DocAlign::Left),
        ("center", "居中对齐", &ALIGN_CENTER, DocAlign::Center),
        ("right", "右对齐", &ALIGN_RIGHT, DocAlign::Right),
    ] {
        let active = align == target;
        let mut button = el()
            .key(format!("doc-image-{id}-align-{key}"))
            .size(26., 24.)
            .radius(5.)
            .center()
            .cursor(Cursor::Pointer)
            .role("button")
            .label(s.tr(title))
            .help(s.tr(title))
            .checked(active)
            .hover(c(crate::ui::accent_soft(s)))
            .child(ico(icon, 15.).color(if active {
                accent
            } else {
                c(crate::ui::theme_ink(s.theme()))
            }));
        if active {
            button = button.fill(accent.alpha(0.14));
        }
        bar = bar.child(button.on_press_tx(move |s: &mut State, tx| {
            select(s, id, tx);
            if s.editor
                .active
                .core
                .block(id)
                .is_some_and(|b| b.align != target)
            {
                s.editor.active.core.set_align(&[id], target);
                sync_to_state(s);
            }
        }));
    }
    bar
}

pub(super) fn render(
    s: &RenderState<'_>,
    id: u64,
    src: &str,
    width: Option<f32>,
    caption: &str,
    available: f32,
    align: DocAlign,
) -> El {
    let session = s.editor.active;
    let handle = s.note_image(src, Some(available));
    let image = handle.as_ref().and_then(|img| Some((img, img.source_size()?)));
    let w = session
        .image_resize
        .as_ref()
        .filter(|r| r.block == id)
        .map(|r| r.preview)
        .or(width)
        .or_else(|| image.map(|(_, [width, _])| width as f32))
        .unwrap_or(available)
        .clamp(1., available);
    let selected = session.focused && session.core.block_sel == Some((id, id));
    let accent = c(crate::ui::accent(s));
    let radius = if s.in_canvas() { 8. } else { 5. };
    let mut figure = col().w(w).gap(if s.in_canvas() { 6. } else { 4. });
    let mut external_bar = false;
    let card =
        if let Some((img, [iw, ih])) = image {
            let ratio = ih as f32 / iw as f32;
            let h = w * ratio;
            let mut frame = el()
                .key(format!("doc-image-{id}"))
                .size(w, h)
                .child(
                    el().size(w, h)
                        .radius(radius)
                        .clip_rounded(radius)
                        .pixels(img.pixels()),
                )
                .on_drag(move |s: &mut State, d, tx| {
                    if d.phase == Phase::Down {
                        select(s, id, tx);
                    }
                });
            if selected {
                frame = frame.child(
                    el().absolute(0., 0.)
                        .size(w, h)
                        .radius(radius)
                        .border(1.5, accent),
                );
                for (key, sx, sy) in [
                    ("nw", -1., -1.),
                    ("ne", 1., -1.),
                    ("sw", -1., 1.),
                    ("se", 1., 1.),
                ] {
                    let x = if sx < 0. { 0. } else { w };
                    let y = if sy < 0. { 0. } else { h };
                    frame = frame.child(
                        el().key(format!("doc-image-{id}-{key}"))
                            .absolute(x - 8., y - 8.)
                            .size(16., 16.)
                            .center()
                            .cursor(Cursor::Crosshair)
                            .help(s.tr("拖动等比调整图片尺寸"))
                            .child(
                                el().size(8., 8.)
                                    .radius(3.)
                                    .fill(c(0xffffff))
                                    .border(1.5, accent),
                            )
                            .on_drag(move |s: &mut State, d, tx| {
                                resize(s, id, w, available, ratio, [sx, sy], d);
                                tx.instant();
                            }),
                    );
                }
                if w >= 110. && h >= 48. {
                    frame = frame.child(toolbar(s, id, align).absolute((w - 88.) / 2., 8.));
                } else {
                    external_bar = true;
                }
            }
            frame
        } else {
            let mut placeholder = row();
            if let Some(image) = &handle {
                placeholder = placeholder.pixels(image.pixels());
            }
            placeholder
                .w(w)
                .h(110.)
                .center()
                .gap(8.)
                .radius(8.)
                .fill(c(crate::ui::editor_colors(s.theme()).inset))
                .child(ico(&IMAGE, 18.).color(c(crate::ui::theme_ink(s.theme()))))
                .child(label(
                    if src.is_empty() {
                        s.tr("图片缺失")
                    } else {
                        src
                    },
                    13.,
                    450.,
                    crate::ui::theme_ink(s.theme()),
                ))
        };
    figure = figure.child(card);
    if !caption.is_empty() {
        let (size, ink) = if s.in_canvas() {
            (11.5, s.theme().choose(0x74717d, 0xa9a7b0))
        } else {
            (12.5, s.theme().choose(0x8f959e, 0x9297a3))
        };
        let caption_text = label(caption, size, 420., ink);
        figure = figure.child(if s.in_canvas() {
            row().w(w).gap(6.).center_y()
                .child(el().w(12.).h(1.).fill(accent.alpha(0.48)).inert())
                .child(caption_text.shrink().min_w(0.).wrap())
        } else { caption_text.w(w).wrap() });
    }
    let line = || match align {
        DocAlign::Left => row().w(available),
        DocAlign::Center => row().w(available).justify_center(),
        DocAlign::Right => row().w(available).justify_end(),
    };
    // Rune captures a drag by element path. Keep the figure under the same
    // parents when the toolbar moves outside a small image.
    let mut content = col().w(available).gap(5.).child(line().child(figure));
    if external_bar {
        content = content.child(line().child(toolbar(s, id, align)));
    }
    content
}
