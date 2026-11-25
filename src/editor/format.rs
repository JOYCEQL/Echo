//! One formatting surface for document text and canvas text. Only the editing target differs.
use crate::{
    doc_core::{DocAlign, DocBlockKind, DocMark},
    doc_editor::DocPopover,
    icons::*,
    model::State,
    ui::{c, font, label},
};
use rune::{Cursor, El, Icon, Key, TextEdit, Tx, col, el, row, text_input};

pub const WIDTH: f32 = 392.;
const COLORS: &[(&str, &str)] = &[
    ("", "默认"),
    ("gray", "灰色"),
    ("red", "红色"),
    ("orange", "橙色"),
    ("yellow", "黄色"),
    ("green", "绿色"),
    ("blue", "蓝色"),
    ("purple", "紫色"),
];
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Menu {
    Type,
    Color,
    Align,
    Link,
    More,
}
impl Menu {
    fn document(self) -> DocPopover {
        match self {
            Self::Type => DocPopover::HeadingMenu,
            Self::Color => DocPopover::ColorMenu,
            Self::Align => DocPopover::AlignMenu,
            Self::Link => DocPopover::LinkMenu,
            Self::More => DocPopover::MoreMenu,
        }
    }
    pub fn from_document(p: &DocPopover) -> Option<Self> {
        Some(match p {
            DocPopover::HeadingMenu => Self::Type,
            DocPopover::ColorMenu => Self::Color,
            DocPopover::AlignMenu => Self::Align,
            DocPopover::LinkMenu => Self::Link,
            DocPopover::MoreMenu => Self::More,
            _ => return None,
        })
    }
    pub fn size(self) -> [f32; 2] {
        match self {
            Self::Type => [220., 256.],
            Self::Color => [284., 162.],
            Self::Align => [180., 112.],
            Self::Link => [300., 104.],
            Self::More => [180., 84.],
        }
    }
}
fn sync(s: &mut State) {
    crate::doc_editor::sync_to_state(s)
}
pub fn close(s: &mut State, tx: &mut Tx) {
    {
        s.editor.active.popover = None;
        s.editor.active.link_input.focused = false;
        tx.blur();
    }
    tx.animated();
}
pub fn open(s: &mut State, menu: Menu, tx: &mut Tx) {
    {
        if menu == Menu::Link {
            let p = s.editor.active.core.ordered().0;
            let url = s
                .editor
                .active
                .core
                .text_of(p.block)
                .and_then(|t| t.marks_at(p.offset).link)
                .unwrap_or_default();
            s.editor.active.link_input = TextEdit::new(url);
        }
        s.editor.active.popover = Some(menu.document());
    }
    if menu == Menu::Link {
        tx.focus("doc-link-input", false);
    }
    tx.animated();
}
pub fn toggle(s: &mut State, mark: DocMark) {
    s.editor.active.core.toggle_mark(mark);
    sync(s);
}
fn active(s: &State, mark: DocMark) -> bool {
    s.editor.active.core.selection_has_mark(mark)
}
fn button(s: &State, key: &str, title: &str, icon: &'static Icon, on: bool) -> El {
    el().key(key)
        .size(26., 28.)
        .center()
        .radius(6.)
        .cursor(Cursor::Pointer)
        .role("button")
        .label(title)
        .help(title)
        .focusable(true)
        .fill(if on {
            c(crate::ui::accent_soft(s))
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(c(crate::ui::editor_colors(s.theme()).inset))
        .press(c(crate::ui::accent_soft(s)))
        .child(ico(icon, 16.).color(c(if on {
            crate::ui::accent(s)
        } else {
            crate::ui::theme_ink(s.theme())
        })))
}
pub fn toolbar(s: &State, position: [f32; 2]) -> El {
    let kind = s
        .editor
        .active
        .core
        .block(s.editor.active.core.sel.head.block)
        .map(|b| b.kind.clone());
    let name = match kind {
        Some(DocBlockKind::Heading { level: 1 }) => "H1",
        Some(DocBlockKind::Heading { level: 2 }) => "H2",
        Some(DocBlockKind::Heading { level: 3 }) => "H3",
        _ => s.tr("正文"),
    };

    let mut bar = crate::overlays::surface(
        s,
        row()
            .key("doc-sel-toolbar")
            .role("toolbar")
            .label(s.tr("文本格式"))
            .absolute(position[0], position[1])
            .size(WIDTH, 40.)
            .px(6.)
            .gap(6.)
            .center_y()
            .keeps_text_selection(),
        10.,
    )
    .child(
        row()
            .key("tb-block-type")
            .size(60., 28.)
            .gap(4.)
            .center()
            .radius(6.)
            .cursor(Cursor::Pointer)
            .role("button")
            .label(s.tr("文本类型"))
            .focusable(true)
            .hover(c(crate::ui::editor_colors(s.theme()).inset))
            .child(label(name, 13., 500., crate::ui::theme_ink(s.theme())))
            .child(ico(&CHEVRON_DOWN, 11.).color(c(crate::ui::theme_muted(s.theme()))))
            .on_press_tx(move |s: &mut State, tx| open(s, Menu::Type, tx)),
    )
    .child(
        el().size(1., 16.)
            .fill(c(crate::ui::editor_colors(s.theme()).border)),
    );
    for (key, mark, icon, title) in [
        ("tb-bold", DocMark::Bold, &BOLD, "加粗 ⌘B"),
        ("tb-italic", DocMark::Italic, &ITALIC, "斜体 ⌘I"),
        ("tb-under", DocMark::Underline, &UNDERLINE, "下划线 ⌘U"),
        ("tb-strike", DocMark::Strike, &STRIKETHROUGH, "删除线 ⇧⌘X"),
        ("tb-code", DocMark::Code, &CODE, "行内代码 ⌘E"),
    ] {
        bar = bar.child(
            button(s, key, s.tr(title), icon, active(s, mark)).on_press_tx(
                move |s: &mut State, tx| {
                    toggle(s, mark);
                    tx.animated();
                },
            ),
        );
    }
    bar = bar.child(
        el().size(1., 16.)
            .fill(c(crate::ui::editor_colors(s.theme()).border)),
    );
    for (key, title, icon, menu) in [
        ("tb-color", "文字与高亮颜色", &TYPE, Menu::Color),
        ("tb-align", "段落对齐", &ALIGN_LEFT, Menu::Align),
        ("tb-link", "链接 ⌘K", &LINK, Menu::Link),
        ("tb-more", "更多操作", &MORE_HORIZONTAL, Menu::More),
    ] {
        let control = if menu == Menu::Color {
            row()
                .key(key)
                .size(26., 28.)
                .center()
                .radius(6.)
                .cursor(Cursor::Pointer)
                .role("button")
                .label(s.tr(title))
                .help(s.tr(title))
                .focusable(true)
                .hover(c(crate::ui::editor_colors(s.theme()).inset))
                .child(
                    col()
                        .gap(0.)
                        .center()
                        .offset(0., 1.)
                        .child(label("A", 13., 600., crate::ui::theme_ink(s.theme())))
                        .child(
                            el().size(13., 2.)
                                .offset(0., -2.)
                                .radius(1.)
                                .fill(c(crate::ui::accent(s))),
                        ),
                )
        } else {
            button(s, key, s.tr(title), icon, false)
        };
        bar = bar.child(control.on_press_tx(move |s: &mut State, tx| open(s, menu, tx)));
    }
    bar
}
fn item(s: &State, key: &str, title: &str, icon: &'static Icon) -> El {
    row()
        .key(key)
        .w_full()
        .h(32.)
        .px(8.)
        .gap(10.)
        .radius(6.)
        .center_y()
        .cursor(Cursor::Pointer)
        .role("button")
        .label(title)
        .focusable(true)
        .hover(c(crate::ui::editor_colors(s.theme()).inset))
        .child(ico(icon, 16.).color(c(crate::ui::theme_muted(s.theme()))))
        .child(label(title, 13., 450., crate::ui::theme_ink(s.theme())))
}
pub fn apply_link(s: &mut State, tx: &mut Tx) {
    let url = s.editor.active.link_input.text.trim().to_owned();
    if !url.is_empty()
        && !(url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("mailto:"))
    {
        s.message = Some(s.tr("请输入 https://、http:// 或 mailto: 链接").into());
        return;
    }
    s.editor
        .active
        .core
        .update_selected_marks(|m| m.link = (!url.is_empty()).then(|| url.clone()));
    sync(s);
    close(s, tx);
}
pub fn color(s: &mut State, name: &str, background: bool) {
    s.editor.active.core.update_selected_marks(|m| {
        let value = (!name.is_empty()).then(|| name.to_owned());
        if background {
            m.bg = value;
        } else {
            m.color = value;
        }
    });
    sync(s);
}
fn color_active(s: &State, name: &str, background: bool) -> bool {
    {
        let core = &s.editor.active.core;
        let (a, b) = core.ordered();
        a.block == b.block
            && core.text_of(a.block).is_some_and(|text| {
                text.all_have(a.offset..b.offset, |m| {
                    if background {
                        m.bg.as_deref().unwrap_or("") == name
                    } else {
                        m.color.as_deref().unwrap_or("") == name
                    }
                })
            })
    }
}
pub fn popover(s: &State, menu: Menu, position: [f32; 2]) -> El {
    let key = match menu {
        Menu::Type => "doc-heading-popover",
        Menu::Color => "doc-color-popover",
        Menu::Align => "doc-align-menu",
        Menu::Link => "doc-link-menu",
        Menu::More => "doc-more-menu",
    };

    let mut panel = crate::overlays::surface(
        s,
        col()
            .key(key)
            .absolute(position[0], position[1])
            .w(menu.size()[0])
            .padding(8.)
            .gap(2.)
            .role("menu")
            .on_key(move |s: &mut State, e, tx| {
                if e.key == Key::Escape {
                    close(s, tx);
                    true
                } else {
                    false
                }
            }),
        12.,
    );
    if menu != Menu::Link {
        panel = panel.keeps_text_selection();
    }
    match menu {
        Menu::Type => {
            for (title, icon, kind) in [
                ("正文", &TYPE, DocBlockKind::Text),
                ("一级标题", &HEADING_1, DocBlockKind::Heading { level: 1 }),
                ("二级标题", &HEADING_2, DocBlockKind::Heading { level: 2 }),
                ("三级标题", &HEADING_3, DocBlockKind::Heading { level: 3 }),
                ("无序列表", &LIST, DocBlockKind::Bullet),
                (
                    "有序列表",
                    &LIST_ORDERED,
                    DocBlockKind::Ordered { start: None },
                ),
                ("待办", &CHECK_SQUARE, DocBlockKind::Todo { done: false }),
            ] {
                panel = panel.child(
                    item(
                        s,
                        &format!("heading-option-{}", s.tr(title)),
                        s.tr(title),
                        icon,
                    )
                    .on_press_tx(move |s: &mut State, tx| {
                        {
                            let ids = s.editor.active.core.selected_blocks_ids();
                            s.editor.active.core.turn_into(&ids, kind.clone());
                        }
                        sync(s);
                        close(s, tx);
                    }),
                );
            }
        }
        Menu::Color => {
            panel = panel.padding(12.).gap(9.);
            for background in [false, true] {
                panel = panel.child(label(
                    s.tr(if background {
                        "背景高亮"
                    } else {
                        "文字颜色"
                    }),
                    11.,
                    500.,
                    crate::ui::theme_muted(s.theme()),
                ));
                let mut swatches = row().gap(4.);
                for &(name, title) in COLORS {
                    let hex = if name.is_empty() {
                        if background {
                            crate::ui::editor_colors(s.theme()).page
                        } else {
                            crate::ui::theme_ink(s.theme())
                        }
                    } else {
                        if background {
                            crate::editor_palette::bg_color_hex(name, s.theme())
                        } else {
                            crate::editor_palette::text_color_hex(name, s.theme())
                        }
                        .expect("shared palette")
                    };
                    let on = color_active(s, name, background);

                    let mut swatch = el()
                        .key(format!(
                            "{}-color-{name}",
                            if background { "highlight" } else { "text" }
                        ))
                        .size(28., 28.)
                        .center()
                        .radius(6.)
                        .role("button")
                        .label(format!(
                            "{} · {}",
                            s.tr(if background {
                                "背景高亮"
                            } else {
                                "文字颜色"
                            }),
                            s.tr(title)
                        ))
                        .help(s.tr(title))
                        .focusable(true)
                        .cursor(Cursor::Pointer)
                        .fill(if background {
                            c(hex)
                        } else {
                            c(crate::ui::editor_colors(s.theme()).raised)
                        })
                        .border(
                            if on { 1.5 } else { 1. },
                            c(if on {
                                crate::ui::accent(s)
                            } else {
                                crate::ui::editor_colors(s.theme()).border
                            }),
                        )
                        .hover(c(crate::ui::accent_soft(s)))
                        .on_press_tx(move |s: &mut State, tx| {
                            color(s, name, background);
                            close(s, tx);
                        });
                    if !background {
                        swatch = swatch.child(label("A", 14., 600., hex));
                    } else if on {
                        swatch = swatch
                            .child(ico(&CHECK, 13.).color(c(crate::ui::theme_ink(s.theme()))));
                    } else if name.is_empty() {
                        swatch =
                            swatch.child(label("∅", 13., 400., crate::ui::theme_muted(s.theme())));
                    }
                    swatches = swatches.child(swatch);
                }
                panel = panel.child(swatches);
            }
        }
        Menu::Align => {
            for (title, icon, align) in [
                ("左对齐", &ALIGN_LEFT, DocAlign::Left),
                ("居中对齐", &ALIGN_CENTER, DocAlign::Center),
                ("右对齐", &ALIGN_RIGHT, DocAlign::Right),
            ] {
                panel = panel.child(item(s, s.tr(title), s.tr(title), icon).on_press_tx(
                    move |s: &mut State, tx| {
                        {
                            let ids = s.editor.active.core.selected_blocks_ids();
                            s.editor.active.core.set_align(&ids, align);
                            if ids.len() == 1
                                && s.editor.active.core.block(ids[0]).is_some_and(|b| {
                                    matches!(
                                        &b.kind,
                                        DocBlockKind::Code { lang, .. }
                                            if crate::rich_render::language(lang).is_some()
                                    )
                                })
                            {
                                s.editor.active.focused = false;
                            }
                        }
                        sync(s);
                        close(s, tx);
                    },
                ));
            }
        }
        Menu::More => {
            for (key, title, icon, clear) in [
                ("selection-copy", "复制", &COPY, false),
                ("selection-clear", "清除格式", &TYPE, true),
            ] {
                panel = panel.child(item(s, key, s.tr(title), icon).on_press_tx(
                    move |s: &mut State, tx| {
                        {
                            if clear {
                                s.editor
                                    .active
                                    .core
                                    .update_selected_marks(|m| *m = Default::default());
                            } else {
                                tx.copy_rich(s.editor.active.core.clipboard_content());
                            }
                        }
                        if clear {
                            sync(s);
                        }
                        close(s, tx);
                    },
                ));
            }
        }
        Menu::Link => {
            let edit = &s.editor.active.link_input;

            panel = panel.padding(10.).gap(8.).child(
                row()
                    .w_full()
                    .h(36.)
                    .px(10.)
                    .center_y()
                    .radius(7.)
                    .fill(c(crate::ui::editor_colors(s.theme()).inset))
                    .border(
                        1.,
                        c(if edit.focused {
                            crate::ui::accent(s)
                        } else {
                            crate::ui::editor_colors(s.theme()).border
                        }),
                    )
                    .child(
                        text_input(edit, font(13., 400.).lh(20.))
                            .key("doc-link-input")
                            .label(s.tr("链接地址"))
                            .placeholder(
                                "https://example.com",
                                c(crate::ui::theme_muted(s.theme())),
                            )
                            .color(c(crate::ui::theme_ink(s.theme())))
                            .caret(c(crate::ui::accent(s)), c(crate::ui::accent_soft(s)))
                            .single_line()
                            .w_full()
                            .h(20.)
                            .on_edit(move |s: &mut State| &mut s.editor.active.link_input)
                            .on_input_key(move |s: &mut State, e, tx| {
                                if e.key == Key::Enter {
                                    apply_link(s, tx);
                                    true
                                } else if e.key == Key::Escape {
                                    close(s, tx);
                                    true
                                } else {
                                    false
                                }
                            }),
                    ),
            );

            panel = panel.child(
                row()
                    .w_full()
                    .gap(8.)
                    .child(
                        item(s, "link-remove", s.tr("移除链接"), &MINUS)
                            .w(138.)
                            .on_press_tx(move |s: &mut State, tx| {
                                {
                                    s.editor.active.link_input = TextEdit::new("")
                                };
                                apply_link(s, tx);
                            }),
                    )
                    .child(
                        item(s, "link-apply", s.tr("应用"), &CHECK)
                            .w(110.)
                            .on_press_tx(move |s: &mut State, tx| apply_link(s, tx)),
                    ),
            );
        }
    }
    panel
}
