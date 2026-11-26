use crate::{
    doc_core::NoteFormat,
    icons::{ARROW_LEFT, COMMAND, FOLDER, FOLDER_OPEN, PALETTE, TRASH_2, ROTATE_CCW, ico},
    model::{SettingsPage, State},
    ui::{accent, accent_soft, c, editor_colors, label, theme_ink, theme_muted},
};
use rune::{Cursor, El, ScrollSpec, col, el, row};

impl SettingsPage {
    fn title(self) -> &'static str {
        match self { Self::Appearance => "外观", Self::Shortcuts => "快捷键", Self::Data => "本地数据", Self::Trash => "回收站", Self::Updates => "软件更新" }
    }
    fn key(self) -> &'static str {
        match self { Self::Appearance => "appearance", Self::Shortcuts => "shortcuts", Self::Data => "data", Self::Trash => "trash", Self::Updates => "updates" }
    }

}

fn rule(s: &State) -> El { el().w_full().h(1.).fill(c(editor_colors(s.theme()).border).alpha(0.65)) }
fn action(s: &State, key: &str, title: &str, glyph: &'static rune::Icon, danger: bool) -> El {
    let ink = if danger { s.theme().choose(0xa94740, 0xffa49b) } else { theme_ink(s.theme()) };
    row().key(key).h(34.).px(12.).gap(7.).center()
        .radius(7.).border(1., c(editor_colors(s.theme()).border))
        .fill(c(editor_colors(s.theme()).raised))
        .hover(if danger { c(s.theme().choose(0xf8e7e4, 0x402b2b)) } else { c(accent_soft(s)) })
        .instant_changes().focusable(true).cursor(Cursor::Pointer).role("button").label(title)
        .child(ico(glyph, 14.).color(c(ink)))
        .child(label(title, 13., 500., ink))
}
fn rail(s: &State, page: SettingsPage, width: f32, height: f32) -> El {
    let ink = crate::ui::nav_ink(s);
    let muted = crate::ui::nav_muted(s);
    let mut nav = col().w_full().px(12.).gap(4.);
    for section in [SettingsPage::Appearance, SettingsPage::Shortcuts, SettingsPage::Data, SettingsPage::Trash, SettingsPage::Updates] {
        let active = section == page;
        let glyph = match section { SettingsPage::Appearance => &PALETTE, SettingsPage::Shortcuts => &COMMAND, SettingsPage::Data => &FOLDER, SettingsPage::Trash => &TRASH_2, SettingsPage::Updates => &ROTATE_CCW };
        let mut item = row().key(format!("settings-tab-{}", section.key()))
            .w_full().h(40.).px(12.).gap(10.).radius(9.).center_y().instant_changes()
            .fill(if active { crate::ui::nav_selection(s) } else { rune::Color::TRANSPARENT })
            .border(
                if active && crate::ui::nav_selection_border(s).is_some() { 0.75 } else { 0. },
                crate::ui::nav_selection_border(s).unwrap_or(rune::Color::TRANSPARENT),
            )
            .hover(crate::ui::nav_hover(s)).cursor(Cursor::Pointer).focusable(true).role("button")
            .label(s.tr(section.title()))
            .child(ico(glyph, 20.).color(c(if active && s.sidebar_material.is_minimal_white() { ink } else { muted })))
            .child(label(s.tr(section.title()), 14., if active { 550. } else { 450. }, ink))
            .on_press_tx(move |s: &mut State, tx| { s.settings = Some(section); tx.instant(); });
        if section == SettingsPage::Trash {
            let count = s.notes.iter().filter(|n| n.notebook == "回收站").count();
            if count > 0 { item = item.child(el().grow(1.)).child(label(count.to_string(), 12., 500., muted)); }
        }
        nav = nav.child(item);
    }
    col().key("settings-rail").size(width, height)
        .child(row().key("settings-drag").w_full().h(crate::ui::CHROME_HEIGHT).pl(crate::platform::CHROME_LEADING).pr(14.).gap(10.).center_y().window_drag()
            .child(label(s.tr("设置"), 14., 600., ink)))
        .child(nav.mt(6.)).child(el().grow(1.))
        .child(el().w_full().px(12.)
            .child(row().key("settings-back").w_full().h(44.).px(12.).gap(10.).radius(9.).center_y()
                .fill(crate::ui::nav_hover(s)).hover(crate::ui::nav_selection(s)).instant_changes()
                .cursor(Cursor::Pointer).role("button").label(s.tr("返回笔记")).help(s.tr("返回笔记 · Esc")).focusable(true)
                .focus_ring(2.,2.,c(muted))
                .child(ico(&ARROW_LEFT,20.).color(c(muted)))
                .child(label(s.tr("返回笔记"),14.,450.,ink))
                .child(el().grow(1.))
                .child(label("Esc",11.,450.,muted))
                .on_press_tx(State::close_settings)))
}
fn preference_menu(s: &State,key:&str,title:&str,value:&str)->El {
    row().key(key).h(32.).min_w(154.).px(10.).gap(12.).center_y().radius(7.)
        .fill(c(editor_colors(s.theme()).raised)).border(0.75,c(editor_colors(s.theme()).border))
        .hover(c(editor_colors(s.theme()).inset)).focusable(true).role("button").label(title)
        .cursor(Cursor::Pointer).focus_ring(2.,2.,c(accent(s)))
        .child(label(value,13.,450.,theme_ink(s.theme())).grow(1.))
        .child(ico(&crate::icons::CHEVRON_RIGHT,12.).rotate(std::f32::consts::FRAC_PI_2).color(c(theme_muted(s.theme()))))
}
fn appearance(s: &State,width:f32)->El {
    let language=preference_menu(s,"language-select",s.tr("语言"),if s.language.is_english(){"English"}else{"简体中文"})
        .on_press_tx(|s:&mut State,tx|{
            let menu=rune::PopupMenu::new()
                .item(rune::PopupItem::new("zh-CN","简体中文").checked(!s.language.is_english()))
                .item(rune::PopupItem::new("en","English").checked(s.language.is_english()));
            tx.popup_menu(menu,"language-select",|s:&mut State,id,tx|{match id {
                Some("zh-CN")=>s.set_language(crate::i18n::Language::Chinese),Some("en")=>s.set_language(crate::i18n::Language::English),_=>return,
            }tx.instant();});
        });
    let icons=preference_menu(s,"notebook-style-select",s.tr("空间图标"),s.tr(s.notebook_icon_style.name()))
        .on_press_tx(|s:&mut State,tx|{
            use crate::notebook_icons::Style;
            let mut menu = rune::PopupMenu::new();
            for style in [Style::Solid, Style::Pals, Style::Color, Style::Ink, Style::Journal, Style::Monochrome] {
                menu = menu.item(rune::PopupItem::new(style.key(), s.tr(style.name()))
                    .checked(s.notebook_icon_style == style));
            }
            tx.popup_menu(menu, "notebook-style-select", |s: &mut State, id, tx| {
                if id.is_some() {
                    s.set_notebook_icon_style(Style::from_preference(id));
                    tx.instant();
                }
            });
        });
    let line=|title:&str,control:El| {
        let layout=if width<320.{col().w_full().gap(12.)}else{row().w_full().gap(16.).center_y()};
        layout.child(label(title,14.,450.,theme_ink(s.theme())).grow(1.)).child(control)
    };
    col().w_full().gap(28.).child(crate::look::studio(s,width))
        .child(line(s.tr("语言"),language))
        .child(line(s.tr("空间图标"),row().gap(8.).center_y()
            .child(crate::notebook_icons::render(s, "产品", 20., true, theme_muted(s.theme()))).child(icons)))
}
fn shortcuts(s: &State) -> El {
    let shortcut = |value: &'static str| -> &'static str {
        #[cfg(target_os = "windows")]
        { return match value {
            "⌘ N" => "Ctrl+N", "⌘ K" => "Ctrl+K", "⌘ \\" => "Ctrl+\\", "⌘ ," => "Ctrl+,",
            "⌘ B / ⌘ I" => "Ctrl+B / Ctrl+I", "⌘ Return" => "Ctrl+Enter",
            "⌘ Z / ⇧ ⌘ Z" => "Ctrl+Z / Ctrl+Shift+Z", "⌘ D" => "Ctrl+D", "⌘ 0" => "Ctrl+0",
            _ => unreachable!("unregistered shortcut"),
        }; }
        #[cfg(target_os = "macos")]
        { value }
    };
    let groups: &[(&str, &[(&str, &str)])] = &[
        (
            s.tr("笔记与导航"),
            &[
                (s.tr("新建笔记"), shortcut("⌘ N")),
                (s.tr("搜索笔记"), shortcut("⌘ K")),
                (s.tr("收起 / 展开侧边栏"), shortcut("⌘ \\")),
                (s.tr("打开设置"), shortcut("⌘ ,")),
                (s.tr("返回笔记"), "Esc"),
            ],
        ),
        (
            s.tr("文字编辑"),
            &[
                (s.tr("选择文字"), s.tr("在正文中拖动")),
                (s.tr("二级标题"), s.tr("## 再按空格")),
                (s.tr("插入块或格式"), "/"),
                (s.tr("粗体 / 斜体"), shortcut("⌘ B / ⌘ I")),
                (s.tr("勾选当前待办"), shortcut("⌘ Return")),
                (s.tr("撤销 / 重做"), shortcut("⌘ Z / ⇧ ⌘ Z")),
                (s.tr("结束编辑"), "Esc"),
            ],
        ),
        (
            s.tr("自由画布"),
            &[
                (s.tr("新建文字块"), s.tr("单击画布空白处")),
                (s.tr("移动块 / 调整尺寸"), s.tr("拖动边框 / 外侧控件")),
                (s.tr("复制选中块"), shortcut("⌘ D")),
                (s.tr("删除选中块"), "⌫"),
                (s.tr("重置画布"), shortcut("⌘ 0")),
            ],
        ),
    ];

    let mut content = col().key("shortcut-groups").w_full().gap(32.);
    for (title, entries) in groups {
        let mut group = col().w_full().gap(0.).child(label(*title, 12.5, 500., theme_muted(s.theme())).mb(12.));
        for (action, keys) in *entries {
            group = group.child(row().w_full().min_h(46.).py(10.).gap(16.).center_y()
                .child(label(*action, 14., 450., theme_ink(s.theme())).grow(1.).min_w(0.).wrap())
                .child(el().px(9.).h(25.).radius(5.).fill(c(editor_colors(s.theme()).raised))
                    .border(1., c(editor_colors(s.theme()).border)).center()
                    .child(label(*keys, 12.5, 500., theme_muted(s.theme())))));
        }
        content = content.child(group);
    }
    content
}
fn data(s: &State) -> El {
    col().w_full().gap(12.)
        .child(label(s.tr("笔记存储位置"), 12.5, 500., theme_muted(s.theme())))
        .child(label(s.tr("正文保存为 Markdown，画布布局与设置单独保存为 JSON。"), 13., 400., theme_muted(s.theme())).w_full().wrap())
        .child(col().w_full().py(12.).gap(18.)
            .child(row().w_full().gap(12.).center_y().child(ico(&FOLDER_OPEN, 22.).color(c(accent(s))))
                .child(label(s.store.root.display().to_string(), 13.5, 450., theme_muted(s.theme())).grow(1.).min_w(0.).wrap()))
            .child(row().child(action(s, "open-data-folder", s.tr("在 Finder 中打开"), &FOLDER_OPEN, false)
                .on_press(|s: &mut State| {
                    if let Err(e) = crate::platform::open_folder(&s.store.root) {
                        s.message = Some(format!("{}: {e}", s.tr("无法打开文件夹")));
                    }
                }))))
}
fn trash(s: &State, width: f32) -> El {
    let notes: Vec<_> = s.notes.iter().filter(|n| n.notebook == "回收站").collect();
    if notes.is_empty() {
        return col().key("trash-empty").w_full().py(64.).gap(14.).items_center()
            .child(el().size(52., 52.).radius(14.).center().fill(c(editor_colors(s.theme()).inset))
                .child(ico(&TRASH_2, 23.).color(c(theme_muted(s.theme())))))
            .child(label(s.tr("回收站是空的"), 17., 550., theme_ink(s.theme())))
            .child(label(s.tr("删除的笔记会出现在这里。"), 13., 400., theme_muted(s.theme())));
    }
    let mut body = col().w_full().gap(16.)
        .child(row().w_full().center_y()
            .child(label(if s.language.is_english() { format!("{} notes", notes.len()) } else { format!("{} 篇笔记", notes.len()) }, 14., 550., theme_ink(s.theme())))
            .child(el().grow(1.))
            .child(action(s, "empty-trash", s.tr("清空回收站"), &TRASH_2, true)
                .on_press_tx(|s: &mut State, tx| { s.empty_all_trash(); tx.instant(); })))
        .child(label(s.tr("还原到原空间，可按 ⌘Z 撤销。"), 12.5, 400., theme_muted(s.theme())).w_full().wrap());
    for note in notes {
        let restore_id = note.id.clone();
        let delete_id = note.id.clone();
        let format_name = if note.format == NoteFormat::Document { s.tr("结构化文档") } else { s.tr("自由画布") };
        let details = col().grow(1.).min_w(0.).gap(5.)
            .child(label(&note.title, 14., 500., theme_ink(s.theme())).w_full().truncate())
            .child(label(format!("{} · {}", format_name, s.book_name(&note.original_book)), 12., 400., theme_muted(s.theme())).w_full().truncate());
        let actions = row().gap(8.)
            .child(action(s, &format!("restore-note-{}", note.id), s.tr("还原"), &ROTATE_CCW, false)
                .on_press_tx(move |s: &mut State, tx| {
                    s.restore_note_by_id(&restore_id); tx.instant();
                }))
            .child(action(s, &format!("delete-forever-{}", note.id), s.tr("删除"), &TRASH_2, true)
                .help(s.tr("从回收站删除这篇笔记"))
                .on_press_tx(move |s: &mut State, tx| { s.delete_note_permanently(&delete_id); tx.instant(); }));
        let mut entry = if width < 520. { col().w_full().gap(14.).py(16.) } else { row().w_full().gap(20.).py(16.).center_y() };
        entry = entry.key(format!("trash-note-{}", note.id)).child(details).child(actions);
        body = body.child(rule(s)).child(entry);
    }
    body
}
fn updates(s: &State) -> El {
    use crate::update::Status;
    let (status, button) = match &s.updater.status {
        Status::Unavailable => (s.tr("此版本暂不支持应用内更新").to_owned(), None),
        Status::Idle => (s.tr("检查是否有新版本可用").to_owned(), Some("检查更新")),
        Status::Checking => (s.tr("正在检查更新…").to_owned(), None),
        Status::Current => (s.tr("已是最新版本").to_owned(), Some("检查更新")),
        Status::Available(m) => (format!("{} {}", s.tr("发现新版本"), m.version), Some("下载更新")),
        Status::Downloading(percent) => (format!("{} · {percent}%", s.tr("正在下载更新")), None),
        Status::Verifying => (format!("100% · {}", s.tr("正在校验更新…")), None),
        Status::Ready(p) => (format!("{} {}", p.manifest.version, s.tr("已准备就绪")), Some("重启以更新")),
        Status::Preparing | Status::Installing => (s.tr("正在准备重启…").to_owned(), None),
        Status::CheckFailed(_) | Status::Failed(_) => (s.tr("更新未完成").to_owned(), Some("重新检查")),
    };
    let mut content = col().w_full().gap(18.)
        .child(label(format!("Echo {}", env!("CARGO_PKG_VERSION")), 18., 550., theme_ink(s.theme())))
        .child(label(status, 14., 450., theme_muted(s.theme())).w_full().wrap());
    if let Some(note) = crate::update::release_note(s) {
        content = content.child(label(note, 13., 450., theme_muted(s.theme())).w_full().wrap());
    }
    if matches!(s.updater.status, Status::Downloading(_)) {
        content = content.child(crate::update::update_button(s));
    }
    if let Status::CheckFailed(error) | Status::Failed(error) = &s.updater.status {
        content = content.child(label(error, 12., 400., theme_muted(s.theme())).w_full().wrap());
    }
    if let Some(button) = button {
        content = content.child(row().child(action(s, "update-action", s.tr(button), &ROTATE_CCW, false).on_press_tx(crate::update::act)));
    }
    if !matches!(s.updater.status, Status::Unavailable) {
        content = content.child(rule(s)).child(row().w_full().gap(16.).center_y()
            .child(col().grow(1.).min_w(0.).gap(6.)
                .child(label(s.tr("自动检查更新"), 14., 450., theme_ink(s.theme())))
                .child(label(s.tr("控制常规更新检查。紧急修复会自动下载，重启时会先保存笔记。"), 12.5, 400., theme_muted(s.theme())).w_full().wrap()))
            .child(action(s, "update-automatic", s.tr(if s.updater.automatic { "已开启" } else { "已关闭" }), &ROTATE_CCW, false)
                .role("switch").checked(s.updater.automatic).on_press_tx(crate::update::toggle)));
    }
    content
}
pub fn view(s: &State, page: SettingsPage, w: f32, h: f32) -> El {
    let rail_w = crate::panes::navigation_width(s, w);
    let gutter = if w < 900. { 28. } else { 48. };
    let width = (w - rail_w - gutter * 2.).min(760.).max(120.);
    let margin = (w - rail_w - width) / 2.;
    let body = match page {
        SettingsPage::Appearance => appearance(s, width),
        SettingsPage::Shortcuts => shortcuts(s),
        SettingsPage::Data => data(s),
        SettingsPage::Trash => trash(s, width),
        SettingsPage::Updates => updates(s),
    };
    let top = crate::ui::window_content_top(s);
    row().key("settings-page").size(w, h).absolute(0., 0.)
        .child(rail(s, page, rail_w, h))
        .child({
            let mut body_panel = col().key("settings-body").size(w - rail_w, h - top).mt(top).fill(c(crate::ui::page_surface(s))).clip_rounded(10.);
            if let Some(edge) = crate::ui::page_surface_border(s) {
                body_panel = body_panel.border(0.75, edge);
            }
            body_panel
                .child(el().w_full().h(26.).window_drag())
                .child(col().key(format!("settings-{}-scroll", page.key())).w_full().grow(1.).min_h(0.)
                    .scroll(ScrollSpec { stick_bottom: false })
                    .child(col().key("settings-content").w(width).ml(margin).pt(8.).pb(48.).gap(30.)
                        .child(label(s.tr(page.title()), 26., 600., theme_ink(s.theme())))
                        .child(body)))
        })
        .child(el().key("settings-toolbar").absolute(rail_w, 0.).size(w - rail_w, top).window_drag())
}
