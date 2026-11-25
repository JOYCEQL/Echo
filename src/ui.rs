use crate::{
    icons::*,
    model::{State, Theme, WorkspaceMode},
};
use rune::{Color, Cx, El, Family, Icon, TextStyle, el, row, text, text_input};

pub fn c(rgb: u32) -> Color {
    Color {
        r: rune::color::srgb_decode(((rgb >> 16) & 255) as f32 / 255.),
        g: rune::color::srgb_decode(((rgb >> 8) & 255) as f32 / 255.),
        b: rune::color::srgb_decode((rgb & 255) as f32 / 255.),
        a: 1.,
    }
}
pub fn font(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
        .family(Family::Named(crate::platform::UI_FONT))
        .lh(size * 1.5)
}
pub fn label(content: impl Into<String>, size: f32, weight: f32, color: u32) -> El {
    text(content).font(font(size, weight)).color(c(color))
}
pub fn theme_ink(t: Theme) -> u32 {
    t.choose(0x202124, 0xe6e6e4)
}
pub(crate) fn editor_scrollbar(theme: Theme) -> rune_kit::ScrollBarStyle {
    let ink = c(theme_ink(theme));
    rune_kit::ScrollBarStyle::native(ink.alpha(0.32), ink.alpha(0.52))
        .inside().always_visible().width(3.).hover_width(5.)
}
pub fn theme_muted(t: Theme) -> u32 {
    t.choose(0x62666e, 0xb3b3af)
}
pub fn theme_subtle(t: Theme) -> u32 {
    t.choose(0x8b8f97, 0xa3a39f)
}
// The first Look stop is the interaction hue. Changing the other gradient stops
// must not make controls drift between colours. Fixed perceptual lightness keeps
// custom pale/dark colours legible; low chroma keeps the editor quiet.
fn interaction_tone(s: &State, dark: bool) -> Color {
    let tint = s.sidebar_material.colors[0];
    let hue = (tint.hue as f32).to_radians();
    let tint_l = tint.lightness as f32 / 100.;
    let source_chroma = crate::material::Tint::gamut(tint_l, hue) * tint.saturation as f32 / 100.;
    // Neutral white/gray surfaces resolve to crisp graphite (light) or silver (dark)
    // instead of a washed-out mid-gray, blending smoothly into colored themes.
    let t = ((source_chroma - 0.008) / 0.020).clamp(0., 1.);
    let color_weight = t * t * (3. - 2. * t);
    let lightness = if dark {
        0.85 + (0.78 - 0.85) * color_weight
    } else {
        0.24 + (0.48 - 0.24) * color_weight
    };
    let chroma = crate::material::Tint::gamut(lightness, hue)
        .min(0.12) * tint.saturation as f32 / 100.;
    let [r, g, b] = crate::material::Tint::linear(lightness, chroma, hue);
    Color { r, g, b, a: 1. }
}
fn rgb(color: Color) -> u32 {
    let byte = |v: f32| (rune::color::srgb_encode(v.clamp(0., 1.)) * 255.).round() as u32;
    (byte(color.r) << 16) | (byte(color.g) << 8) | byte(color.b)
}
pub fn accent(s: &State) -> u32 {
    rgb(interaction_tone(s, s.theme().is_dark()))
}
pub fn accent_soft(s: &State) -> u32 {
    // Mix in OKLab to avoid the milky, overbright selection of linear-light alpha.
    let base = c(editor_colors(s.theme()).raised).to_oklab();
    let tint = c(accent(s)).to_oklab();
    rgb(Color::from_oklab(std::array::from_fn(|i| base[i] + (tint[i] - base[i]) * 0.16)))
}
pub fn accent_on_dark(s: &State) -> u32 {
    rgb(interaction_tone(s, true))
}
pub fn kit(s: &State) -> rune_kit::Kit {
    rune_kit::Kit::from_palette_focus(rune_kit::PaletteSeed {
        bg: c(editor_colors(s.theme()).raised),
        fg: c(theme_ink(s.theme())),
        accent: c(accent(s)),
    }, rune_kit::PaletteFocus::Accent)
}
#[derive(Clone, Copy)]
pub struct EditorColors {
    pub page: u32,
    pub raised: u32,
    pub inset: u32,
    pub border: u32,
}
// Soft charcoal reading surface; inset blocks and raised controls share neutral grays.
// Keep the page lighter than the native window backdrop without tinting the content.
pub fn editor_colors(t: Theme) -> EditorColors {
    EditorColors {
        page: t.choose(0xffffff, 0x262625),
        raised: t.choose(0xffffff, 0x30302f),
        inset: t.choose(0xf5f6f8, 0x2d2d2c),
        border: t.choose(0xe8e9ed, 0x41413f),
    }
}
pub fn page_surface(s: &State) -> u32 {
    if s.sidebar_material.is_dawn() && !s.theme().is_dark() { 0xf8f7fc }
    else { editor_colors(s.theme()).page }
}
pub fn sidebar_tint(t: Theme) -> Color {
    c(t.choose(0xf4f4f6, 0x24262b))
}
pub(crate) fn sidebar_surface(s: &State) -> Color {
    s.sidebar_material.colors[0].color().alpha(s.sidebar_material.opacity())
}
pub fn nav_dark(s: &State) -> bool {
    let colors=&s.sidebar_material.colors;
    let luminance=(0..colors.len()).map(|i| {let c=s.sidebar_material.color(i,s.system_dark.get());0.2126*c.r+0.7152*c.g+0.0722*c.b}).sum::<f32>()/colors.len() as f32;
    // The tint is composited over the system material. A dark tint with high
    // transparency can still produce a light surface in the light system theme.
    let opacity = s.sidebar_material.opacity();
    let material_luminance = if s.theme().is_dark() { 0.04 } else { 0.9 };
    luminance * opacity + material_luminance * (1. - opacity) < 0.27
}
pub fn nav_ink(s: &State) -> u32 {
    if nav_dark(s) {
        0xf4f4f5
    } else if s.sidebar_material.is_dawn() {
        0x292c47
    } else if s.sidebar_material.is_porcelain() {
        0x18191c
    } else if s.sidebar_material.is_warm_white() {
        0x262522
    } else {
        0x202124
    }
}
pub fn nav_muted(s: &State) -> u32 {
    if nav_dark(s) {
        0xc4c8cf
    } else if s.sidebar_material.is_porcelain() {
        0x636770
    } else if s.sidebar_material.is_warm_white() {
        0x6c6962
    } else {
        0x62666e
    }
}
pub fn nav_subtle(s: &State) -> u32 {
    if nav_dark(s) {
        0xaeb4bf
    } else if s.sidebar_material.is_porcelain() {
        0x8c9098
    } else if s.sidebar_material.is_warm_white() {
        0x8e8c86
    } else {
        0x8b8f97
    }
}
pub fn nav_selection(s: &State) -> Color {
    if nav_dark(s) {
        c(0xf4f4f5).alpha(0.10)
    } else if s.sidebar_material.is_dawn() {
        c(0xffffff).alpha(0.58)
    } else if s.sidebar_material.is_porcelain() {
        c(0xffffff)
    } else if s.sidebar_material.is_warm_white() {
        c(0xfcfbf9)
    } else {
        c(0x202124).alpha(0.065)
    }
}
pub fn nav_selection_border(s: &State) -> Option<Color> {
    if nav_dark(s) {
        None
    } else if s.sidebar_material.is_porcelain() {
        Some(c(0xe6e8ec))
    } else if s.sidebar_material.is_warm_white() {
        Some(c(0xe6e4dd))
    } else {
        None
    }
}
pub fn page_surface_border(s: &State) -> Option<Color> {
    if s.theme().is_dark() {
        None
    } else if s.sidebar_material.is_porcelain() {
        Some(c(0xe8eaee))
    } else if s.sidebar_material.is_warm_white() {
        Some(c(0xe7e5de))
    } else {
        None
    }
}
pub fn nav_hover(s:&State)->Color {c(nav_ink(s)).alpha(0.055)}
// Native macOS buttons are 14 pt tall, placed at y=10: all chrome shares y=17.
pub const TRAFFIC_LIGHT_Y: f32 = 10.;
pub const CHROME_CENTER_Y: f32 = TRAFFIC_LIGHT_Y + rune::Chrome::TRAFFIC_LIGHT / 2.;
pub const CHROME_HEIGHT: f32 = CHROME_CENTER_Y * 2.;
pub const WINDOW_INSET: f32 = 8.;
// Editor controls keep their own in-page inset; sidebar actions end at the page surface edge.
pub const BOTTOM_CONTROL_INSET: f32 = 16.;
pub const BOTTOM_CONTROL_HEIGHT: f32 = 36.;
pub const EDITOR_TOOLBAR_HEIGHT: f32 = 40.;
pub fn window_content_top(s: &State) -> f32 {
    let chrome = s.window_chrome.get();
    if chrome.trailing > 0. { EDITOR_TOOLBAR_HEIGHT.max(chrome.height) } else { WINDOW_INSET }
}
pub fn editor_top(s: &State) -> f32 {
    if s.sidebar_collapsed { EDITOR_TOOLBAR_HEIGHT.max(window_content_top(s)) } else { window_content_top(s) }
}
pub fn chrome_muted(s: &State) -> u32 { nav_muted(s) }
fn window_chrome(s: &State, _w: f32) -> El {
    let width = if s.sidebar_collapsed { crate::platform::CHROME_LEADING } else { crate::navigation::RAIL_WIDTH };
    el().key("window-chrome").absolute(0., 0.)
        .size(width, CHROME_HEIGHT).window_drag()
}

/// Controls sit on the shared window backdrop, above the independent page surface.
pub fn editor_toolbar(s: &State, width: f32) -> El {
    let muted = nav_muted(s);
    let caption = s.window_chrome.get();
    let controls_width = width - caption.trailing;
    let control_y = CHROME_CENTER_Y - 14.;
    let mut bar = el().key("editor-toolbar").absolute(0., 0.).size(width, EDITOR_TOOLBAR_HEIGHT);
    if s.sidebar_collapsed || caption.trailing > 0. { bar = bar.window_drag(); }
    if s.sidebar_collapsed {
        bar = bar
            .child(crate::navigation::sidebar_toggle(s).absolute(crate::platform::CHROME_LEADING, control_y))
            .child(crate::navigation::search_button(s).absolute(crate::platform::CHROME_LEADING + 36., control_y));
    }
    let options_y = if s.sidebar_collapsed || caption.trailing > 0. { control_y } else { WINDOW_INSET + 4. };
    if s.book_order.contains(&s.book) {
        let create = el().key("editor-new-page").size(24., 24.).radius(7.).center()
            .role("button").label(s.tr("新建页面")).help(s.tr("新建页面 · ⌘N"))
            .focusable(true).cursor(rune::Cursor::Pointer).hover(nav_hover(s))
            .child(crate::icons::ico(&crate::icons::PLUS, 17.).color(c(chrome_muted(s))))
            .on_press_tx(|s: &mut State, tx| {
                let book = s.book.clone();
                crate::library_ui::open_new_page(s, &book, None, tx);
            });
        let import = el().key("editor-import-markdown").size(24., 24.).radius(7.).center()
            .role("button").label(s.tr("导入 Markdown")).help(s.tr("导入 Markdown"))
            .focusable(true).cursor(rune::Cursor::Pointer).hover(nav_hover(s))
            .child(crate::icons::ico(&crate::icons::FOLDER_OPEN, 17.).color(c(muted)))
            .on_press_tx(crate::import::choose_file);
        bar = bar.child(el().absolute(controls_width - 104., options_y).size(28., 28.).center().child(import));
        bar = bar.child(el().absolute(controls_width - 72., options_y).size(28., 28.).center().child(create));
    }
    if let Some(note) = s.note() {
        let options = match s.mode {
            WorkspaceMode::Document => crate::doc_mode::document_options(s, note),
            WorkspaceMode::Canvas => crate::canvas::canvas_options(s, width),
        };
        bar = bar.child(el().absolute(controls_width - 40., options_y).size(28., 28.).center().child(options));
    }
    if s.document_save_failed || s.pending_document_save.is_some() {
        let status = s.tr(if s.document_save_failed { "保存失败" } else { "正在保存…" });
        bar = bar.child(el().key("save-status").absolute(controls_width - 122., options_y)
            .size(12., 28.).center().label(status).help(status)
            .child(el().size(5., 5.).radius(2.5)
                .fill(c(if s.document_save_failed { if nav_dark(s) { 0xffbdb8 } else { 0xa94740 } } else { muted }))));
    }
    bar
}
/// Rune caches tokens by language and source. Colour-only spans preserve glyph advances.
pub(crate) fn code_spans(language: &str, source: &str, dark: bool) -> Vec<rune::Span> {
    let tokens = rune_kit::syntax::highlight(Some(language), source);
    use rune_kit::syntax::{Syntax, SyntaxTheme};
    let theme = if dark {
        SyntaxTheme::neutral(true)
            .with(Syntax::Function, c(0xb9d0ab))
            .with(Syntax::Link, c(0xb9d0ab))
    } else {
        SyntaxTheme::neutral(false)
            .with(Syntax::Function, c(0x526946))
            .with(Syntax::Link, c(0x526946))
            .with(Syntax::Variable, c(0x3d444d))
            .with(Syntax::Comment, c(0x626b66))
    };
    theme.spans(&tokens, 0)
}
pub(crate) fn navigation_selection(t: Theme) -> Color {
    c(theme_ink(t)).alpha(if t.is_dark() {0.10} else {0.065})
}
pub(crate) fn navigation_hover(_: Theme) -> Color {
    c(0x202124).alpha(0.04)
}
pub(crate) fn selection_plate(theme: Theme, key: &str, y: f32) -> El {
    el().key(key)
        .absolute(0., 0.)
        .w_full()
        .h(44.)
        .radius(10.)
        .fill(navigation_selection(theme))
        .offset(0., y)
        .motion(rune::Spring::NONE)
        .offset_motion(rune::Spring::NONE)
        .instant_changes()
        .transition(rune::Transition::none())
        .inert()
}
#[allow(dead_code)]
pub fn button(key: &str, icon: &'static Icon, size: f32) -> El {
    el().key(key)
        .size(40., 40.)
        .center()
        .radius(10.)
        .hover(c(0xffffff).alpha(0.52))
        .cursor(rune::Cursor::Pointer)
        .child(ico(icon, size))
}

/// Shader grain sits behind controls and stays fixed in device-pixel space.
pub fn material_grain(_s: &State, key: &str, w: f32, h: f32) -> El {
    if _s.sidebar_material.grain == 0 {
        return el().key(key).absolute(0., 0.).size(0., 0.).inert();
    }
    el().key(key)
        .absolute(0., 0.)
        .size(w, h)
        .effect(crate::material::GRAIN)
        .mark(crate::material::GRAIN)
        .inert()
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    s.formulas.begin_frame();
    let [w, h] = cx.layout_size("window").unwrap_or([1420., 1046.]);
    s.display_scale.set(cx.env().scale);
    s.system_dark.set(cx.system_dark());
    s.window_chrome.set(cx.chrome_insets());
    cx.caption_color(c(nav_ink(s)));
    cx.focus_ring(2., 1., c(accent(s)));
    let scale = 1.;
    s.ui_scale.set(scale);
    let body_w = w - WINDOW_INSET;
    let body_h = h - WINDOW_INSET;
    let content = match s.settings {
        Some(page) => crate::settings::view(s, page, body_w, body_h),
        None => workspace(s, cx, body_w, body_h),
    };
    let mut app = el().key("window-content").size(w, h)
        .child(material_grain(s, "window-material-grain", w, h))
        .child(el().key("workspace-frame").absolute(0., 0.).size(body_w, body_h).child(content))
        .child(window_chrome(s, w));
    if let Some(ghost) = crate::page_tree::render_nav_drag_ghost(s, w, h, theme_ink(s.theme()), theme_muted(s.theme())) {
        app = app.child(ghost);
    }
    if let Some(message) = &s.message {
        let width = (w - 40.).min(520.);
        app = app.child(crate::overlays::surface(s,
            row().key("message").role("alert").label(message)
                .pin(Some((w - width) / 2.), None, None, Some(24.))
                .w(width).min_h(52.).padding(14.).gap(12.).center_y()
                .child(label(message, 13., 450., theme_ink(s.theme())).grow(1.).min_w(0.).wrap())
                .child(el().key("message-close").size(24.,24.).radius(6.).center()
                    .role("button").label(s.tr("关闭")).focusable(true).cursor(rune::Cursor::Pointer)
                    .hover(c(theme_ink(s.theme())).alpha(0.06))
                    .child(ico(&rune_kit::glyphs::CLOSE, 14.).color(c(theme_muted(s.theme()))))
                    .on_press_tx(|s: &mut State, tx| { s.message = None; tx.animated(); })), 12.));
    }
    if let Some(overlay) = crate::doc_editor::canvas_overlays(s) { app = app.child(overlay); }
    if s.look.open && s.settings == Some(crate::model::SettingsPage::Appearance) {
        app = app.child(crate::look::popover(s, cx, w, h));
    }
    for overlay in crate::library_ui::layers(s, w, h) { app = app.child(overlay); }
    let mut window = el().key("window").w_full().h_full()
        .on_paste_content(crate::doc_editor::on_paste).child(app);
    if s.settings.is_none() {
        window = window.on_drop_files(crate::import::drop_markdown).accepts_files(crate::import::is_markdown);
    }
    crate::platform::backdrop(s, window)
}
fn workspace(s: &State, cx: &mut Cx<State>, w: f32, h: f32) -> El {
    if s.mode == WorkspaceMode::Document {
        return crate::doc_mode::view(s, cx, w, h);
    }
    let nav_width = crate::panes::visible_navigation_width(s, w);
    crate::panes::workspace_shell(s, crate::canvas::view(s, cx, w - nav_width, h - editor_top(s)), w, h, crate::panes::Boundary::Notes)
}

pub(crate) fn rename_input(s: &State) -> El {
    let field = text_input(&s.name_input, font(14.5, 500.).lh(20.))
        .key("rename-field")
        .label(s.tr("名称"))
        .single_line()
        .min_w(0.)
        .grow(1.)
        .h(20.)
        .color(c(theme_ink(s.theme())))
        .caret(
            c(crate::ui::accent(s)),
            c(crate::ui::accent_soft(s)),
        )
        .on_edit(|s: &mut State| &mut s.name_input)
        .on_input_key(|s: &mut State, e, tx| {
            if s.name_input.is_composing() || !matches!(e.key, rune::Key::Escape | rune::Key::Enter)
            {
                return false;
            }
            let target = s.renaming.clone();
            s.end_rename(e.key == rune::Key::Enter);
            if let Some(target) = target {
                tx.focus(target.row_key(), false);
            }
            true
        });
    row()
        .min_w(0.)
        .grow(1.)
        .h(30.)
        .px(9.)
        .gap(6.)
        .center_y()
        .radius(7.)
        .fill(c(editor_colors(s.theme()).raised).alpha(0.95))
        .border(1.5, c(crate::ui::accent(s)).alpha(0.72))
        .shadow(1., 6., c(crate::ui::accent(s)).alpha(0.10))
        .child(field)
        .child(
            el().px(4.5)
                .h(17.)
                .radius(4.)
                .fill(c(editor_colors(s.theme()).inset))
                .border(1., c(editor_colors(s.theme()).border))
                .center()
                .child(label("↵", 10.5, 600., theme_muted(s.theme()))),
        )
}
