//! Presentation palettes; serialization uses the canonical light RGB values.
pub const DARK_TEXT: &[(u32, u32)] = &[
    (0x8f959e, 0xa4a9b3),
    (0xe22e28, 0xff8d86),
    (0xed6d0c, 0xffb16e),
    (0xd99b00, 0xe4c767),
    (0x2ea121, 0x8bce82),
    (0x336df4, 0x93b7ff),
    (0x7a4ff3, 0xbca1ff),
];
pub const DARK_HIGHLIGHT: &[(u32, u32)] = &[
    (0xebedf0, 0x393c43),
    (0xfee3e2, 0x513334),
    (0xfee7cd, 0x503c2a),
    (0xfff5c2, 0x49422a),
    (0xe1f7df, 0x304632),
    (0xe0e9ff, 0x303f5a),
    (0xede6ff, 0x403552),
];
pub fn text_color_hex(name: &str, theme: crate::model::Theme) -> Option<u32> {
    let rgb = crate::document_color::text_color_hex(name)?;
    Some(if theme.is_dark() {
        DARK_TEXT
            .iter()
            .find(|(light, _)| *light == rgb)
            .map_or(rgb, |(_, dark)| *dark)
    } else {
        rgb
    })
}
pub fn bg_color_hex(name: &str, theme: crate::model::Theme) -> Option<u32> {
    let rgb = crate::document_color::bg_color_hex(name)?;
    Some(if theme.is_dark() {
        DARK_HIGHLIGHT
            .iter()
            .find(|(light, _)| *light == rgb)
            .map_or(rgb, |(_, dark)| *dark)
    } else {
        rgb
    })
}

pub fn callout_palette_colors(name: &str, theme: crate::model::Theme) -> (u32, u32) {
    match name {
        "blue" => (
            theme.choose(0xf0f4ff, 0x252f43),
            theme.choose(0xc2d4ff, 0x414f70),
        ),
        "green" => (
            theme.choose(0xeffaf0, 0x26382c),
            theme.choose(0xb7ebc0, 0x405f49),
        ),
        "purple" => (
            theme.choose(0xf5f2ff, 0x332c43),
            theme.choose(0xd9ccff, 0x55466c),
        ),
        "red" => (
            theme.choose(0xfef0f0, 0x3d292c),
            theme.choose(0xfdc6c4, 0x684348),
        ),
        "gray" => (
            theme.choose(0xf5f6f7, 0x2b2e34),
            theme.choose(0xdee0e3, 0x464a53),
        ),
        _ => (
            theme.choose(0xfff7eb, 0x3c3225),
            theme.choose(0xfed4a4, 0x665039),
        ), // amber/orange default
    }
}
