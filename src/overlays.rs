//! Shared surfaces and lifecycle motion for Echo-owned overlays. Native menus stay AppKit-owned.
use crate::{icons::ico, model::State, ui::{c, editor_colors, font, label, theme_ink, theme_muted}};
use rune::{El, Spring, SpringPair, Transition, Tx, col, el, row};

pub fn fade_transition() -> Transition {
    Transition::opacity()
        .springs(SpringPair::new(Spring::new(0.20, 0.), Spring::new(0.13, 0.)))
}

pub fn transition(distance: f32) -> Transition {
    fade_transition().combined(Transition::offset(0., distance))
}

/// A single moving surface. Content updates stay immediate, including filtering and typing.
pub fn surface(s: &State, content: El, radius: f32) -> El {
    let dark = s.theme().is_dark();
    content.radius(radius).fill(c(editor_colors(s.theme()).raised))
        .border(0.75, c(theme_ink(s.theme())).alpha(if dark { 0.15 } else { 0.10 }))
        .shadow(2., 6., c(0x000000).alpha(if dark { 0.20 } else { 0.05 }))
        .shadow(10., 28., c(0x000000).alpha(if dark { 0.30 } else { 0.12 }))
        .motion(Spring::NONE).instant_changes().transition(transition(6.))
}

pub const DIALOG_PADDING: f32 = 24.;
pub const DIALOG_WIDTH: f32 = 460.;

pub fn dialog_surface(s: &State, content: El) -> El {
    let dark = s.theme().is_dark();
    content.radius(16.).fill(c(editor_colors(s.theme()).raised))
        .border(1., c(theme_ink(s.theme())).alpha(if dark { 0.13 } else { 0.075 }))
        .shadow(2., 5., c(0x101014).alpha(0.07))
        .shadow(14., 42., c(0x101014).alpha(if dark { 0.36 } else { 0.15 }))
        .motion(Spring::NONE).instant_changes().transition(transition(8.))
}

pub fn close_button(s: &State, key: &str, close: fn(&mut State, &mut Tx)) -> El {
    let ink = c(theme_muted(s.theme()));
    el().key(key).size(28., 28.).radius(14.).center().focusable(true)
        .role("button").label(s.tr("关闭")).help(s.tr("关闭")).cursor(rune::Cursor::Pointer)
        .hover(ink.alpha(0.10)).press(ink.alpha(0.16))
        .focus_ring(2., 2., c(crate::ui::accent(s)))
        .child(ico(crate::ui::kit(s).glyph.close, 16.).color(ink)).on_press_tx(close)
}

pub fn button(s: &State, key: &str, title: &str, primary: bool, danger: bool) -> El {
    let theme = s.theme();
    let fill = if danger { theme.choose(0xb03935, 0x762e2d) }
        else if primary { theme_ink(theme) } else { editor_colors(theme).inset };
    let ink = if danger { 0xffffff } else if primary { editor_colors(theme).page } else { theme_ink(theme) };
    el().key(key).h(38.).min_w(72.).px(16.).radius(11.).center()
        .focusable(true).role("button").label(title).cursor(rune::Cursor::Pointer)
        .fill(c(fill)).hover(c(fill).mix(c(ink), 0.07)).press(c(fill).mix(c(ink), 0.12))
        .press_scale(0.98).focus_ring(2., 2., c(crate::ui::accent(s)))
        .child(label(title, 13., 550., ink))
}

pub fn dialog(s: &State, window: [f32; 2], key: &str, title: &str,
    body: El, close: fn(&mut State, &mut Tx)) -> Vec<El> {
    let [w, h] = window;
    let panel = dialog_surface(s, col().key(format!("{key}-dialog"))
        .role("dialog").label(title).w_full().max_w(DIALOG_WIDTH)
        .padding(DIALOG_PADDING).gap(20.)
        .child(row().w_full().gap(16.).center_y()
            .child(rune::text(title).font(font(21., 600.).lh(29.)).color(c(theme_ink(s.theme()))).grow(1.).min_w(0.).wrap())
            .child(close_button(s, &format!("{key}-close"), close)))
        .child(body)).modal_tx(true, close);
    vec![
        el().key(format!("{key}-scrim")).absolute(0., 0.).size(w, h)
            .fill(c(0x15151a).alpha(if s.theme().is_dark() { 0.44 } else { 0.22 }))
            .transition(Transition::opacity().springs(SpringPair::new(Spring::new(0.20, 0.), Spring::new(0.13, 0.))))
            .inert().a11y_hidden(),
        el().key(format!("{key}-layer")).absolute(0., 0.).size(w, h).padding(16.).center()
            .transition(Transition::identity()).child(panel),
    ]
}
