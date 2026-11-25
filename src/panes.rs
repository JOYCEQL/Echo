//! Resizable native workspace panes, with one sizing model for layout and overlays.
use crate::{model::State, ui::c};
use rune::{Cursor, El, Key, Phase, el};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Widths {
    pub folders: Option<f32>,
    pub notes: Option<f32>,
    pub documents: Option<f32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boundary {
    Notes,
    Documents,
}
#[derive(Clone, Copy, Debug)]
pub struct Resize {
    pub boundary: Boundary,
    pub before: Widths,
    pub widths: [f32; 2],
}
pub fn canvas_widths(s: &State, w: f32) -> [f32; 2] {
    let folders = crate::navigation::RAIL_WIDTH;
    let notes = document_width(s, w) - folders;
    [folders, notes]
}
pub fn document_width(s: &State, w: f32) -> f32 {
    s.pane_widths
        .documents
        .unwrap_or(318.)
        .clamp(284., 484_f32.min((w - 380.).max(284.)))
}
/// Keep the workspace and settings attached to the same content edge.
pub fn navigation_width(s: &State, w: f32) -> f32 {
    document_width(s, w)
}
/// A collapsed sidebar gives its edge back to the same window inset as the other sides.
pub fn visible_navigation_width(s: &State, w: f32) -> f32 {
    if s.sidebar_collapsed { crate::ui::WINDOW_INSET } else { navigation_width(s, w) }
}
fn save(s: &mut State) {
    let json = serde_json::to_string(&s.pane_widths).expect("finite pane widths");
    if let Err(err) = s.store.set_preference("pane_widths_light", &json) {
        s.message = Some(format!("{}: {err}", s.tr("无法保存分栏宽度")));
    }
}
fn set(s: &mut State, b: Boundary, value: f32, w: f32) {
    match b {
        Boundary::Notes => {
            s.pane_widths.documents = Some((value + crate::navigation::RAIL_WIDTH).clamp(284., 484_f32.min((w - 380.).max(284.))));
        }
        Boundary::Documents => {
            s.pane_widths.documents = Some(value.clamp(284., 484_f32.min((w - 380.).max(284.))))
        }
    }
}
pub fn divider(s: &State, b: Boundary, x: f32, h: f32, w: f32) -> El {
    let (key, name) = match b {
        Boundary::Notes => ("split-notes", "调整笔记列表宽度"),
        Boundary::Documents => ("split-documents", "调整文档侧栏宽度"),
    };
    let active = s.pane_resize.is_some_and(|d| d.boundary == b);
    let hovered = s.pane_hover == Some(b);
    el().key(key)
        .absolute(x - 4., 0.)
        .size(8., h)
        .cursor(Cursor::ResizeColumn)
        .role("separator")
        .label(s.tr(name))
        .help(s.tr("拖动调整宽度 · 双击恢复默认 · 方向键微调"))
        .focusable(true)
        .on_hover_change(move |s: &mut State, on, _| {
            if on {
                s.pane_hover = Some(b)
            } else if s.pane_hover == Some(b) {
                s.pane_hover = None
            }
        })
        .on_double_press_tx(move |s: &mut State, tx| {
            match b {
                Boundary::Notes => s.pane_widths.documents = None,
                Boundary::Documents => s.pane_widths.documents = None,
            };
            save(s);
            tx.instant();
        })
        .on_drag(move |s: &mut State, d, tx| {
            match d.phase {
                Phase::Down => {
                    s.pane_resize = Some(Resize {
                        boundary: b,
                        before: s.pane_widths,
                        widths: if b == Boundary::Documents {
                            [document_width(s, w), 0.]
                        } else {
                            canvas_widths(s, w)
                        },
                    });
                }
                Phase::Move => {
                    if let Some(start) = s.pane_resize {
                        let initial = if b == Boundary::Notes {
                            start.widths[1]
                        } else {
                            start.widths[0]
                        };
                        set(s, b, initial + d.dx, w);
                    }
                }
                Phase::Up => {
                    if let Some(start) = s.pane_resize.take() {
                        if d.cancelled() {
                            s.pane_widths = start.before
                        } else {
                            save(s);
                        }
                    }
                }
            }
            tx.instant();
        })
        .on_key(move |s: &mut State, e, tx| {
            let delta = match e.key {
                Key::Left => -16.,
                Key::Right => 16.,
                _ => return false,
            };
            let initial = match b {
                Boundary::Notes => canvas_widths(s, w)[1],
                Boundary::Documents => document_width(s, w),
            };
            set(s, b, initial + delta, w);
            save(s);
            tx.instant();
            true
        })
        .child(
            el().absolute(if active || hovered { 2.5 } else { 3.5 }, 0.)
                .size(if active || hovered { 3. } else { 1. }, h)
                .fill(if active {
                    c(crate::ui::accent(s)).alpha(0.75)
                } else if hovered {
                    c(crate::ui::accent(s)).alpha(0.45)
                } else {
                    rune::Color::TRANSPARENT
                })
                .inert(),
        )
}

/// Keep navigation mounted so reversing a toggle continues from its drawn position.
/// One clipped, fixed-width strip prevents notebook labels from reflowing during travel.
pub fn workspace_shell(s: &State, editor: El, w: f32, h: f32, boundary: Boundary) -> El {
    let full = navigation_width(s, w);
    let visible = visible_navigation_width(s, w);
    let spring = rune::Spring::new(0.26, 0.);
    let strip = rune::row().key("navigation-strip").size(full, h)
        .offset(if s.sidebar_collapsed { -full } else { 0. }, 0.)
        .offset_motion(spring)
        .child(crate::navigation::rail(s, crate::navigation::RAIL_WIDTH, h))
        .child(crate::navigation::pages(s, full - crate::navigation::RAIL_WIDTH, h));
    let mut navigation = el().key("navigation-viewport").size(visible, h)
        .clip_rounded(0.).child(strip);
    if s.sidebar_collapsed { navigation = navigation.inert().a11y_hidden(); }
    let mut shell = rune::row().key("app").size(w, h).absolute(0., 0.)
        .motion(spring)
        .child(navigation)
        .child(rune::col().key("editor-column").size(w - visible, h)
            .child({
                let mut surface = el().key("editor-surface").size(w - visible, h - crate::ui::editor_top(s)).mt(crate::ui::editor_top(s))
                    .fill(c(crate::ui::page_surface(s))).clip_rounded(10.);
                if let Some(edge) = crate::ui::page_surface_border(s) {
                    surface = surface.border(0.75, edge);
                }
                surface.child(crate::navigation::switch_surface(format!("page-content-{}", s.current),
                    editor.hold_landing_width()).h_full())
            })
            .child(crate::ui::editor_toolbar(s, w - visible))
            .hold_landing_width());
    if !s.sidebar_collapsed {
        shell = shell.child(divider(s, boundary, full, h, w).transition(rune::Transition::none()));
    }
    shell
}
