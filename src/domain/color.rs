//! Canonical document colours; persisted values do not depend on an application theme.
fn hex(name: &str) -> Option<u32> {
    let value = name.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}
pub fn text_color_hex(name: &str) -> Option<u32> {
    if name.starts_with('#') {
        return hex(name);
    }
    Some(match name {
        "gray" => 0x8f959e,
        "red" => 0xe22e28,
        "orange" => 0xed6d0c,
        "yellow" => 0xd99b00,
        "green" => 0x2ea121,
        "blue" => 0x336df4,
        "purple" => 0x7a4ff3,
        _ => return None,
    })
}
pub fn bg_color_hex(name: &str) -> Option<u32> {
    if name.starts_with('#') {
        return hex(name);
    }
    Some(match name {
        "gray" => 0xebedf0,
        "red" => 0xfee3e2,
        "orange" => 0xfee7cd,
        "yellow" => 0xfff5c2,
        "green" => 0xe1f7df,
        "blue" => 0xe0e9ff,
        "purple" => 0xede6ff,
        _ => return None,
    })
}
