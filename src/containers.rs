use crate::{
    icons::*,
    model::{Block, Kind, State},
    ui::c,
};
use rune::{Cursor, El, Phase, el};

pub(crate) fn is_focused(s: &State, b: &Block) -> bool {
    s.editor.canvas_owner() == Some(&b.id) && s.editor.active.focused
}

pub fn view(s: &State, b: &Block, bounds: [f32; 4]) -> El {
    let textual = b.is_textual();
    let focused = is_focused(s, b);
    let show =
        s.hovered.as_deref() == Some(&b.id) || focused || s.selected.as_deref() == Some(&b.id);
    let mut node = el()
        .key(format!("block-{}", b.id))
        .absolute(b.x, (b.y - crate::canvas::ORIGIN_Y).max(8.))
        .w(b.w);
    if s.arrange_anim_until.is_some() {
        let rank = s.note().map_or(0, |n| {
            n.blocks
                .iter()
                .filter(|other| {
                    other.id != b.id
                        && (other.y, other.x)
                            .partial_cmp(&(b.y, b.x))
                            .is_some_and(|ord| ord.is_lt())
                })
                .count()
        });
        node = node.motion(rune::Spring::new(0.36 + (rank.min(8) as f32) * 0.022, 0.));
    } else {
        node = node.instant_changes();
    }
    if textual && s.draft.as_deref() == Some(&b.id) {
        // Keep Rune's text layout and IME anchor. A second click belongs to the canvas.
        return node.child(crate::doc_editor::render_embedded(s, b)).inert();
    }
    if textual {
        node = node.radius(CARD_RADIUS).continuous_corners()
            .fill(c(s.theme().choose(0xffffff, 0x2b2a30)))
            .border(0.8, c(s.theme().choose(0xdad8df, 0x45434b)))
            .shadow(2., 5., c(0x282032).alpha(0.065));
    }
    let hover_id = b.id.clone();
    // One tracking area includes the content, border, and all external controls.
    // It is painted before the content and has no press/drag handler.
    node = node.child(
        el().key(format!("container-hover-{}", b.id))
            .pin(
                Some(-5.),
                Some(if b.kind == Kind::Table { -32. } else { -24. }),
                Some(if b.kind == Kind::Table { -30. } else { -22. }),
                Some(-22.),
            )
            .on_hover_change(move |s: &mut State, on, _| {
                if on {
                    s.hovered = Some(hover_id.clone());
                } else if s.hovered.as_deref() == Some(&hover_id) {
                    s.hovered = None;
                }
            }),
    );
    if textual {
        node = node.child(crate::doc_editor::render_embedded(s, b));
        let id = b.id.clone();
        node = node.on_layout_tx(move |s: &mut State, r, tx| {
            if !s.resize_text_height(&id, r[3]) {
                tx.live_only();
            }
        });
    } else {
        node = node.h(b.h).child(if b.kind == Kind::Divider {
            el().w_full().h_full().center_y().child(
                el().w_full().h(1.).fill(c(s
                    .theme()
                    .choose(0xdfe2e8, crate::ui::editor_colors(s.theme()).border))),
            )
        } else if b.kind == Kind::Shape {
            el().w_full()
                .h_full()
                .radius(12.)
                .border(1.5, c(s.theme().choose(0xbcb1c6, 0x685c70)))
                .fill(c(s.theme().choose(0xe9e2ef, 0x3c3542)))
        } else {
            let radius = if b.kind == Kind::Handwritten { 5. } else { 10. };
            let mut img = el().w_full().h_full().radius(radius).clip_rounded(radius).border(
                1.,
                if b.kind == Kind::Handwritten {
                    c(s.theme()
                        .choose(0xeaecef, crate::ui::editor_colors(s.theme()).inset))
                } else {
                    c(0x1f2633).alpha(0.08)
                },
            );
            if let Some(image) = s.note_image(&b.image, None) {
                img = img.pixels_fit(image.pixels(), rune::SurfaceFit::Fill);
            }
            img
        });
        let id = b.id.clone();
        node = node
            .cursor(Cursor::Move)
            .on_drag(move |s: &mut State, d, tx| move_container(s, &id, bounds, d, tx));
    }
    node = node.child(move_frame(s, b, textual, bounds));
    if show {
        node = node.child(chrome(s, b, textual, focused, bounds));
    }
    node
}
fn begin_gesture(s: &mut State, id: &str, bounds: [f32; 4], tx: &mut rune::Tx) {
    s.arrange_anim_until = None;
    s.drag_bounds = s.visible_canvas_bounds(bounds);
    s.finish_edit();
    s.begin_canvas_gesture();
    tx.blur();
    s.selected = Some(id.into());
}
fn drag_canvas_block(s: &mut State, id: &str, d: &mut rune::Drag, tx: &mut rune::Tx) {
    let (Some(view), Some(note)) = (s.canvas_viewport_rect, s.note()) else {
        return;
    };
    let zoom = note.zoom;
    let scroll = s.scroll_pos.get();
    let mut offset = scroll.offset;
    let pointer = d.window_point();
    let travel = [d.dx, d.dy];
    let [x, y, scroll_x, scroll_y] = s.drag_start;
    let original_scroll = [scroll_x, scroll_y];
    let mut position = [x, y];
    let dt = (s.clock as f32 - d.data).clamp(0., 0.05);
    d.data = s.clock as f32;
    let mut at_edge = false;
    for axis in 0..2 {
        let local = pointer[axis] - view[axis];
        let inside = local.clamp(0., view[axis + 2]);
        let zone = 44.;
        let edge = ((inside - (view[axis + 2] - zone)).max(0.) - (zone - inside).max(0.)) / zone;
        let speed = 720. * edge * edge.abs();
        if speed > 0. || (speed < 0. && offset[axis] > 0.) {
            at_edge = true;
            offset[axis] = (offset[axis] + speed * dt).clamp(0., scroll.max[axis]);
        }
        // Preserve the original grab point as the viewport moves under the pointer.
        position[axis] +=
            (travel[axis] + inside - local + offset[axis] - original_scroll[axis]) / zoom;
    }
    if let Some(b) = s.block_mut(id) {
        b.x = position[0].max(12.);
        b.y = position[1].max(crate::canvas::ORIGIN_Y + 32.);
    }
    if offset != scroll.offset {
        tx.scroll_to_instant(s.scroll_key(), offset[0], offset[1]);
    }
    if at_edge {
        tx.drag_repeat_after(std::time::Duration::from_millis(16));
    }
    tx.instant();
}

fn move_container(
    s: &mut State,
    id: &str,
    bounds: [f32; 4],
    d: &mut rune::Drag,
    tx: &mut rune::Tx,
) {
    match d.phase {
        Phase::Down => {
            begin_gesture(s, id, bounds, tx);
            let [scroll_x, scroll_y] = s.scroll_pos.get().offset;
            if let Some(b) = s.block_mut(id) {
                s.drag_start = [b.x, b.y, scroll_x, scroll_y];
            }
            d.data = s.clock as f32;
        }
        Phase::Move => drag_canvas_block(s, id, d, tx),
        Phase::Up => {
            if d.cancelled() {
                let start = s.drag_start;
                if let Some(b) = s.block_mut(id) {
                    b.x = start[0];
                    b.y = start[1];
                }
                s.pending_scroll = Some([start[2], start[3]]);
                s.end_canvas_gesture(false);
            } else {
                let start = s.drag_start;
                let moved = s.note().and_then(|n| n.blocks.iter().find(|b| b.id == id))
                    .is_some_and(|b| [b.x, b.y] != [start[0], start[1]]);
                if moved && tx.mods().shift {
                    let target = s
                        .note()
                        .and_then(|n| {
                            let source = n.blocks.iter().find(|b| b.id == id)?;
                            let [x, y] = [source.x + 20., source.y + 20.];
                            n.blocks.iter().find(|b| {
                                b.id != id
                                    && x >= b.x
                                    && x <= b.x + b.w
                                    && y >= b.y
                                    && y <= b.y + b.h
                            })
                        })
                        .map(|b| b.id.clone());
                    if let Some(target) = target {
                        s.merge_containers(id, &target);
                    }
                }
                let changed = moved && s.block_mut(id).is_some() && s.draft.as_deref() != Some(id);
                s.end_canvas_gesture(changed);
                if changed {
                    s.save_canvas_geometry();
                }
            }
        }
    }
}
fn context_menu(s: &mut State, id: &str, press: rune::PointerPress, tx: &mut rune::Tx) {
    s.selected = Some(id.into());
    tx.popup_menu(
        rune::PopupMenu::new()
            .item(rune::PopupItem::new("duplicate", s.tr("复制容器")).symbol("doc.on.doc"))
            .separator()
            .item(rune::PopupItem::new("delete", s.tr("删除容器")).symbol("trash")),
        press,
        |s: &mut State, picked, _| match picked {
            Some("duplicate") => s.duplicate(),
            Some("delete") => s.delete_selected(),
            _ => {}
        },
    );
}
/// A narrow, continuous perimeter. Its center is empty so text keeps native input.
fn move_frame(s: &State, b: &Block, textual: bool, bounds: [f32; 4]) -> El {
    let inner = if textual { 4. } else { 6. };
    let band = inner + 5.;
    let mut frame =
        el().key(format!("move-frame-{}", b.id))
            .pin(Some(0.), Some(0.), Some(0.), Some(0.));
    let edges = [
        (
            "top",
            el().pin(Some(-5.), Some(-5.), Some(-5.), None).h(band),
        ),
        (
            "bottom",
            el().pin(Some(-5.), None, Some(-5.), Some(-5.)).h(band),
        ),
        (
            "left",
            el().pin(Some(-5.), Some(inner), None, Some(inner)).w(band),
        ),
        (
            "right",
            el().pin(None, Some(inner), Some(-5.), Some(inner)).w(band),
        ),
    ];
    for (edge, node) in edges {
        let id = b.id.clone();
        let menu = id.clone();
        frame = frame.child(
            node.key(format!("move-edge-{edge}-{}", b.id))
                .cursor(Cursor::Move)
                .help(s.tr("拖动边框移动容器；按住 Shift 拖到其他文字容器可合并"))
                .on_drag(move |s: &mut State, d, tx| move_container(s, &id, bounds, d, tx))
                .on_secondary_press(move |s: &mut State, press, tx| {
                    context_menu(s, &menu, press, tx)
                }),
        );
    }
    frame
}
#[derive(Clone, Copy)]
enum ResizeAxis {
    Width,
    Height,
    Both,
}
fn resize_container(
    s: &mut State,
    id: &str,
    axis: ResizeAxis,
    bounds: [f32; 4],
    d: &mut rune::Drag,
    tx: &mut rune::Tx,
) {
    match d.phase {
        Phase::Down => {
            begin_gesture(s, id, bounds, tx);
            if let Some(b) = s.block_mut(id) {
                s.drag_start = [b.w, b.h, b.min_h, 0.];
            }
        }
        Phase::Move => {
            let start = s.drag_start;
            let zoom = s.note().map_or(1., |n| n.zoom);
            if let Some(b) = s.block_mut(id) {
                let textual = matches!(
                    b.kind,
                    Kind::Text | Kind::Tasks | Kind::Code | Kind::Callout | Kind::Table
                );
                if matches!(axis, ResizeAxis::Width | ResizeAxis::Both) {
                    b.w = (start[0] + d.dx / zoom).max(if let Some(t) = &b.table {
                        t.cols() as f32 * 72. + 2.
                    } else if textual {
                        150.
                    } else {
                        100.
                    });
                }
                if matches!(axis, ResizeAxis::Height | ResizeAxis::Both) {
                    if textual {
                        b.min_h = (start[1] + d.dy / zoom).max(40.);
                    } else if matches!(axis, ResizeAxis::Both) && b.kind != Kind::Shape {
                        b.h = b.w * start[1] / start[0];
                    } else {
                        b.h = (start[1] + d.dy / zoom).max(60.);
                    }
                }
            }
            s.constrain_resize(id);
            tx.instant();
        }
        Phase::Up => {
            if d.cancelled() {
                let start = s.drag_start;
                if let Some(b) = s.block_mut(id) {
                    b.w = start[0];
                    b.min_h = start[2];
                    if !matches!(
                        b.kind,
                        Kind::Text | Kind::Tasks | Kind::Code | Kind::Callout | Kind::Table
                    ) {
                        b.h = start[1];
                    }
                }
                s.end_canvas_gesture(false);
                tx.instant();
            } else {
                let start = s.drag_start;
                let changed = s.draft.as_deref() != Some(id) && s.block_mut(id).is_some_and(|b| {
                    [b.w, b.h, b.min_h] != [start[0], start[1], start[2]]
                });
                s.end_canvas_gesture(changed);
                if changed {
                    s.save_canvas_geometry();
                }
            }
        }
    }
}
const CARD_RADIUS: f32 = 10.;

fn chrome(s: &State, b: &Block, textual: bool, focused: bool, bounds: [f32; 4]) -> El {
    let hovered = s.hovered.as_deref() == Some(&b.id);
    let selected = s.selected.as_deref() == Some(&b.id);
    let accent = if s.sidebar_material.is_dawn() {
        c(s.theme().choose(0x967bdf, 0xb39beb))
    } else {
        c(crate::ui::accent(s))
    };
    let border_color = if selected || focused { accent } else { accent.alpha(0.52) };
    let radius = match b.kind {
        Kind::Image => 10.,
        Kind::Handwritten => 5.,
        Kind::Shape => 12.,
        Kind::Divider => 3.,
        _ => CARD_RADIUS,
    };
    let handle_color = accent;
    let mut overlay = el()
        .key("container-chrome")
        .pin(Some(0.), Some(0.), Some(0.), Some(0.))
        .radius(radius)
        .transition(rune::Transition::opacity().springs(rune::SpringPair::new(
            rune::Spring::new(0.14, 0.),
            rune::Spring::new(0.10, 0.),
        )))
        .motion(rune::Spring::NONE);
    if hovered || selected || focused {
        overlay = overlay.border(1., border_color);
    }
    let writing = textual && focused && s.editor.active.typing;
    if writing || (!hovered && !selected) {
        return overlay;
    }
    let id = b.id.clone();
    let context = id.clone();
    let handle = el()
        .key(format!("move-handle-{}", b.id))
        .absolute((b.w - 44.) * 0.5, -12.)
        .size(44., 22.)
        .center()
        .child(ico(&GRIP_VERTICAL, 15.).rotate(std::f32::consts::FRAC_PI_2)
            .color(handle_color))
        .cursor(Cursor::Move)
        .help(s.tr("拖动边框或顶部把手移动容器；按住 Shift 可合并"))
        .on_drag(move |s: &mut State, d, tx| move_container(s, &id, bounds, d, tx))
        .on_secondary_press(move |s: &mut State, press, tx| context_menu(s, &context, press, tx));
    overlay = overlay.child(handle);
    let id = b.id.clone();
    let mid_y = ((b.h - 28.) * 0.5).max(6.);
    overlay = overlay.child(
        el().key(format!("width-handle-{}", b.id))
            .absolute(b.w + 4., mid_y)
            .size(14., 28.)
            .radius(7.)
            .hover(handle_color.alpha(0.12))
            .center()
            .child(el().size(5., 13.).radius(2.5)
                .fill(c(crate::ui::editor_colors(s.theme()).raised)).border(1., handle_color))
            .cursor(Cursor::ResizeColumn)
            .help(s.tr("拖动调整宽度"))
            .on_drag(move |s: &mut State, d, tx| {
                resize_container(s, &id, ResizeAxis::Width, bounds, d, tx)
            }),
    );
    let id = b.id.clone();
    overlay = overlay.child(
        el().key(format!("height-handle-{}", b.id))
            .pin(Some((b.w - 28.) * 0.5), None, None, Some(-18.))
            .size(28., 14.)
            .radius(7.)
            .hover(handle_color.alpha(0.12))
            .center()
            .child(el().size(14., 3.).radius(1.5).fill(handle_color))
            .cursor(Cursor::ResizeRow)
            .help(s.tr("拖动调整高度；正文仍会自动增高"))
            .on_drag(move |s: &mut State, d, tx| {
                resize_container(s, &id, ResizeAxis::Height, bounds, d, tx)
            }),
    );
    let id = b.id.clone();
    overlay = overlay.child(
        el().key(format!("size-handle-{}", b.id))
            .pin(None, None, Some(-10.), Some(-10.))
            .size(20., 20.)
            .center()
            .child(
                el().size(8., 8.)
                    .radius(2.5)
                    .fill(c(s
                        .theme()
                        .choose(0xffffff, crate::ui::editor_colors(s.theme()).raised)))
                    .border(1., handle_color)
                    .shadow(1., 3., c(0x1f2633).alpha(0.08)),
            )
            .cursor(Cursor::Image(rune::CursorImage::png(
                include_bytes!("../assets/resize-diagonal.png"),
                [20., 20.],
            )))
            .help(if textual {
                s.tr("拖动同时调整宽度和高度")
            } else {
                s.tr("拖动等比调整图片尺寸")
            })
            .on_drag(move |s: &mut State, d, tx| {
                resize_container(s, &id, ResizeAxis::Both, bounds, d, tx)
            }),
    );
    overlay
}
