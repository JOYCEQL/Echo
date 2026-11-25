//! Library dialogs use Rune Kit's modal focus, dismissal and background inertness.
use std::cell::RefCell;
use crate::{model::{State, WorkspaceMode}, icons::*, ui::{c, font, label, theme_ink, theme_muted, editor_colors}};
use rune::{El, Key, ScrollSpec, TextEdit, Tx, col, el, row, text_input};

#[derive(Default)]
struct SearchResults {
    query: Option<String>,
    ids: Vec<String>,
    selected: usize,
}
#[derive(Clone)]
pub struct NewPage { pub book: String, pub parent: Option<String>, pub book_select_open: bool }
pub struct Dialogs {
    pub new_page: Option<NewPage>,
    pub new_book_open: bool,
    pub delete_book: Option<String>,
    pub name: TextEdit,
    pub error: Option<String>,
    search: RefCell<SearchResults>,
}
impl Default for Dialogs {
    fn default() -> Self {
        Self { new_page: None, new_book_open: false, delete_book: None, name: TextEdit::new(""), error: None, search: RefCell::new(SearchResults::default()) }
    }
}
pub fn open_search(s: &mut State, tx: &mut Tx) {
    s.finish_edit();
    if s.document_save_failed { return; }
    s.query.clear();
    *s.library_dialogs.search.borrow_mut() = SearchResults::default();
    s.search_open = true;
    tx.focus("search-field", false);
    tx.animated();
}
fn close_search(s: &mut State) {
    s.search_open = false;
    s.query.clear();
    s.query.focused = false;
}
pub fn search_ids(s: &State) -> Vec<String> {
    let query = s.query.text.trim().to_lowercase();
    let mut cache = s.library_dialogs.search.borrow_mut();
    if cache.query.as_deref() != Some(query.as_str()) {
        let mut matches: Vec<_> = s.notes.iter().filter(|n| n.notebook != "回收站" &&
            (query.is_empty() || n.title.to_lowercase().contains(&query) || n.preview.to_lowercase().contains(&query)
                || n.blocks.iter().any(|b| b.body.to_lowercase().contains(&query) || b.title.to_lowercase().contains(&query))
                || n.doc.as_ref().is_some_and(|d| d.to_plain_text().to_lowercase().contains(&query)))).collect();
        matches.sort_by(|a,b| b.title.to_lowercase().contains(&query).cmp(&a.title.to_lowercase().contains(&query))
            .then_with(|| b.updated.cmp(&a.updated)).then_with(|| a.id.cmp(&b.id)));
        cache.ids = matches.iter().map(|n| n.id.clone()).collect();
        cache.query = Some(query);
        cache.selected = 0;
    }
    cache.ids.clone()
}
fn open_result(s: &mut State, id: &str, tx: &mut Tx) {
    s.select_note(id);
    if s.current != id || s.document_save_failed { return; }
    for (ancestor, _) in s.note_breadcrumbs(id) { s.tree_expanded.insert(ancestor); }
    close_search(s);
    tx.blur();
    tx.animated();
}
fn open_selected(s: &mut State, tx: &mut Tx) {
    let ids = search_ids(s);
    let index = s.library_dialogs.search.borrow().selected;
    if let Some(id) = ids.get(index) { open_result(s, id, tx); }
}
pub fn open_new_page(s: &mut State, book: &str, parent: Option<String>, tx: &mut Tx) {
    s.finish_edit();
    if s.document_save_failed { return; }
    if !s.book_order.iter().any(|b| b == book) { return; }
    s.library_dialogs.new_page = Some(NewPage { book: book.into(), parent, book_select_open: false });
    tx.focus("new-page-document", false);
    tx.animated();
}
fn close_new_page(s: &mut State, tx: &mut Tx) { s.library_dialogs.new_page = None; tx.animated(); }
fn create_page(s: &mut State, mode: WorkspaceMode, tx: &mut Tx) {
    let Some(target) = s.library_dialogs.new_page.clone() else { return; };
    if !s.book_order.contains(&target.book) { return; }
    if target.parent.as_ref().is_some_and(|id| !s.notes.iter().any(|n| &n.id == id && n.notebook == target.book)) { return; }
    let id = match mode {
        WorkspaceMode::Document => s.create_doc_note(&target.book, target.parent.clone()),
        WorkspaceMode::Canvas => s.create_canvas_note(&target.book, target.parent.clone()),
    };
    if id.is_empty() || s.document_save_failed { return; }
    if let Some(parent) = target.parent { s.tree_expanded.insert(parent); }
    close_new_page(s, tx);
    if mode == WorkspaceMode::Canvas { s.focus_page_rename(&id, tx); } else { tx.blur(); }
    tx.animated();
}
fn page_type_option(s: &State, mode: WorkspaceMode) -> El {
    let document = mode == WorkspaceMode::Document;
    let theme = s.theme();
    let ink = theme_ink(theme);
    let muted = theme_muted(theme);
    let tint = if document { theme.choose(0x627667, 0xa8bdad) } else { theme.choose(0x966e48, 0xc9ab89) };
    let paper = || el().radius(3.).fill(c(editor_colors(theme).raised))
        .border(1., c(tint).alpha(0.42)).shadow(1., 2., c(tint).alpha(0.08));
    let line = |w, h| el().size(w, h).radius(1.).fill(c(tint).alpha(0.65));
    let drawing = if document {
        paper().size(48., 54.).child(line(22., 3.).absolute(7., 9.))
            .child(line(32., 1.5).absolute(7., 20.)).child(line(28., 1.5).absolute(7., 26.))
            .child(line(32., 1.5).absolute(7., 32.)).child(line(19., 1.5).absolute(7., 38.))
    } else {
        el().size(70., 50.)
            .child(paper().absolute(0., 1.).size(39., 25.)
                .child(line(18., 2.).absolute(6., 6.)).child(line(24., 1.5).absolute(6., 13.)))
            .child(paper().absolute(35., 19.).size(33., 28.)
                .child(el().absolute(6., 5.).size(20., 16.).radius(2.).fill(c(tint).alpha(0.22))))
            .child(paper().absolute(7., 32.).size(19., 12.))
    };
    col().key(format!("new-page-{}", mode.key())).w(0.).grow(1.).min_w(0.)
        .padding(8.).radius(13.).gap(9.).fill(c(editor_colors(theme).raised))
        .border(1., c(ink).alpha(0.13)).hover(c(ink).alpha(0.045)).press(c(ink).alpha(0.07))
        .role("button").label(s.tr(mode.name())).focusable(true).cursor(rune::Cursor::Pointer)
        .press_scale(0.985).focus_ring(2., 2., c(crate::ui::accent(s)))
        .child(el().w_full().h(75.).center().radius(8.)
            .fill(c(if document { theme.choose(0xeef2ed, 0x343d35) } else { theme.choose(0xf6f0e8, 0x403831) }))
            .child(drawing).a11y_hidden().inert())
        .child(col().w_full().px(5.).pb(6.).gap(3.)
            .child(row().w_full().center_y().gap(4.)
                .child(label(s.tr(mode.name()), 14., 600., ink).grow(1.).min_w(0.).wrap())
                .child(ico(&CHEVRON_RIGHT, 13.).color(c(muted))))
            .child(label(s.tr(if document { "连续写作，组织段落与大纲" } else { "自由摆放文字、图片与卡片" }), 12., 400., muted).w_full().wrap()))
        .on_press_tx(move |s: &mut State, tx| create_page(s, mode, tx))
        .on_key(move |s: &mut State, e, tx| {
            if e.key == Key::Enter { create_page(s, mode, tx); true } else { false }
        })
}
pub fn open_new_notebook(s: &mut State, tx: &mut Tx) {
    s.finish_edit();
    if s.document_save_failed { return; }
    s.library_dialogs.name = TextEdit::new("");
    s.library_dialogs.error = None;
    s.library_dialogs.new_book_open = true;
    tx.focus("notebook-name", false);
    tx.animated();
}
fn close_new_notebook(s: &mut State, tx: &mut Tx) {
    tx.animated();
    s.library_dialogs.new_book_open = false;
    s.library_dialogs.name.focused = false;
    s.library_dialogs.error = None;
}
pub fn create_notebook(s: &mut State, tx: &mut Tx) {
    let name = s.library_dialogs.name.text.trim().to_owned();
    if name.is_empty() {
        s.library_dialogs.error = Some(s.tr("请输入空间名称").into());
        return;
    }
    if s.book_order.iter().map(String::as_str).chain(std::iter::once("回收站"))
        .any(|key| s.book_name(key).to_lowercase() == name.to_lowercase()) {
        s.library_dialogs.error = Some(s.tr("已有同名空间").into());
        return;
    }
    let key = format!("notebook-{}", crate::model::uid());
    let mut order = s.book_order.clone();
    order.push(key.clone());
    if let Err(error) = s.store.set_preferences(&[("book_order", &order.join(",")), (&format!("book-name/{key}"), &name)]) {
        s.library_dialogs.error = Some(format!("{}: {error}", s.tr("保存失败")));
        return;
    }
    s.book_order = order;
    s.book_names.insert(key.clone(), name);
    s.select_book(&key);
    close_new_notebook(s, tx);
    tx.focus(format!("book-{}", s.book_order.len()-1), false);
    tx.scroll_into_view(format!("book-{}", s.book_order.len()-1));
    tx.animated();
}
pub fn open_delete_notebook(s: &mut State, book: &str, tx: &mut Tx) {
    s.finish_edit();
    if s.document_save_failed { return; }
    s.library_dialogs.delete_book = Some(book.into());
    s.library_dialogs.error = None;
    tx.focus("delete-notebook-cancel", false);
    tx.animated();
}
fn close_delete_notebook(s: &mut State, tx: &mut Tx) {
    tx.animated();
    s.library_dialogs.delete_book = None;
    s.library_dialogs.error = None;
}
fn confirm_delete_notebook(s: &mut State, tx: &mut Tx) {
    let Some(book) = s.library_dialogs.delete_book.clone() else { return; };
    match s.delete_notebook(&book) {
        Ok(()) => {
            close_delete_notebook(s, tx);
            tx.focus("new-notebook", false);
        }
        Err(error) => s.library_dialogs.error = Some(format!("{}: {error}", s.tr("删除失败"))),
    }
    tx.animated();
}
fn button(s: &State, key: &str, title: &str, primary: bool) -> El {
    crate::overlays::button(s, key, title, primary, false)
}
pub fn layers(s: &State, w: f32, h: f32) -> Vec<El> {
    let mut layers = Vec::new();
    if let Some(target) = &s.library_dialogs.new_page {
        let mut select_layers = Vec::new();
        let books: Vec<_> = s.book_order.iter().map(|id| (id.clone(), s.book_name(id).to_owned())).collect();
        let names: Vec<_> = books.iter().map(|(_, name)| name.as_str()).collect();
        let field_width = crate::overlays::DIALOG_WIDTH.min(w - 32.) - 2. * crate::overlays::DIALOG_PADDING;
        let mut kit = crate::ui::kit(s);
        kit.ty.ui_compact = font(14., 450.).lh(20.);
        kit.c.bg_base = c(editor_colors(s.theme()).inset);
        kit.c.layer_01 = c(editor_colors(s.theme()).raised);
        kit.c.hover = c(theme_ink(s.theme())).alpha(0.075);
        kit.elevation.button_neutral.clear();
        kit.elevation = kit.elevation.with_input(Vec::new());
        let select = rune_kit::Select::new("new-page-notebook", &names)
            .current(Some(s.book_name(&target.book)))
            .label(s.tr("保存到空间"))
            .appearance(rune_kit::SelectAppearance::Field)
            .placement(rune_kit::Placement::BottomStart)
            .width(field_width)
            .open(target.book_select_open)
            .on_toggle(|s: &mut State| {
                if let Some(target) = &mut s.library_dialogs.new_page { target.book_select_open = !target.book_select_open; }
            })
            .on_close(|s: &mut State| {
                if let Some(target) = &mut s.library_dialogs.new_page { target.book_select_open = false; }
            })
            .on_choose(move |s: &mut State, name, tx| {
                let Some((id, _)) = books.iter().find(|(_, label)| label == name) else { return; };
                if let Some(target) = &mut s.library_dialogs.new_page {
                    if target.book != *id { target.book = id.clone(); target.parent = None; }
                    target.book_select_open = false;
                }
                tx.focus("new-page-notebook-trigger", false);
            })
            .build(&kit, &mut select_layers)
            .w_full().h(42.).px(6.).radius(11.).cursor(rune::Cursor::Pointer)
            .border(1., c(theme_ink(s.theme())).alpha(if target.book_select_open { 0.30 } else { 0.095 }));
        let mut location = col().w_full().gap(7.)
            .child(label(s.tr("保存到空间"), 12., 500., theme_muted(s.theme())))
            .child(select);
        if let Some(parent) = target.parent.as_ref().and_then(|id| s.notes.iter().find(|n| &n.id == id)) {
            location = location.child(row().w_full().gap(6.).center_y()
                .child(ico(&FILE_TEXT, 13.).color(c(theme_muted(s.theme()))))
                .child(label(&parent.title, 12., 400., theme_muted(s.theme())).truncate()));
        }
        let body = col().w_full().gap(18.)
            .child(location)
            .child(row().w_full().gap(10.)
                .child(page_type_option(s, WorkspaceMode::Document))
                .child(page_type_option(s, WorkspaceMode::Canvas)));
        layers.extend(crate::overlays::dialog(s, [w,h], "new-page",
            s.tr(if target.parent.is_some() { "新建子页面" } else { "新建页面" }), body, close_new_page));
        // Keep Rune's listbox, focus and dismissal; match the field edges in this dialog.
        if target.book_select_open {
            let listbox = select_layers.pop().expect("an open Rune Select mounts its listbox");
            select_layers.push(listbox.w(field_width).max_w(field_width).radius(12.)
                .border(1., c(theme_ink(s.theme())).alpha(0.12)));
        }
        layers.extend(select_layers);
    }
    if s.search_open { search_panel(s, w, h, &mut layers); }
    if s.library_dialogs.new_book_open {
        let field = row().key("notebook-name-field").w_full().h(42.).px(12.).center_y().radius(11.)
            .fill(c(editor_colors(s.theme()).inset))
            .border(1., c(if s.library_dialogs.error.is_some() { s.theme().choose(0xb03935, 0xffa49b) } else if s.library_dialogs.name.focused { crate::ui::accent(s) } else { editor_colors(s.theme()).border }))
            .child(text_input(&s.library_dialogs.name,font(14.,400.).lh(22.)).key("notebook-name").single_line().autofocus()
                .grow(1.).min_w(0.).h(22.).label(s.tr("空间名称"))
                .color(c(theme_ink(s.theme()))).caret(c(crate::ui::accent(s)),c(crate::ui::accent_soft(s)))
                .invalid(s.library_dialogs.error.is_some())
                .on_edit(|s: &mut State| &mut s.library_dialogs.name)
                .on_input_change(|s: &mut State, _| s.library_dialogs.error = None)
                .on_submit(create_notebook));
        let mut body = col().w_full().gap(8.)
            .child(label(s.tr("空间名称"), 12., 500., theme_muted(s.theme()))).child(field);
        if let Some(error) = &s.library_dialogs.error { body=body.child(label(error,12.,400.,s.theme().choose(0xa94740,0xffa49b)).w_full().wrap()); }
        body=body.child(row().w_full().pt(12.).gap(8.).child(el().grow(1.))
            .child(button(s,"new-notebook-cancel",s.tr("取消"),false).on_press_tx(close_new_notebook))
            .child(button(s,"new-notebook-create",s.tr("创建"),true).on_press_tx(create_notebook)));
        layers.extend(crate::overlays::dialog(s, [w,h], "new-notebook", s.tr("新建空间"), body, close_new_notebook));
    }
    if let Some(book) = &s.library_dialogs.delete_book {
        let count = s.notes.iter().filter(|n| n.notebook == *book).count();
        let title = s.tr("删除空间");
        let description = if s.language.is_english() {
            if count == 0 { "This space is empty. It will be removed from the sidebar.".to_owned() }
            else { format!("{count} {} will move to Trash, including documents and canvases. Restoring a note will also restore this space.", if count == 1 { "note" } else { "notes" }) }
        } else if count == 0 { "这是一个空空间，删除后将从侧边栏移除。".to_owned() }
        else { format!("其中的 {count} 篇笔记（包含结构化文档和自由画布）将移至回收站。恢复笔记时，也会恢复对应的空间。") };
        let mut body = col().w_full().gap(8.)
            .child(label(s.book_name(book),16.,600.,theme_ink(s.theme())).w_full().wrap())
            .child(label(description,13.,400.,theme_muted(s.theme())).w_full().wrap());
        if let Some(error) = &s.library_dialogs.error {
            body = body.child(label(error,12.,400.,s.theme().choose(0xa94740,0xffa49b)).w_full().wrap());
        }
        body = body.child(row().w_full().pt(12.).gap(8.).child(el().grow(1.))
            .child(button(s,"delete-notebook-cancel",s.tr("取消"),false).autofocus().on_press_tx(close_delete_notebook)
                .on_key(|s: &mut State,e,tx| { if e.key == Key::Enter { close_delete_notebook(s, tx); true } else { false } }))
            .child(crate::overlays::button(s,"delete-notebook-confirm",s.tr("删除空间"),true,true)
                .on_press_tx(confirm_delete_notebook)
                .on_key(|s: &mut State,e,tx| { if e.key == Key::Enter { confirm_delete_notebook(s,tx); true } else { false } })));
        layers.extend(crate::overlays::dialog(s, [w,h], "delete-notebook", title, body, close_delete_notebook));
    }
    layers
}

fn dismiss_search(s: &mut State, tx: &mut Tx) {
    close_search(s);
    tx.animated();
}
fn search_key(s: &State, value: &str) -> El {
    el().h(20.).min_w(24.).px(5.).radius(4.).center()
        .fill(c(editor_colors(s.theme()).inset))
        .border(0.5, c(editor_colors(s.theme()).border))
        .child(label(value, 10.5, 450., theme_muted(s.theme())))
}
fn search_panel(s: &State, w: f32, h: f32, layers: &mut Vec<El>) {
    let theme = s.theme();
    let ink = theme_ink(theme);
    let muted = theme_muted(theme);
    let colors = editor_colors(theme);
    let ids = search_ids(s);
    let selected = s.library_dialogs.search.borrow().selected;
    let width = (w - 48.).min(560.);
    // Whole rows, with a stable viewport while typing; no clipped half-row at rest.
    let result_height = ((h - 240.) / 56.).floor().clamp(2., 6.) * 56.;
    let height = 64. + 1. + 32. + result_height + 8. + 1. + 38.;
    let top = ((h - height) * 0.34).max(48.);
    let input = text_input(&s.query, font(17., 400.).lh(26.)).key("search-field")
        .single_line().autofocus().grow(1.).min_w(0.).color(c(ink))
        .label(s.tr("搜索笔记"))
        .caret(c(crate::ui::accent(s)), c(crate::ui::accent_soft(s)))
        .placeholder(s.tr("搜索标题或正文…"), c(muted))
        .on_edit(|s: &mut State| &mut s.query).on_submit(open_selected)
        .on_input_key(|s: &mut State, e, tx| {
            if s.query.is_composing() || !matches!(e.key, Key::Up | Key::Down) { return false; }
            let ids = search_ids(s);
            if !ids.is_empty() {
                let mut cache = s.library_dialogs.search.borrow_mut();
                cache.selected = if e.key == Key::Down { (cache.selected + 1).min(ids.len()-1) }
                    else { cache.selected.saturating_sub(1) };
                tx.scroll_into_view(format!("search-result-{}", ids[cache.selected]));
            }
            tx.instant(); true
        });
    let header = row().w_full().h(64.).px(20.).gap(12.).center_y()
        .child(ico(&SEARCH, 19.).color(c(muted))).child(input)
        .child(el().key("search-close").size(32., 28.).radius(6.).center()
            .role("button").label(s.tr("关闭")).help(s.tr("关闭"))
            .focusable(true).cursor(rune::Cursor::Pointer)
            .hover(c(ink).alpha(0.06)).child(search_key(s, "esc"))
            .on_press_tx(dismiss_search));
    let mut results = col().key("search-results").w_full().h(result_height).px(8.)
        .scroll(ScrollSpec { stick_bottom: false });
    for (i, id) in ids.iter().enumerate() {
        let note = s.notes.iter().find(|n| n.id == *id).expect("search result belongs to library");
        let target = id.clone();
        let mut path = vec![s.book_name(&note.notebook).to_owned()];
        path.extend(s.note_breadcrumbs(id).into_iter().filter(|(ancestor, _)| ancestor != id).map(|(_, title)| title));
        let format = if note.format == crate::doc_core::NoteFormat::Document { s.tr("结构化文档") } else { s.tr("自由画布") };
        results = results.child(row().key(format!("search-result-{id}")).w_full().h(56.).px(12.).gap(12.).center_y().radius(9.)
            .fill(if i == selected { crate::ui::navigation_selection(theme) } else { rune::Color::TRANSPARENT })
            .hover(c(ink).alpha(0.045)).role("button").focusable(true).label(&note.title)
            .cursor(rune::Cursor::Pointer)
            .child(el().size(30., 34.).center()
                .child(s.notebook_icon_style.page_icon(note.format, 19., muted)))
            .child(col().grow(1.).min_w(0.).gap(2.)
                .child(label(&note.title, 14., 500., ink).truncate())
                .child(label(path.join(" / "), 11.5, 400., muted).truncate()))
            .child(label(format, 10.5, 400., muted))
            .on_press_tx(move |s: &mut State, tx| open_result(s, &target, tx)));
    }
    if ids.is_empty() {
        results = results.child(col().w_full().h(result_height).gap(9.).center()
            .child(el().size(40., 40.).radius(12.).fill(c(colors.inset)).center().child(ico(&SEARCH, 20.).color(c(muted))))
            .child(label(s.tr("未找到匹配的文档"), 14., 500., ink))
            .child(label(s.tr("尝试搜索其他关键词或标题"), 12., 400., muted)));
    }
    let footer = row().w_full().h(38.).px(20.).gap(6.).center_y()
        .child(search_key(s, "↑↓")).child(label(s.tr("选择"), 11., 400., muted))
        .child(el().w(8.))
        .child(search_key(s, "↵")).child(label(s.tr("打开"), 11., 400., muted))
        .child(el().grow(1.))
        .child(label(s.tr("所有空间"), 11., 400., muted));
    let rule = || el().w_full().h(1.).fill(c(colors.border).alpha(0.7));
    let panel = crate::overlays::surface(s, col().key("library-search-dialog").role("dialog").label(s.tr("搜索笔记"))
        .absolute((w-width)/2., top).size(width, height), 16.)
        .transition(crate::overlays::transition(-6.))
        .modal_tx(true, dismiss_search)
        .child(header).child(rule())
        .child(row().w_full().h(32.).px(20.).center_y()
            .child(label(s.tr(if s.query.text.trim().is_empty() { "最近编辑" } else { "搜索结果" }), 11., 500., muted))
            .child(el().grow(1.)).child(label(ids.len().to_string(), 11., 400., muted)))
        .child(results).child(el().h(8.)).child(rule()).child(footer);
    layers.push(el().key("library-search-scrim").absolute(0.,0.).size(w,h)
        .fill(c(0x000000).alpha(if theme.is_dark() { 0.24 } else { 0.10 }))
        .transition(crate::overlays::transition(0.)).inert().a11y_hidden());
    layers.push(panel);
}
