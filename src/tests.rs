use super::*;
use crate::table_data;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

#[test]
fn update_settings_render_states_in_both_languages_and_persist_opt_in() {
    use crate::update::Status;
    for english in [false, true] {
        for dark in [false, true] {
            let f = Fixture::new();
            let mut s = State::from_root(f.0.clone());
            assert!(!s.updater.automatic);
            s.language = if english { crate::i18n::Language::English } else { crate::i18n::Language::Chinese };
            s.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
            let mut rt = runtime(s, (1000., 736.));
            tap(&mut rt, "settings");
            tap(&mut rt, "settings-tab-updates");
            assert!(matches!(rt.state.settings, Some(crate::model::SettingsPage::Updates)));
            rt.update(|s, tx| { s.updater.status = Status::Idle; tx.instant(); });
            rt.frame(0.02);
            assert!(rt.find("update-action").is_some());
            tap(&mut rt, "update-automatic");
            assert!(rt.state.updater.automatic);
            assert_eq!(rt.state.store.preference("automatic_updates").as_deref(), Some("true"));
            tap(&mut rt, "update-automatic");
            assert!(!rt.state.updater.automatic);
            assert_eq!(rt.state.store.preference("automatic_updates").as_deref(), Some("false"));
            for status in [Status::Checking, Status::Downloading(47), Status::Verifying, Status::Preparing, Status::Installing, Status::Unavailable] {
                rt.update(move |s, tx| { s.updater.status = status; tx.instant(); });
                rt.frame(0.02);
                assert!(rt.find("update-action").is_none(), "busy/unavailable states must not accept another operation");
            }
            for status in [Status::Current, Status::CheckFailed("Offline".into()), Status::Failed("Verification failed".into())] {
                rt.update(move |s, tx| { s.updater.status = status; tx.instant(); });
                rt.frame(0.02);
                assert!(rt.find("update-action").is_some());
            }
            press(&mut rt, Key::Escape);
            assert!(rt.state.settings.is_none());
        }
    }
}

#[test]
fn notebook_creation_follows_books_and_update_progress_stays_above_settings() {
    use crate::update::Status;
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    for status in [Status::Unavailable, Status::Idle, Status::Checking, Status::Current, Status::CheckFailed("Offline".into())] {
        rt.update(move |s, tx| { s.updater.status = status; tx.instant(); });
        rt.frame(0.02);
        assert!(rt.find("editor-updates").is_none());
    }
    let idle_settings = rect(&rt, "settings");
    let idle_list = rect(&rt, "notebook-scroll");
    rt.update(|s, tx| { s.updater.status = Status::Downloading(0); tx.instant(); });
    rt.frame(1.);
    let bounds = rect(&rt, "editor-updates");
    let add = rect(&rt, "new-notebook");
    let settings = rect(&rt, "settings");
    assert_eq!(idle_settings, settings);
    assert!((idle_list[3] - rect(&rt, "notebook-scroll")[3] - bounds[3] - 2.).abs() < 0.1);
    assert_eq!(&bounds[2..], &add[2..]);
    let last_book = rect(&rt, &format!("book-{}", rt.state.book_order.len() - 1));
    assert_eq!(add[1] - (last_book[1] + last_book[3]), 6.);
    assert_eq!(settings[1] - (bounds[1] + bounds[3]), 2.);
    assert!((bounds[0] + bounds[2] / 2. - add[0] - add[2] / 2.).abs() < 0.1);
    assert!(add[1] + add[3] <= bounds[1]);
    for percent in [0, 9, 98, 100] {
        rt.update(move |s, tx| { s.updater.status = Status::Downloading(percent); tx.instant(); });
        rt.frame(0.02);
        assert_eq!(rect(&rt, "editor-updates"), bounds);
        assert_eq!(rt.find("update-wave").is_some(), percent > 0);
    }
    rt.update(|s, tx| { s.toggle_sidebar(); tx.instant(); });
    rt.frame(1.);
    assert!(rt.presented("editor-updates").is_none_or(|(r, _)| r[0] + r[2] <= 0.));
    rt.update(|s, tx| { s.toggle_sidebar(); tx.instant(); });
    rt.frame(1.);
    tap(&mut rt, "editor-updates");
    assert!(matches!(rt.state.updater.status, Status::Downloading(100)));
    assert!(rt.state.settings.is_none());
    rt.update(|s, tx| { s.updater.status = Status::Verifying; tx.instant(); });
    rt.frame(0.02);
    assert!(rt.find("update-wave").is_none());
    rt.update(|s, tx| { s.updater.status = Status::Failed("Verification failed".into()); tx.instant(); });
    rt.frame(0.02);
    assert_eq!(&rect(&rt, "update-disc")[2..], &[24., 24.]);
    assert_eq!(&rect(&rt, "editor-updates-glyph")[2..], &rect(&rt, "new-notebook-glyph")[2..]);
    tap(&mut rt, "editor-updates");
    assert!(matches!(rt.state.settings, Some(crate::model::SettingsPage::Updates)));
    assert!(rt.find("update-action").is_some());
}

#[test]
fn new_notebook_scrolls_with_a_long_list_while_footer_stays_fixed() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    for n in 0..18 { s.book_order.push(format!("scroll-book-{n}")); }
    let last = format!("book-{}", s.book_order.len() - 1);
    let mut rt = runtime(s, (800., 600.));
    let footer = rect(&rt, "settings");
    rt.update(|_, tx| tx.scroll_into_view("new-notebook"));
    rt.frame(1.);
    let add = rect(&rt, "new-notebook");
    let book = rect(&rt, &last);
    assert!((add[1] - book[1] - book[3] - 6.).abs() < 0.1);
    assert!(add[1] >= 0. && add[1] + add[3] <= footer[1]);
    assert_eq!(rect(&rt, "settings"), footer);
    tap(&mut rt, "new-notebook");
    assert!(rt.state.library_dialogs.new_book_open);
}

#[test]
fn markdown_import_picker_preserves_formatting_local_images_and_source_on_reopen() {
    use crate::doc_core::DocBlockKind;
    let f = Fixture::new();
    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("样稿.MD");
    let md = "\u{feff}# 会议记录\r\n\r\n**结论**与[资料][ref]\r\n\r\n[ref]: https://example.org/\r\n\r\n* [x] 已核对\r\n  第二行\r\n\r\n1) 回访\r\n\r\n见图 ![图示](a%20b.png) 后记\r\n";
    std::fs::write(&source, md).unwrap();
    std::fs::write(files.path().join("a b.png"), include_bytes!("../assets/architecture.png")).unwrap();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    let book = rt.state.book.clone();
    let bounds = rect(&rt, "editor-import-markdown");
    assert_eq!(rect(&rt, "editor-new-page")[0] - bounds[0], 32.);
    let initial = rt.state.notes.len();
    tap(&mut rt, "editor-import-markdown");
    assert_eq!(rt.file_dialog().unwrap().0, rune::FileDialogKind::Open);
    rt.resolve_file_dialog(None).unwrap(); rt.frame(0.1);
    assert_eq!(rt.state.notes.len(), initial);
    tap(&mut rt, "editor-import-markdown");
    rt.resolve_file_dialog(Some(source.clone())).unwrap(); rt.frame(0.1);
    assert_eq!(rt.state.notes.len(), initial + 1, "{:?}", rt.state.message);
    assert_eq!(std::fs::read_to_string(source).unwrap(), md);
    drop(files);
    let reopened = State::from_root(f.0.clone());
    let note = reopened.note().unwrap();
    assert_eq!((&note.title, &note.notebook), (&"会议记录".to_string(), &book));
    let blocks = &note.doc.as_ref().unwrap().blocks;
    assert!(blocks[0].text.runs.iter().any(|r| r.marks.bold));
    assert!(blocks[0].text.runs.iter().any(|r| r.marks.link.as_deref() == Some("https://example.org/")));
    assert!(blocks.iter().any(|b| matches!(b.kind, DocBlockKind::Todo { done: true }) && b.text.text.contains("第二行")));
    assert!(blocks.iter().any(|b| matches!(b.kind, DocBlockKind::Ordered {..})));
    let asset = blocks.iter().find_map(|b| match &b.kind { DocBlockKind::Image { src, .. } => Some(src), _ => None }).unwrap();
    assert_eq!(std::fs::read(f.0.join(asset)).unwrap(), include_bytes!("../assets/architecture.png").as_slice());
    assert!(blocks.last().unwrap().text.text.contains("后记"));
}

#[test]
fn markdown_import_drop_creates_independent_pages_and_failed_batches_leave_no_partial_notes() {
    let files = tempfile::tempdir().unwrap();
    let a = files.path().join("one.md");
    let b = files.path().join("two.markdown");
    std::fs::write(&a, "same title\n=====\n\n- item\n").unwrap();
    std::fs::write(&b, "# same title\n\nBody\n").unwrap();
    for canvas in [false, true] {
        let f = Fixture::new();
        let mut s = State::from_root(f.0.clone());
        if canvas { s.create_canvas_note("工作", None); } else { s.create_doc_note("工作", None); }
        let mut rt = runtime(s, (1000., 736.));
        let before = rt.state.notes.len();
        let page = rect(&rt, "editor-surface");
        let at = Some((page[0] + 150., page[1] + 130.));
        rt.drop_files(vec![a.clone(), files.path().join("missing.md")], at);
        rt.frame(0.1);
        assert_eq!(rt.state.notes.len(), before);
        assert!(rt.state.message.as_deref().is_some_and(|s| s.contains("missing.md")));
        rt.drop_files(vec![a.clone(), b.clone()], at);
        rt.frame(0.1);
        assert_eq!(rt.state.notes.len(), before + 2, "{:?}", rt.state.message);
        let reopened = State::from_root(f.0.clone());
        assert_eq!(reopened.note().unwrap().title, "same title");
        assert_eq!(reopened.notes.iter().filter(|n| n.title == "same title").count(), 2);
        assert_eq!(rt.state.mode, WorkspaceMode::Document);
        assert_eq!(rt.state.editor.active.note_id, rt.state.current);
    }
}

#[path = "performance_audit.rs"]
mod performance_audit;

fn choose_new_page(rt: &mut Runtime<State>, mode: WorkspaceMode) {
    rt.frame(0.02);
    assert!(rt.state.library_dialogs.new_page.is_some());
    tap(rt, &format!("new-page-{}", mode.key()));
}
fn create_same_type(rt: &mut Runtime<State>) {
    let mode = rt.state.mode;
    tap(rt, "editor-new-page");
    choose_new_page(rt, mode);
}

fn layout_test_runtime(doc: crate::doc_core::DocDocument, size: (f32, f32)) -> (Fixture, Runtime<State>) {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.choose_mode(WorkspaceMode::Document);
    s.create_doc_note("工作", None);
    s.note_mut().unwrap().doc = Some(doc.clone());
    s.editor.active.load(&s.current.clone(), doc, true);
    let mut rt = runtime(s, size);
    for _ in 0..10 { rt.frame(0.02); }
    (f, rt)
}

#[test]
fn slash_popovers_fade_without_moving_in_both_workspaces() {
    let (_cf, mut canvas, _) = fresh_body();
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    state.create_doc_note("工作", None);
    let mut doc = runtime(state, (1280., 943.));
    press(&mut doc, Key::Enter);
    let menu = |rt: &Runtime<State>, key: &str| rt.elements().into_iter().find(|el| el.key.as_deref() == Some(key));
    for (rt, key) in [(&mut canvas, "doc-slash-menu"), (&mut doc, "doc-slash-menu")] {
        rt.frame(1.);
        type_text(rt, "/");
        rt.frame(0.04);
        let entering = menu(rt, key).expect("slash menu enters");
        assert!(entering.opacity > 0. && entering.opacity < 1., "{key}: entrance must have an intermediate opacity, got {}", entering.opacity);
        rt.frame(0.5);
        let settled = menu(rt, key).unwrap();
        assert!((settled.opacity - 1.).abs() < 0.01);
        assert!((entering.rect[1] - settled.rect[1]).abs() < 0.1, "slash menu must stay anchored while fading: {:?} -> {:?}", entering.rect, settled.rect);
        press(rt, Key::Escape);
        rt.frame(0.03);
        let exiting = menu(rt, key).expect("exit retains the menu for its fade");
        assert!(exiting.exiting && exiting.opacity > 0. && exiting.opacity < 1.);
        rt.frame(0.5);
        assert!(menu(rt, key).is_none());
        press(rt, Key::Backspace);
        type_text(rt, "、");
        rt.frame(0.5);
        press(rt, Key::Backspace);
        rt.frame(0.03);
        assert!(menu(rt, key).is_some_and(|e| e.exiting), "deleting trigger also fades out");
        rt.frame(0.5);
    }
}

#[test]
fn structured_interaction_colours_follow_theme_and_do_not_persist_command_marks() {
    for color in [0xece7df, 0xdce8f3, 0xe1eae1, 0xe8e4f2, 0xff694f, 0x808080] {
    for theme in [Theme::Light, Theme::Dark] {
        let f = Fixture::new();
        let mut s = State::from_root(f.0.clone());
        s.sidebar_material.colors = vec![crate::material::Tint::from_hex(color)];
        s.choose_mode(WorkspaceMode::Document);
        s.create_doc_note("工作", None);
        s.sidebar_material.mode = if theme.is_dark() { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
        let mut rt = runtime(s, (1280., 943.));
        press(&mut rt, Key::Enter);
        type_text(&mut rt, "/");
        rt.frame(0.5);
        let elements = rt.elements();
        let caret = elements.iter().find(|e| e.key.as_deref() == Some("doc-caret")).unwrap();
        assert_eq!(caret.fill, crate::ui::c(crate::ui::accent(&rt.state)));
        let selected = elements.iter().find(|e| e.key.as_deref() == Some("slash-item-text")).unwrap();
        assert_eq!(selected.fill, crate::ui::c(crate::ui::accent_soft(&rt.state)));
        let id = rt.state.editor.active.core.sel.head.block;
        let meta = rt.state.editor.active.text_layout(id).unwrap();
        assert!(meta.spans.iter().any(|span| span.color == Some(crate::ui::c(crate::ui::accent(&rt.state)))));
        assert!(rt.state.editor.active.core.text_of(id).unwrap().runs.iter().all(|r| r.marks.color.is_none()));
        let before = rt.state.editor.active.core.doc.clone();
        let old_accent = crate::ui::accent(&rt.state);
        rt.update(|s, tx| {
            s.sidebar_material.colors[0] = crate::material::Tint::from_hex(0xf1d1ec);
            tx.instant();
        });
        rt.frame(0.5);
        assert_ne!(old_accent, crate::ui::accent(&rt.state));
        assert_eq!(rt.elements().iter().find(|e| e.key.as_deref() == Some("slash-item-text")).unwrap().fill,
            crate::ui::c(crate::ui::accent_soft(&rt.state)));
        assert!(rt.state.editor.active.text_layout(id).unwrap().spans.iter()
            .any(|sp| sp.color == Some(crate::ui::c(crate::ui::accent(&rt.state)))));
        assert_eq!(before, rt.state.editor.active.core.doc, "appearance must not rewrite document content");
        press(&mut rt, Key::Escape);
        rt.frame(0.5);
        assert!(rt.state.editor.active.text_layout(id).unwrap().spans.is_empty());
    }
}
}
#[test]
fn structured_code_highlighting_tracks_language_unicode_and_composition() {
    use crate::doc_core::{DocBlockKind as K, DocDocument, DocPos};
    let mut doc = DocDocument::empty();
    let source = "const greeting = \"你好 🌱\"; // comment";
    let code = doc.new_block(K::Code { lang: "TypeScript".into(), wrap: true }, source);
    let id = code.id;
    doc.blocks = vec![code];
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    let meta = rt.state.editor.active.text_layout(id).unwrap();
    let token = |needle: &str| {
        let at = source.find(needle).unwrap() as u32;
        meta.spans.iter().find(|sp| sp.start <= at && sp.end > at).unwrap().color
    };
    assert_ne!(token("const"), token("你好"));
    assert_ne!(token("const"), token("//"));
    for sp in &meta.spans {
        assert!(source.is_char_boundary(sp.start as usize) && source.is_char_boundary(sp.end as usize));
        assert!(sp.weight.is_none(), "syntax colours must not change glyph advances");
    }
    let off = source.find(" 🌱").unwrap();
    let coloured = rune::text::doc_text::caret_rect(source, &meta.spans, meta.style, meta.max_width, meta.align, off);
    let plain = rune::text::doc_text::caret_rect(source, &[], meta.style, meta.max_width, meta.align, off);
    assert_eq!(coloured, plain);
    rt.update(|s, tx| {
        s.editor.active.core.set_head(DocPos::new(id, off), false);
        crate::doc_editor::on_ime(s, &rune::Ime::Preedit("中文".into(), Some((6, 6))), tx);
    });
    rt.frame(0.02);
    let composing = rt.state.editor.active.text_layout(id).unwrap();
    assert!(composing.display_text.contains("你好中文 🌱"));
    assert!(composing.spans.iter().filter(|sp| sp.color.is_some()).count() > 3);
    rt.update(|s, tx| {
        crate::doc_editor::on_ime(s, &rune::Ime::Preedit(String::new(), None), tx);
        if let K::Code { lang, .. } = &mut s.editor.active.core.block_mut(id).unwrap().kind {
            *lang = "Plain Text".into();
        }
    });
    rt.frame(0.02);
    assert!(rt.state.editor.active.text_layout(id).unwrap().spans.is_empty());
    let first = rune_kit::syntax::highlight(Some("TypeScript"), source);
    let second = rune_kit::syntax::highlight(Some("TypeScript"), source);
    assert!(std::rc::Rc::ptr_eq(&first, &second), "unchanged code reuses Rune's cached tokens");
}

#[test]
fn document_tree_selection_and_hover_follow_nested_widths() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let parent = state.create_doc_note("工作", None);
    let child = state.create_doc_note("工作", Some(parent.clone()));
    let grandchild = state.create_doc_note("工作", Some(child.clone()));
    state.tree_expanded.extend(["book:工作".into(), parent.clone(), child.clone()]);
    let mut rt = runtime(state, (1280., 943.));
    rt.set_system_reduced_motion(true);
    let mut widths = Vec::new();
    for id in [&parent, &child, &grandchild] {
        rt.update(|s, _| s.select_note(id));
        rt.frame(1.);
        let row = rect(&rt, &format!("note-{id}"));
        assert_navigation_selected(&rt, &format!("note-{id}"));
        widths.push(row);
    }
    for pair in widths.windows(2) {
        assert!((pair[1][0] - pair[0][0] - 14.).abs() < 0.1);
        assert!((pair[1][0] + pair[1][2] - pair[0][0] - pair[0][2]).abs() < 0.1);
    }
    assert!(widths[0][2] < rect(&rt, "note-list")[2]);
}

#[test]
fn structured_layout_anchors_and_rhythm() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    doc.blocks.clear();
    let paragraph = doc.new_block(K::Text, "正文");
    let heading = doc.new_block(K::Heading { level: 2 }, "标题");
    let (pid, hid) = (paragraph.id, heading.id);
    doc.blocks.extend([paragraph, heading]);
    let mut ids = vec![];
    for kind in [K::Bullet, K::Todo { done: false }, K::Ordered { start: Some(1) },
        K::Ordered { start: Some(99) }, K::Ordered { start: None }, K::Bullet] {
        let b = doc.new_block(kind, "同级内容 Abc");
        ids.push(b.id);
        doc.blocks.push(b);
    }
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    let text = |id| rect(&rt, &format!("doc-text-{id}"));
    let origin = text(pid)[0];
    assert!((origin - text(hid)[0]).abs() < 0.1);
    for id in [ids[0], ids[1], ids[2], ids[5]] {
        assert!((text(id)[0] - origin - 24.).abs() < 0.1, "24pt reference list inset; large numbers must not move other rows");
    }
    assert!(text(ids[4])[0] > text(ids[3])[0], "number width grows per item");
    for pair in ids.windows(2) {
        assert!((text(pair[1])[1] - text(pair[0])[1] - 34.).abs() < 0.1, "26pt line plus one collapsed 8pt gap");
    }
    let dot = rect(&rt, &format!("doc-marker-{}", ids[0]));
    let checkbox = rect(&rt, &format!("doc-marker-{}", ids[1]));
    assert!((dot[0] - origin - 4.).abs() < 0.1);
    assert!((dot[2] - 5.5).abs() < 0.1);
    assert!((checkbox[0] - origin).abs() < 0.1, "checkbox begins at the paragraph edge");
    assert!((checkbox[1] + 8. - text(ids[1])[1] - 13.).abs() < 0.1);
    let body = rt.state.editor.active.text_layout(pid).unwrap();
    let heading = rt.state.editor.active.text_layout(hid).unwrap();
    assert_eq!(body.style.size, 16.);
    assert_eq!(heading.style.size, 22.);
    assert_eq!(heading.style.weight, 500.);
    assert!((text(hid)[1] - (text(pid)[1] + text(pid)[3]) - 22.).abs() < 0.1);
    let target = rect(&rt, &format!("todo-chk-{}", ids[1]));
    assert!(target[2] >= 24. && target[3] >= 26.);
    tap(&mut rt, &format!("todo-chk-{}", ids[1]));
    assert!(matches!(rt.state.editor.active.core.block(ids[1]).unwrap().kind, K::Todo { done: true }));
    tap(&mut rt, &format!("todo-chk-{}", ids[1]));
    assert!(matches!(rt.state.editor.active.core.block(ids[1]).unwrap().kind, K::Todo { done: false }));
}

#[test]
fn structured_layout_container_indent_is_local_at_every_depth() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    let a = doc.new_block(K::Text, "引用正文");
    let aid = a.id;
    let top = doc.new_container(K::Quote, a);
    let tid = top.id;
    let b = doc.new_block(K::Text, "引用正文");
    let bid = b.id;
    let deep = doc.new_container(K::Quote, b);
    let did = deep.id;
    let calltext = doc.new_block(K::Text, "高亮正文");
    let cid = calltext.id;
    let callout = doc.new_container(K::Callout { emoji: "💡".into(), palette: "blue".into() }, calltext);
    let coid = callout.id;
    let mut parent = doc.new_block(K::Bullet, "父级");
    let mut child = doc.new_block(K::Bullet, "子级");
    let mut grandchild = doc.new_block(K::Bullet, "孙级");
    grandchild.children = vec![deep, callout];
    child.children.push(grandchild);
    parent.children.push(child);
    doc.blocks = vec![top, parent];
    let (_f, rt) = layout_test_runtime(doc, (1000., 736.));
    let x = |key: String| rect(&rt, &key)[0];
    let top_inset = x(format!("doc-text-{aid}")) - x(format!("doc-blk-{tid}"));
    let deep_rect = rect(&rt, &format!("doc-blk-{did}"));
    // The virtual row owns depth-three padding; the container itself starts after it.
    let deep_inset = x(format!("doc-text-{bid}")) - deep_rect[0] - 75.;
    assert!((top_inset - 14.).abs() < 0.1);
    assert!((deep_inset - top_inset).abs() < 0.1, "no global depth applied inside quote");
    let call = rect(&rt, &format!("doc-blk-{coid}"));
    let txt = rect(&rt, &format!("doc-text-{cid}"));
    assert!((txt[0] - call[0] - 75. - 53.).abs() < 0.1);
    assert!(txt[0] + txt[2] <= call[0] + call[2] - 13.9);
}

#[test]
fn structured_layout_code_line_numbers_follow_wrap_and_horizontal_scroll() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    let first = format!("const data = \"{}END_MARKER\";", "abc_中文_".repeat(50));
    let content = format!("{first}\nconsole.log(data);\nreturn data;");
    let code = doc.new_block(K::Code { lang: "TypeScript".into(), wrap: true }, &content);
    let id = code.id;
    doc.blocks = vec![code];
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    let meta = rt.state.editor.active.text_layout(id).unwrap();
    let code_box = rect(&rt, &format!("doc-blk-{id}"));
    assert!((meta.rect[0] - code_box[0] - 60.).abs() < 0.1, "code block text inset");
    assert!((meta.rect[1] - code_box[1] - 41.).abs() < 0.1, "code content starts below header");
    let second = rune::text::doc_text::caret_rect(&meta.display_text, &meta.spans, meta.style,
        meta.max_width, meta.align, first.len() + 1);
    assert!(second[1] > 22.);
    assert!((rect(&rt, &format!("code-line-{id}-1"))[1] - meta.rect[1]).abs() < 0.1);
    assert!((rect(&rt, &format!("code-line-{id}-2"))[1] - meta.rect[1] - second[1]).abs() < 0.1);
    tap(&mut rt, &format!("code-wrap-{id}"));
    for _ in 0..30 { rt.frame(0.02); }
    let meta = rt.state.editor.active.text_layout(id).unwrap();
    assert_eq!(meta.max_width, None, "unwrapped text uses unbounded hit geometry");
    assert!((meta.rect[3] - 66.).abs() < 0.1);
    let viewport = rect(&rt, &format!("code-viewport-{id}"));
    assert!(meta.rect[2] > viewport[2]);
    let line_x = rect(&rt, &format!("code-line-{id}-1"))[0];
    rt.wheel_xy(viewport[0] + 40., viewport[1] + 12., -10000., 0.);
    for _ in 0..40 { rt.frame(0.02); }
    let moved = rt.state.editor.active.text_layout(id).unwrap();
    assert!(moved.rect[0] < meta.rect[0] - 100., "code really scrolls horizontally");
    assert_eq!(rect(&rt, &format!("code-line-{id}-1"))[0], line_x, "line numbers stay fixed");
    let off = first.len() - 2;
    let caret = rune::text::doc_text::caret_rect(&moved.display_text, &moved.spans,
        moved.style, moved.max_width, moved.align, off);
    let x = moved.rect[0] + caret[0];
    assert!(x >= viewport[0] && x < viewport[0] + viewport[2]);
    assert_eq!(rt.state.editor.active.hit_test_window(x, moved.rect[1] + 11.).offset, off);
    rt.update(|s, tx| {
        s.editor.active.core.set_head(crate::doc_core::DocPos::new(id, 0), false);
        tx.scroll_to_x(format!("code-viewport-{id}"), 0.);
    });
    rt.key(KeyEvent { key: Key::Right, mods: Mods { cmd: true, ..Default::default() } });
    for _ in 0..40 { rt.frame(0.02); }
    let caret = rect(&rt, "doc-caret");
    assert!(caret[0] >= viewport[0] && caret[0] + caret[2] <= viewport[0] + viewport[2] + 0.1,
        "keyboard navigation reveals the unwrapped code caret");
}

#[test]
fn editor_scrollbar_stays_on_the_outer_edge_and_drags_the_viewport() {
    use crate::doc_core::{DocBlockKind, DocDocument};
    let mut doc = DocDocument::empty();
    let heading = doc.new_block(DocBlockKind::Heading { level: 1 }, "目录");
    doc.blocks.push(heading);
    for _ in 0..55 {
        let block = doc.new_block(DocBlockKind::Text, "正文");
        doc.blocks.push(block);
    }
    let (_fixture, mut rt) = layout_test_runtime(doc, (1200., 800.));
    for outline in [false, true] {
        if outline {
            tap(&mut rt, "doc-outline-toggle");
        }
        rt.pointer_move(4., 4.);
        for _ in 0..80 { rt.frame(0.016); }
        let pane = rect(&rt, "canvas-pane");
        let thumb = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("thumb")).unwrap();
        let [x, y, w, h] = thumb.rect;
        assert!(thumb.opacity > 0.99 && h > 0.);
        assert_eq!(w, 3.);
        assert!((pane[0] + pane[2] - (x + w) - 2.).abs() < 1.);
        if outline {
            let rail = rect(&rt, "doc-outline-pane");
            assert!(rail[0] + rail[2] < x);
        }
        let start = rt.state.scroll_pos.get().offset[1];
        rt.pointer_down(x + 1., y + h * 0.5);
        rt.frame(0.016);
        rt.pointer_move(x + 1., y + h * 0.5 + 65.);
        rt.frame(0.016);
        rt.pointer_up(x + 1., y + h * 0.5 + 65.);
        rt.frame(0.016);
        assert!(rt.state.scroll_pos.get().offset[1] > start + 65.);
    }
    let (_short, short) = layout_test_runtime(DocDocument::empty(), (1200., 800.));
    assert_eq!(rect(&short, "scroll-bar")[3], 0.);
}

#[test]
fn document_drag_across_table_scrolls_and_copies_in_both_directions() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    doc.blocks.clear();
    for row in 0..45 {
        let block = doc.new_block(K::Text, &format!("正文 {row:02}"));
        doc.blocks.push(block);
    }
    let mut data = table_data::TableData::default();
    data.rows[0][0] = "表头 <预算>".into();
    data.rows[2][2] = "末格 128".into();
    let table = doc.new_block(K::Table(data), "");
    let id = table.id;
    let above = doc.blocks[21].id;
    let below = doc.blocks[22].id;
    doc.blocks.insert(22, table);
    for (canvas, upwards) in [(false, true), (false, false), (true, true), (true, false)] {
        let (_fixture, mut rt) = if canvas {
            let f = Fixture::new();
            let mut state = State::from_root(f.0.clone());
            state.create_canvas_note("工作", None);
            state.create_at(60., 110.);
            state.editor.active.core.doc = doc.clone();
            state.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(&mut state);
            (f, runtime(state, (1100., 736.)))
        } else {
            layout_test_runtime(doc.clone(), (1100., 736.))
        };
        for _ in 0..12 { rt.frame(1. / 60.); }
        rt.update(|s, tx| tx.scroll_to_instant(s.scroll_key(), 0., s.scroll_pos.get().max[1] * 0.46));
        for _ in 0..12 { rt.frame(1. / 60.); }
        let start = if upwards { below } else { above };
        let [x, y, _, h] = rect(&rt, &format!("doc-text-{start}"));
        let [_, top, _, height] = rt.state.editor.active.viewport_rect.unwrap();
        assert!((top..top + height).contains(&(y + h/2.)));
        let edge = if upwards { top + 2. } else { top + height - 2. };
        let original = rt.state.scroll_pos.get().offset[1];
        rt.pointer_down(x + 8., y + h/2.);
        rt.frame(0.016);
        rt.pointer_move(x + 8., edge);
        rt.frame(0.016);
        let cell_key = format!("doc-cell-box-{id}-1-1");
        let cell = rt.elements().into_iter().find(|e| e.key.as_ref() == Some(&cell_key)).unwrap();
        assert_eq!(cell.fill, crate::ui::c(crate::ui::accent_soft(&rt.state)));
        let first_head = rt.state.editor.active.core.sel.head;
        for _ in 0..48 { rt.frame(1. / 60.); }
        let moved = rt.state.scroll_pos.get().offset[1] - original;
        assert!(if upwards { moved < -180. } else { moved > 180. }, "canvas={canvas}, upwards={upwards}, moved={moved}");
        assert_eq!(rt.state.editor.active.core.sel.anchor.block, start);
        assert_ne!(rt.state.editor.active.core.sel.head.block, first_head.block);
        rt.pointer_up(x + 8., edge);
        rt.frame(0.016);
        let offset = rt.state.scroll_pos.get().offset;
        for _ in 0..6 { rt.frame(0.016); }
        assert_eq!(rt.state.scroll_pos.get().offset, offset);
        command(&mut rt, 'c', false);
        let write = rt.copy_request.take().expect("跨表选区响应 Cmd+C");
        let copied = write.content();
        let plain = copied.text();
        assert!(plain.find("表头 <预算>").unwrap() < plain.find("末格 128").unwrap());
        let (first, last) = rt.state.editor.active.core.ordered();
        let first_text = &rt.state.editor.active.core.text_of(first.block).unwrap().text;
        let last_text = &rt.state.editor.active.core.text_of(last.block).unwrap().text;
        assert!(plain.starts_with(&first_text[first.offset..]));
        assert!(plain.ends_with(&last_text[..last.offset]));
        let html = copied.html().unwrap();
        assert!(html.contains("<td>表头 &lt;预算&gt;</td>"));
        assert!(html.contains("<td>末格 128</td>"));
    }
}

#[test]
fn structured_layout_table_read_and_edit_keep_geometry_and_alignment() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    let table = doc.new_block(K::Table(table_data::TableData {
        rows: vec![vec!["左".into(), "居中".into(), "右".into()],
            vec!["短".into(), "长文本 English mixed 中文测试 ".repeat(8), "123.45".into()]],
        widths: vec![1.; 3], align: vec![0, 1, 2],
    }), "");
    let id = table.id;
    let after = doc.new_block(K::Text, "下方内容");
    let afterid = after.id;
    doc.blocks = vec![table, after];
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    let before = rect(&rt, &format!("doc-cell-box-{id}-1-1"));
    let below = rect(&rt, &format!("doc-text-{afterid}"));
    assert!(before[3] > 100., "read mode wraps long content");
    tap(&mut rt, &format!("doc-cell-box-{id}-1-1"));
    for _ in 0..30 { rt.frame(0.02); }
    assert_eq!(rect(&rt, &format!("doc-cell-box-{id}-1-1")), before);
    assert_eq!(rect(&rt, &format!("doc-text-{afterid}")), below);
    press(&mut rt, Key::Escape);
    tap(&mut rt, &format!("doc-cell-box-{id}-1-2"));
    let input = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("doc-cell-{id}-1-2"))).unwrap();
    let caret = input.caret.expect("focused numeric cell has caret");
    assert!(caret[0] > input.rect[0] + input.rect[2] - 5., "numeric input caret aligns to the right edge");
}

#[test]
fn document_outline_visibility_follows_each_document_and_survives_restart() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let a = state.create_doc_note("工作", None);
    let b = state.create_doc_note("工作", None);
    for id in [&a, &b] {
        state.select_note(id);
        let doc = &mut state.editor.active.core.doc;
        let h = doc.new_block(crate::doc_core::DocBlockKind::Heading { level: 2 }, "章节");
        doc.blocks.push(h);
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        state.finish_edit();
    }
    state.select_note(&a);
    let before = state.store.load_note(&a).unwrap();
    let mut rt = runtime(state, (1280., 943.));
    tap(&mut rt, "doc-outline-toggle");
    assert!(rt.state.document_outline_visible());
    assert_eq!(rt.state.store.load_note(&a).unwrap(), before, "outline is view state, not document content");
    rt.update(|s, _| s.select_note(&b));
    assert!(!rt.state.document_outline_visible(), "new documents do not inherit another document's outline");
    tap(&mut rt, "doc-outline-toggle");
    assert!(rt.state.document_outline_visible());
    rt.update(|s, _| s.select_note(&a));
    assert!(rt.state.document_outline_visible());
    tap(&mut rt, "doc-outline-toggle");
    assert!(!rt.state.document_outline_visible());
    drop(rt);
    let mut reopened = State::from_root(f.0.clone());
    reopened.select_note(&a);
    assert!(!reopened.document_outline_visible());
    reopened.select_note(&b);
    assert!(reopened.document_outline_visible());
    reopened.choose_mode(WorkspaceMode::Canvas);
    reopened.choose_mode(WorkspaceMode::Document);
    reopened.select_note(&b);
    assert!(reopened.document_outline_visible());
}

#[test]
fn structured_outline_pins_on_the_right_and_floats_in_narrow_windows() {
    use crate::doc_core::{DocBlockKind as K, DocDocument};
    let mut doc = DocDocument::empty();
    let heading = doc.new_block(K::Heading { level: 2 }, "目录项");
    let text = doc.new_block(K::Text, "内容不能被目录遮住。".repeat(50));
    let id = text.id;
    doc.blocks = vec![heading, text];
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    tap(&mut rt, "doc-outline-toggle");
    rt.frame(0.02);
    for (size, pinned) in [((1440., 943.), true), ((1000., 736.), false)] {
        rt.update(|s, _| s.pane_widths.documents = Some(420.));
        rt.set_viewport(size, 1.);
        for _ in 0..30 { rt.frame(0.02); }
        let outline = rect(&rt, "doc-outline-pane");
        let text = rect(&rt, &format!("doc-text-{id}"));
        let pane = rect(&rt, "canvas-pane");
        assert!((pane[0] + pane[2] - outline[0] - outline[2] - 12.).abs() < 1.);
        let viewport = rect(&rt, &rt.state.scroll_key());
        assert_eq!(viewport[0] + viewport[2], pane[0] + pane[2], "scrollbar stays at the outer edge");
        let column = rect(&rt, "doc-page-column");
        if pinned {
            assert!(text[0] + text[2] <= outline[0]);
        } else {
            assert_eq!(column[2], viewport[2], "floating outline preserves the writing column");
        }
        assert!(outline[0] + outline[2] <= size.0);
        assert!(rt.find("outline-item-0").is_some(), "active outline remains visible in narrow layouts");
        let more = rect(&rt, "doc-more");
        assert!(rect(&rt, "outline-item-0")[1] >= more[1] + more[3], "outline stays below document actions");
    }
}

#[test]
fn document_hit_geometry_refreshes_when_text_changes_without_resizing() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    state.new_note();
    let bid = state.editor.active.core.doc.blocks[0].id;
    state.editor.active.core.sel = crate::doc_core::DocSel::caret(crate::doc_core::DocPos::new(bid, 0));
    let mut rt = runtime(state, (1280., 943.));
    for _ in 0..5 { rt.frame(0.02); }
    type_text(&mut rt, "点击定位测试 abcdef");
    for _ in 0..5 { rt.frame(0.02); }
    let meta = rt.state.editor.active.text_layout(bid).unwrap();
    assert_eq!(meta.display_text, "点击定位测试 abcdef", "hit testing must use current text even when the box did not resize");
    let off = "点击定位测试 abc".len();
    let r = rune::text::doc_text::caret_rect(&meta.display_text, &meta.spans,
        meta.style, meta.max_width, meta.align, off);
    let (x, y) = (meta.rect[0] + r[0], meta.rect[1] + r[1] + r[3] / 2.);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.01);
    assert_eq!(rt.state.editor.active.core.sel.head, crate::doc_core::DocPos::new(bid, off));
}

#[test]
fn document_caret_blinks_without_rebuilding_and_stays_visible_during_ime() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.choose_mode(WorkspaceMode::Document);
    s.new_note();
    let mut rt = runtime(s, (1280., 943.));
    for _ in 0..10 { rt.frame(0.02); }
    let commits = rt.stats.commits;
    let mut shown = false;
    let mut hidden = false;
    for _ in 0..120 {
        rt.frame(1. / 60.);
        shown |= rt.state.editor.active.caret_opacity.get() == 1.;
        hidden |= rt.state.editor.active.caret_opacity.get() == 0.;
    }
    assert!(shown && hidden, "an idle insertion caret must blink");
    assert_eq!(rt.stats.commits, commits, "blink must not rebuild the document view");
    rt.update(|s, tx| { crate::doc_editor::on_ime(s, &rune::Ime::Preedit("nihao".into(), Some((5, 5))), tx); });
    for _ in 0..90 {
        rt.frame(1. / 60.);
        assert_eq!(rt.state.editor.active.caret_opacity.get(), 1., "composition caret stays visible");
    }
    rt.update(|s, tx| {
        s.editor.active.window_focused = false;
        tx.live_only();
    });
    for _ in 0..5 { rt.frame(0.02); }
    assert_eq!(rt.state.editor.active.caret_opacity.get(), 0.);
    assert!(!rt.live_pending());
}

#[test]
fn document_hit_geometry_tracks_scrolling_resize_and_virtual_rows_at_2x() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.choose_mode(WorkspaceMode::Document);
    s.new_note();
    let mut doc = crate::doc_core::DocDocument::empty();
    doc.blocks.clear();
    for i in 0..300 {
        let b = doc.new_block(crate::doc_core::DocBlockKind::Text, &format!("第{i}段 中文 English 123 {}", "换行测试".repeat(40)));
        doc.blocks.push(b);
    }
    s.note_mut().unwrap().doc = Some(doc.clone());
    s.editor.active.load(&s.current.clone(), doc, true);
    let mut rt = Runtime::new(s, crate::ui::view, (1280., 800.), Env::new(false, 2.));
    rt.poll = Some(crate::poll);
    rt.on_key = Some(crate::key);
    for _ in 0..5 { rt.frame(0.02); }
    for step in 0..35 {
        if step == 20 { rt.set_viewport((1100., 800.), 2.); }
        rt.wheel(800., 400., -60.);
        rt.frame(1. / 60.);
        let els = rt.elements();
        let e = els.iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("doc-text-") && k != "doc-text-0")
            && e.rect[1] > 100. && e.rect[1] + e.rect[3] < 750.).unwrap();
        let id: u64 = e.key.as_ref().unwrap().strip_prefix("doc-text-").unwrap().parse().unwrap();
        let meta = rt.state.editor.active.text_layout(id).unwrap();
        for (a, b) in meta.rect.into_iter().zip(e.rect) {
            assert!((a - b).abs() < 0.1, "frame {step}: cached geometry must follow drawn pixels: {:?} != {:?}", meta.rect, e.rect);
        }
        let off = meta.display_text.char_indices().nth(5).unwrap().0;
        let cr = rune::text::doc_text::caret_rect(&meta.display_text, &meta.spans,
            meta.style, meta.max_width, meta.align, off);
        let hit = rt.state.editor.active.hit_test_window(meta.rect[0] + cr[0], meta.rect[1] + cr[1] + cr[3] / 2.);
        assert_eq!(hit, crate::doc_core::DocPos::new(id, off), "scroll frame {step}");
        assert!(!rt.state.editor.active.is_below_last_block(790.), "a virtual viewport's bottom is not the document end");
    }
    assert!(rt.state.editor.active.text_layouts.borrow().len() < 100,
        "scrolling must release the geometry of unmounted paragraphs");
}

#[test]
fn live_only_layout_without_bindings_returns_to_idle() {
    let mut rt = Runtime::new((), |_, _| {
        rune::el().size(100., 100.).on_layout_tx(|_: &mut (), _, tx| tx.live_only())
    }, (200., 200.), Env::new(false, 1.));
    rt.check_live_only(false);
    for _ in 0..5 { rt.frame(1. / 60.); }
    assert!(rt.live_only_events() > 0);
    assert!(!rt.live_pending(), "layout bookkeeping must not keep the native display link awake");
    assert!(!rt.has_pending_input());
    assert!(!rt.animating);
}

#[test]
fn incremental_storage_preserves_other_notes_order_and_deletions() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.save(); // Persist startup normalization before observing editing writes.
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(f.0.join(".echo/index.json")).unwrap()).unwrap();
    let other = f.0.join(index["entries"][1]["path"].as_str().unwrap());
    let modified = std::fs::metadata(&other).unwrap().modified().unwrap();
    let mut note = s.notes[0].clone();
    note.title = "只修改这一篇".into();
    s.store.save_note(&note).unwrap();
    assert_eq!(std::fs::metadata(&other).unwrap().modified().unwrap(), modified);
    s.notes[0] = note;
    s.store.save(&s.notes, &s.current).unwrap();
    assert_eq!(std::fs::metadata(&other).unwrap().modified().unwrap(), modified,
        "saving unchanged notes must not rewrite them");
    s.notes.reverse();
    let removed = s.notes.pop().unwrap();
    s.store.save(&s.notes, &s.current).unwrap();
    let loaded = s.store.load().unwrap();
    assert_eq!(loaded.iter().map(|n| &n.id).collect::<Vec<_>>(), s.notes.iter().map(|n| &n.id).collect::<Vec<_>>());
    assert!(loaded.iter().all(|n| n.id != removed.id));
    assert!(s.store.save_note(&removed).is_err(), "an absent note must not be reported saved");
}

#[test]
fn file_library_transfer_preserves_order_and_preferences() {
    let source = Fixture::new();
    let mut s = State::from_root(source.0.clone());
    s.save();
    let target = Fixture(std::env::temp_dir().join(format!("echo-import-{}",uid())));
    let transfer = source.0.join("transfer.json");
    let notes: Vec<_> = s.notes.iter().rev().cloned().collect();
    std::fs::write(&transfer, serde_json::to_vec(&serde_json::json!({"notes": notes, "preferences": {"ui_language":"en"}})).unwrap()).unwrap();
    crate::storage::import_library(&transfer, &target.0).unwrap();
    let store = crate::storage::Store::open(target.0.clone()).unwrap();
    assert_eq!(store.load().unwrap(), notes);
    assert_eq!(store.preference("ui_language").as_deref(), Some("en"));
    assert!(!target.0.join("echo.sqlite3").exists());
}

#[test]
fn document_autosave_coalesces_flushes_and_keeps_errors_visible() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.choose_mode(WorkspaceMode::Document);
    s.new_note();
    let id = s.current.clone();
    let original = s.store.load().unwrap().into_iter().find(|n| n.id == id).unwrap();
    s.editor.active.core.insert_text("合并保存");
    crate::doc_editor::sync_to_state(&mut s);
    assert!(s.pending_document_save.is_some());
    assert_eq!(s.store.load().unwrap().into_iter().find(|n| n.id == id).unwrap(), original);
    let past = std::time::Instant::now() - std::time::Duration::from_secs(3);
    s.pending_document_save = Some((past, past));
    let mut rt = runtime(s, (1280., 800.));
    assert!(rt.state.pending_document_save.is_none());
    assert_eq!(rt.state.store.load().unwrap().into_iter().find(|n| n.id == id).unwrap().doc,
        rt.state.note().unwrap().doc);
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(f.0.join(".echo/index.json")).unwrap()).unwrap();
    let entry = index["entries"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap();
    let path = f.0.join(entry["path"].as_str().unwrap());
    let original_file = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"External editor change").unwrap();
    type_text(&mut rt, "错误可见");
    rt.state.pending_document_save = Some((past, past));
    rt.frame(0.01);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while rt.state.store.saving() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2)); rt.frame(0.01);
    }
    assert!(rt.state.document_save_failed);
    assert!(rt.state.pending_document_save.is_some());
    assert!(rt.state.document_save_deadline().is_none(), "failure must not create a redraw/retry loop");
    std::fs::write(&path, original_file).unwrap();
    rt.state.finish_edit();
    assert!(!rt.state.document_save_failed);
    assert!(rt.state.pending_document_save.is_none());
    type_text(&mut rt, "退出落盘");
    let expected = rt.state.note().unwrap().doc.clone();
    drop(rt);
    let restored = State::from_root(f.0.clone());
    assert_eq!(restored.notes.iter().find(|n| n.id == id).unwrap().doc, expected);
}

#[test]
fn million_character_document_virtualizes_reveals_and_settles() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.choose_mode(WorkspaceMode::Document);
    s.new_note();
    let mut doc = crate::doc_core::DocDocument::empty();
    doc.blocks.clear();
    for i in 0..5000 {
        let b = doc.new_block(crate::doc_core::DocBlockKind::Text, &format!("第{i:05}段{}", "测试".repeat(97)));
        doc.blocks.push(b);
    }
    let first = doc.blocks[0].id;
    let last = doc.blocks.last().unwrap().id;
    s.note_mut().unwrap().doc = Some(doc.clone());
    s.editor.active.load(&s.current.clone(), doc, true);
    s.save();
    let mut rt = runtime(s, (1280., 800.));
    for _ in 0..120 { rt.frame(1. / 60.); }
    assert!(rt.find(&format!("doc-blk-{first}")).is_some());
    assert!(rt.find(&format!("doc-blk-{last}")).is_none());
    assert!(rt.elements().len() < 500, "only nearby rows should exist in the element tree");
    assert!(!rt.animating, "a stationary long document must settle");
    assert!(!rt.live_pending(), "no live update may keep an idle document redrawing");
    rt.key(KeyEvent { key: Key::Down, mods: Mods { cmd: true, ..Default::default() } });
    for _ in 0..60 { rt.frame(1. / 60.); }
    assert_eq!(rt.state.editor.active.core.sel.head.block, last);
    assert!(rt.find(&format!("doc-blk-{last}")).is_some(), "Cmd+Down must mount the last row");
    type_text(&mut rt, "尾部输入");
    assert!(rt.state.editor.active.core.block(last).unwrap().text.text.ends_with("尾部输入"));
    rt.key(KeyEvent { key: Key::Up, mods: Mods { cmd: true, ..Default::default() } });
    for _ in 0..60 { rt.frame(1. / 60.); }
    assert!(rt.find("doc-title").is_some());
    assert!(rt.elements().len() < 500);
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!("noteflow-test-{}", uid())));
        let mut store = crate::storage::Store::open(f.0.clone()).unwrap();
        let notes = seed();
        store.save(&notes, &notes[0].id).unwrap();
        store.set_preference("book_order", &BOOKS[..6].join(",")).unwrap();
        f
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn runtime(state: State, size: (f32, f32)) -> Runtime<State> {
    let mut rt = Runtime::new(state, crate::ui::view, size, Env::new(false, 1.));
    rt.poll = Some(crate::poll);
    rt.on_key = Some(crate::key);
    rt.frame(0.);
    rt.frame(0.);
    rt
}
fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}; mode={:?} current={} cards={:?} visible={:?} message={:?}", rt.state.mode, rt.state.current, rt.state.note().map(|n| n.blocks.iter().map(|b| b.id.as_str()).collect::<Vec<_>>()), rt.state.canvas_visible, rt.state.message));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.01);
}
fn type_text(rt: &mut Runtime<State>, text: &str) {
    rt.key(KeyEvent {
        key: Key::Text(text.into()),
        mods: Mods::default(),
    });
    rt.frame(0.01);
}
fn press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(0.01);
}

#[test]
fn imported_image_remains_after_original_is_removed() {
    let f = Fixture::new();
    let source = f.0.with_extension("png");
    std::fs::write(&source, include_bytes!("../assets/architecture.png")).unwrap();
    let mut s = State::from_root(f.0.clone());
    s.import_image(&source);
    let rel = s.note().unwrap().blocks.last().unwrap().image.clone();
    std::fs::remove_file(source).unwrap();
    drop(s);
    let s = State::from_root(f.0.clone());
    assert_eq!(
        std::fs::read(s.store.root.join(rel)).unwrap(),
        include_bytes!("../assets/architecture.png")
    );
}
#[test]
fn undo_new_note_does_not_resurrect_it_on_relaunch() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let count = s.notes.len();
    s.new_note();
    s.undo(false);
    assert_eq!(s.notes.len(), count);
    drop(s);
    assert_eq!(State::from_root(f.0.clone()).notes.len(), count);
}
#[test]
fn trash_and_restore_preserve_canvas() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let original = s.note().unwrap().clone();
    s.trash_note();
    s.select_book("回收站");
    assert_eq!(s.note().unwrap().blocks, original.blocks);
    s.trash_note();
    s.select_book("工作");
    assert!(
        s.notes
            .iter()
            .any(|n| n.id == original.id && n.notebook == "工作")
    );
}


#[test]
fn single_click_blank_page_creates_text_at_pointer() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1420., 1046.));
    let count = rt.state.note().unwrap().blocks.len();
    // Click the space formerly covered by the centered tool strip.
    rt.pointer_down(1120., 990.);
    rt.pointer_up(1120., 990.);
    rt.frame(0.01);
    type_text(&mut rt, "空白处直接记录");
    assert_eq!(rt.state.note().unwrap().blocks.len(), count + 1);
    let b = rt.state.note().unwrap().blocks.last().unwrap();
    assert_eq!(embedded_text(&rt.state, &b.id), "空白处直接记录");
    assert!((b.x - (1120. - rect(&rt, "canvas")[0] - crate::doc_editor::CARD_TEXT_INSET)).abs() < 1.);
}
#[test]
fn canvas_blank_click_places_only_a_caret_until_text_is_committed() {
    for theme in [Theme::Light, Theme::Dark] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.create_canvas_note("工作", None);
        state.sidebar_material.mode = if theme.is_dark() { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
        let mut rt = runtime(state, (1280., 943.));
        let mut last_draft: Option<String> = None;
        for (x, y) in [(700., 380.), (746., 395.)] {
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.05);
            let draft = rt.state.draft.clone().unwrap();
            assert_eq!(rt.state.note().unwrap().blocks.len(), 1);
            if let Some(old) = last_draft { assert_ne!(draft, old); assert!(!rt.state.editor.card(&old).is_some()); }
            last_draft = Some(draft.clone());
            let caret = rect(&rt, "doc-caret");
            assert!((caret[0]-x).abs() < 1. && (caret[1]+caret[3]/2.-y).abs() < 5., "{caret:?}");
            assert!(rt.find("container-chrome").is_none());
            assert!(rt.find(&format!("move-frame-{draft}")).is_none());
            let id = rt.state.editor.active.core.sel.head.block;
            assert!(rt.state.editor.active.text_layout(id).unwrap().display_text.is_empty());
        }
        rt.update(|s,tx| { assert!(crate::doc_editor::on_ime(s,&rune::Ime::Preedit("ni".into(),Some((2,2))),tx)); });
        assert!(rt.state.draft.is_some());
        rt.update(|s,tx| { assert!(crate::doc_editor::on_ime(s,&rune::Ime::Commit("你".into()),tx)); });
        rt.frame(0.05);
        assert!(rt.state.draft.is_none());
        let id = last_draft.unwrap();
        assert_eq!(embedded_text(&rt.state, &id), "你");
        assert!(rt.find("container-chrome").is_some());
        assert!(rt.find(&format!("move-handle-{id}")).is_none());
        assert!(rect(&rt, &format!("block-{id}"))[3] <= 46.);
        rt.state.finish_edit();
        assert_eq!(embedded_text(&State::from_root(f.0.clone()), &id), "你");
    }
}

#[test]
fn hovered_canvas_text_keeps_its_full_outline_while_focused() {
    let fixture = Fixture::new();
    let mut state = State::from_root(fixture.0.clone());
    state.create_canvas_note("工作", None);
    let card = state.create_at(150., 200.);
    let mut rt = runtime(state, (1280., 943.));
    type_text(&mut rt, "编辑正文");
    let bounds = rect(&rt, &format!("block-{card}"));
    let [x, y, w, h] = bounds;
    for (px, py) in [
        (x - 50., y), (x + 1., y + h / 2.), (x + w / 2., y + h / 2.),
        (x + w + 40., y), (x + w / 2., y + 10.),
    ] {
        rt.pointer_move(px, py);
        rt.frame(0.3);
        assert!(rt.state.editor.active.focused);
        let full_borders = rt.instances.iter().filter(|quad| quad.params[0] > 0.
            && quad.border[3] > 0. && quad.rect.iter().zip(bounds).all(|(a,b)| (a-b).abs()<1.)).count();
        assert!(full_borders >= 2, "paper edge and focus outline at ({px}, {py})");
    }
    rt.update(|s,tx| { s.finish_edit(); s.selected=None; tx.instant(); });
    rt.pointer_move(x - 50., y);
    rt.frame(0.3);
    assert!(rt.find("container-chrome").is_none());
    assert!(rt.instances.iter().any(|quad| quad.params[0] > 0. && quad.border[3] > 0.
        && quad.rect.iter().zip(bounds).all(|(a,b)| (a-b).abs()<1.)), "idle card keeps its paper edge");

}

#[test]
fn canvas_list_handle_leaves_with_pointer_and_typing_stays_on_active_row() {
    use crate::doc_core::{DocBlockKind, DocPos};
    for zoom in [0.65, 1., 1.4] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        state.create_canvas_note("工作", None);
        state.note_mut().unwrap().zoom = zoom;
        let card = state.create_at(100., 150.);
        let core = &mut state.editor.active.core;
        core.doc.blocks = ["112", "12445", "123234", ""].into_iter()
            .map(|text| core.doc.new_block(DocBlockKind::Bullet, text)).collect();
        let third = core.doc.blocks[2].id;
        let fourth = core.doc.blocks[3].id;
        let pos = DocPos::new(third, "123234".len());
        core.set_head(pos, false);
        core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        let saved = rt.state.editor.active.core.doc.clone();
        let (x, y) = rt.find(&format!("doc-text-{fourth}")).unwrap();
        let handle = format!("doc-handle-{fourth}");
        let [cx, cy, cw, ch] = rect(&rt, &format!("block-{card}"));
        for outside in [(cx + cw + 40., y), (cx - 40., y), (x, cy + ch + 40.), (x, cy - 40.)] {
            rt.pointer_move(x, y);
            rt.frame(0.05);
            assert!(rt.find(&handle).is_some());
            rt.pointer_move(outside.0, outside.1);
            rt.frame(0.3);
            assert!(rt.find(&handle).is_none(), "outside={outside:?}, zoom={zoom}");
            assert_eq!(rt.state.editor.active.hover_block, None);
            assert_eq!(rt.state.editor.active.core.sel.head, pos);
            assert_eq!(rt.state.editor.active.core.doc, saved);
            assert!(rt.find("doc-caret").is_some());
        }
        rt.pointer_move(x, y);
        rt.frame(0.05);
        assert!(rt.find(&handle).is_some());
        rt.pointer_leave();
        rt.frame(0.3);
        assert!(rt.find(&handle).is_none());
        assert_eq!(rt.state.editor.active.core.sel.head, pos);
        assert!(rt.state.editor.active.text_layout(fourth).unwrap().display_text.is_empty());
        type_text(&mut rt, "7");
        assert_eq!(rt.state.editor.active.core.block(third).unwrap().text.text, "1232347");
        tap(&mut rt, &format!("doc-marker-{fourth}"));
        assert_eq!(rt.state.editor.active.core.sel.head, DocPos::new(fourth, 0));
        assert!(!rt.state.editor.active.text_layout(fourth).unwrap().display_text.is_empty());
    }
}

#[test]
fn blank_clicks_do_not_persist_or_change_note_history() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let before = s.note().unwrap().clone();
    let mut previous = None;
    for (x, y) in [(100., 1100.), (300., 900.), (500., 1200.)] {
        let id = s.create_at(x, y);
        assert_eq!(s.note().unwrap().blocks.len(), before.blocks.len() + 1);
        if let Some(old) = previous {
            assert!(s.note().unwrap().blocks.iter().all(|b| b.id != old));
        }
        assert_eq!(s.note().unwrap().updated, before.updated);
        assert!(s.undo.is_empty());
        // Any save path (zoom, scroll, Cmd+S, close) must exclude the draft.
        s.save();
        let persisted = s.store.load().unwrap();
        assert_eq!(
            persisted.iter().find(|n| n.id == before.id).unwrap(),
            &before
        );
        previous = Some(id);
    }
    s.finish_edit();
    assert_eq!(s.note().unwrap(), &before);
    assert!(s.draft.is_none());
    assert_eq!(State::from_root(f.0.clone()).note().unwrap(), &before);
}
#[test]
fn empty_input_position_disappears_when_focus_leaves() {
    for action in 0..6 {
        let (f, mut rt, id) = fresh_body();
        let key = format!("body-{id}");
        assert!(rt.find(&key).is_some());
        match action {
            0 => tap(&mut rt, "open-search"),
            1 => tap(&mut rt, "body-code"),
            2 => press(&mut rt, Key::Escape),
            3 => tap(&mut rt, "note-sample-1"),
            4 => command(&mut rt, ',', false),
            _ => command(&mut rt, 'k', false),
        }
        rt.frame(0.01);
        rt.frame(0.01);
        assert!(rt.state.draft.is_none(), "action {action}");
        assert!(crate::canvas_document::session(&rt.state, &id).is_none(), "action {action}");
        assert!(rt.find(&key).is_none(), "action {action}");
        assert!(
            rt.state
                .store
                .load()
                .unwrap()
                .iter()
                .all(|n| { n.blocks.iter().all(|b| b.id != id) })
        );
        drop(f);
    }
}
#[test]
fn whitespace_draft_has_no_resize_handles_and_is_not_saved() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.create_canvas_note("工作", None);
    let id = s.create_at(80., 140.);
    let mut rt = runtime(s, (1420., 1046.));
    let before = rt.state.store.load().unwrap();
    let undo_before = rt.state.undo.len();
    type_text(&mut rt, "    ");
    let (x, y) = rt.find(&format!("body-{id}")).unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.01);
    assert!(rt.find(&format!("size-handle-{id}")).is_none());
    assert_eq!(rt.state.draft.as_deref(), Some(id.as_str()));
    assert_eq!(rt.state.undo.len(), undo_before);
    assert_eq!(rt.state.store.load().unwrap(), before);
    tap(&mut rt, "open-search");
    rt.frame(0.01);
    assert!(rt.state.block_mut(&id).is_none());
    assert_eq!(State::from_root(f.0.clone()).notes, before);
}
#[test]
fn click_lower_interior_after_moving_and_resizing_keeps_same_editor() {
    let (_f, mut rt, id) = fresh_body();
    rt.update(|s, _| s.note_mut().unwrap().blocks.retain(|b| b.id == id));
    rt.frame(0.01);
    type_text(&mut rt, "已有内容");
    let (x, y) = rt.find(&format!("body-{id}")).unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.01);
    drag_key(&mut rt, &format!("move-handle-{id}"), 90., 72.);
    let initial_height = rect(&rt, &format!("block-{id}"))[3];
    drag_key(&mut rt, &format!("size-handle-{id}"), 60., 90.);
    let [x, y, w, h] = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(&format!("block-{id}")))
        .unwrap()
        .rect;
    assert!((h - initial_height - 90.).abs() < 1.);
    let (x, y) = (x + w - 30., y + h - 18.);
    rt.pointer_down(x, y);
    rt.frame(0.1);
    rt.pointer_up(x, y);
    rt.frame(0.1);
    assert!(rt.state.draft.is_none());
    type_text(&mut rt, "移动后继续写");
    assert_eq!(embedded_text(&rt.state, &id), "已有内容\n移动后继续写");
}
#[test]
fn clearing_saved_card_keeps_its_identity_and_can_be_undone() {
    let (f, mut rt, id) = fresh_body();
    type_text(&mut rt, "一行文字");
    command(&mut rt, 'a', false);
    press(&mut rt, Key::Backspace);
    assert!(rt.state.draft.is_none());
    assert_eq!(embedded_text(&rt.state, &id), "");
    tap(&mut rt, &format!("body-{id}"));
    command(&mut rt, 'z', false);
    assert_eq!(embedded_text(&rt.state, &id), "一行文字");
    command(&mut rt, 'z', true);
    tap(&mut rt, "open-search");
    rt.frame(0.01);
    assert!(rt.state.note().unwrap().blocks.iter().any(|b| b.id == id));
    let reopened = State::from_root(f.0.clone());
    assert_eq!(embedded_text(&reopened, &id), "");
}
#[test]
fn first_content_commits_at_latest_click_and_undo_has_no_empty_block() {
    let (f, mut rt, first) = fresh_body();
    let count = rt.state.note().unwrap().blocks.len() - 1;
    rt.pointer_down(1120., 990.);
    rt.pointer_up(1120., 990.);
    rt.frame(0.01);
    rt.frame(0.01);
    let id = rt.state.draft.clone().unwrap();
    assert_ne!(id, first);
    assert!(crate::canvas_document::session(&rt.state, &first).is_none());
    type_text(&mut rt, "只保存这里的内容");
    assert!(rt.state.draft.is_none());
    assert_eq!(rt.state.undo.len(), 1);
    rt.state.finish_edit(); // explicit durable boundary
    let restored = State::from_root(f.0.clone());
    let b = restored.note().unwrap().blocks.last().unwrap();
    assert_eq!(b.id, id);
    assert_eq!(embedded_text(&restored, &b.id), "只保存这里的内容");
    assert!((b.x - (1120. - rect(&rt, "canvas")[0] - crate::doc_editor::CARD_TEXT_INSET)).abs() < 1.);
    rt.state.undo(false);
    assert_eq!(rt.state.note().unwrap().blocks.len(), count);
    rt.state.undo(true);
    assert_eq!(rt.state.note().unwrap().blocks.len(), count + 1);
    assert_eq!(embedded_text(&rt.state, &id), "只保存这里的内容");
}
#[test]
fn reopening_preserves_existing_canvas_blocks_without_implicit_cleanup() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let empty = Block::new(Kind::Text, 100., 500., 360., 40., "", "");
    s.mount_editor(&empty);
    s.note_mut().unwrap().blocks.push(empty);
    let before = s.note().unwrap().blocks.clone();
    s.save();
    drop(s);
    let s = State::from_root(f.0.clone());
    assert!(!f.0.join("Backups/before-empty-cleanup").exists());
    assert_eq!(s.note().unwrap().blocks, before);
}
#[test]
fn long_text_auto_grows_container_and_page() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.block_mut("intro").unwrap().min_h = 120.;
    let mut rt = runtime(state, (1420., 1046.));
    tap(&mut rt, "body-intro");
    for _ in 0..70 { press(&mut rt, Key::Enter); type_text(&mut rt, "长文会自动增高，不应被裁切。"); }
    rt.frame(0.01);
    rt.frame(0.01);
    let b = &rt.state.note().unwrap().blocks[0];
    assert!(b.h > 1800., "height {}", b.h);
    assert!(rt.state.scroll_pos.get().max[1] > 1500.);
    assert!(rt.state.scroll_pos.get().offset[1] > 0.);
    let els = rt.elements();
    let caret = els.iter().find(|e| e.key.as_deref() == Some("doc-caret")).unwrap().rect;
    let viewport = els
        .iter()
        .find(|e| e.key.as_deref() == Some(rt.state.scroll_key().as_str()))
        .unwrap()
        .rect;
    assert!(
        caret[1] + caret[3] <= viewport[1] + viewport[3] + 1.,
        "caret {caret:?}, viewport {viewport:?}"
    );
    assert!(
        rt.state
            .note()
            .unwrap()
            .blocks
            .iter()
            .find(|b| b.id == "photo")
            .unwrap()
            .y
            >= b.y + b.h
    );
}
#[test]
fn scroll_is_bounded_and_zoom_control_stays_fixed() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.block_mut("handwritten").unwrap().y = 1300.;
    let mut rt = runtime(s, (1420., 1046.));
    let control = rt.find("zoom-control").unwrap();
    rt.pointer_move(4., 4.);
    for _ in 0..65 { rt.frame(0.016); }
    let thumb = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("thumb")).unwrap();
    assert_eq!(thumb.rect[2], 3.);
    assert!(thumb.opacity > 0.99);
    rt.wheel(900., 600., -100000.);
    rt.frame(0.01);
    let at = rt.state.scroll_pos.get();
    assert!(at.offset[1] > 0.);
    assert!((at.offset[1] - at.max[1]).abs() < 0.1);
    assert_eq!(control, rt.find("zoom-control").unwrap());
    rt.wheel(900., 600., 100000.);
    rt.frame(0.01);
    assert_eq!(rt.state.scroll_pos.get().offset[1], 0.);
    let (thumb_x, thumb_y) = rt.find("thumb").unwrap();
    rt.pointer_move(thumb_x, thumb_y);
    rt.frame(0.05);
    rt.pointer_down(thumb_x, thumb_y);
    rt.pointer_move(thumb_x, thumb_y + 160.);
    rt.pointer_up(thumb_x, thumb_y + 160.);
    rt.frame(0.05);
    assert!(rt.state.scroll_pos.get().offset[1] > 100.);
    assert_eq!(control, rt.find("zoom-control").unwrap());
}
#[test]
fn move_handle_respects_zoom_and_width_handle_reflows() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.note_mut().unwrap().zoom = 1.5;
    s.note_mut().unwrap().blocks.retain(|b| b.id == "intro");
    let mut rt = runtime(s, (1600., 1046.));
    tap(&mut rt, "body-intro");
    let b = rt.state.note().unwrap().blocks[0].clone();
    let (x, y) = rt.find("move-handle-intro").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.01);
    assert!(
        rt.find("move-handle-intro").is_some(),
        "handle stays mounted when approached"
    );
    rt.pointer_down(x, y);
    rt.frame(0.01);
    rt.pointer_move(x + 60., y + 30.);
    rt.frame(0.01);
    rt.pointer_up(x + 60., y + 30.);
    rt.frame(0.01);
    let after = &rt.state.note().unwrap().blocks[0];
    assert!((after.x - b.x - 40.).abs() < 0.1);
    assert!((after.y - b.y - 20.).abs() < 0.1);
    let before = after.clone();
    let (x, y) = rt.find("width-handle-intro").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.01);
    rt.pointer_down(x, y);
    rt.pointer_move(x + 60., y);
    rt.pointer_up(x + 60., y);
    rt.frame(0.01);
    let after = &rt.state.note().unwrap().blocks[0];
    assert!((after.w - before.w - 40.).abs() < 0.1);
    assert_eq!(after.body, before.body);
}
#[test]
fn ime_composition_is_saved_only_after_commit() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.new_note();
    let mut rt = runtime(s, (1420., 1046.));
    rt.ime(rune::Ime::Preedit("zhongwen".into(), Some((8, 8))));
    rt.frame(0.01);
    assert_eq!(rt.state.note().unwrap().title, "未命名笔记");
    rt.ime(rune::Ime::Commit("中文输入法".into()));
    rt.frame(0.01);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.note().unwrap().title, "中文输入法");
}



#[test]
fn page_scroll_restores_when_switching_notes() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.block_mut("handwritten").unwrap().y = 1300.;
    let mut rt = runtime(s, (1420., 1046.));
    rt.wheel(900., 600., -140.);
    rt.frame(0.01);
    let offset = rt.state.scroll_pos.get().offset[1];
    assert!(offset > 0.);
    tap(&mut rt, "note-sample-1");
    rt.frame(0.01);
    assert_eq!(rt.state.scroll_pos.get().offset[1], 0.);
    tap(&mut rt, "note-sample-0");
    rt.frame(0.01);
    assert!((rt.state.scroll_pos.get().offset[1] - offset).abs() < 1.);
}
#[test]
fn merging_text_containers_keeps_both_contents_and_can_be_undone() {
    let f=Fixture::new();
    let mut s=State::from_root(f.0.clone());
    s.create_canvas_note("工作",None);
    let first=s.create_at(40.,150.);
    s.editor.active.core.insert_text("第一块");
    crate::doc_editor::sync_to_state(&mut s);
    let second=s.create_at(440.,150.);
    s.editor.active.core.insert_text("合并的内容");
    crate::doc_editor::sync_to_state(&mut s);
    s.merge_containers(&second,&first);
    assert_eq!(s.note().unwrap().blocks.len(),1);
    assert!(embedded_text(&s,&first).contains("第一块"));
    assert!(embedded_text(&s,&first).contains("合并的内容"));
    s.undo(false);
    assert_eq!(s.note().unwrap().blocks.len(),2);
    assert_eq!(embedded_text(&s,&second),"合并的内容");
}

fn command(rt: &mut Runtime<State>, ch: char, shift: bool) {
    rt.key(KeyEvent {
        key: Key::Command(ch),
        mods: Mods {
            cmd: true,
            shift,
            ..Default::default()
        },
    });
    rt.frame(0.01);
    rt.frame(0.01);
}
fn fresh_body() -> (Fixture, Runtime<State>, String) {
    body_at_zoom(1.)
}
fn body_at_zoom(zoom: f32) -> (Fixture, Runtime<State>, String) {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.note_mut().unwrap().zoom = zoom;
    let id = s.create_at(48., 150.);
    s.edit(&id);
    let mut rt = runtime(s, (1420., 1046.));
    rt.frame(0.01);
    (f, rt, id)
}












fn drag_key(rt: &mut Runtime<State>, key: &str, dx: f32, dy: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_move(x, y);
    rt.frame(0.01);
    assert!(
        rt.find(key).is_some(),
        "control remains under the pointer: {key}"
    );
    rt.pointer_down(x, y);
    rt.frame(0.01);
    rt.pointer_move(x + dx, y + dy);
    rt.frame(0.01);
    rt.pointer_up(x + dx, y + dy);
    rt.frame(0.01);
    rt.frame(0.01);
}
#[test]
fn all_border_edges_and_corners_move_without_resizing() {
    for zoom in [1., 1.5] {
        for at in 0..8 {
            let (_f, mut rt, id) = body_at_zoom(zoom);
            type_text(&mut rt, "移动容器");
            let before = rt
                .state
                .note()
                .unwrap()
                .blocks
                .iter()
                .find(|b| b.id == id)
                .unwrap()
                .clone();
            let el = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(format!("block-{id}").as_str()))
                .unwrap();
            let [x, y, w, h] = el.rect;
            let (px, py) = match at {
                0 => (x + w / 2., y),
                1 => (x + w, y + h / 2.),
                2 => (x + w / 2., y + h),
                3 => (x, y + h / 2.),
                4 => (x + 3. * zoom, y + 3. * zoom),
                5 => (x + w - 3. * zoom, y + 3. * zoom),
                6 => (x + 3. * zoom, y + h - 3. * zoom),
                _ => (x + w - 12. * zoom, y + h - 3. * zoom),
            };
            rt.pointer_move(px, py);
            rt.frame(0.01);
            rt.pointer_down(px, py);
            rt.frame(0.01);
            rt.pointer_move(px + 30., py + 18.);
            rt.frame(0.01);
            rt.pointer_up(px + 30., py + 18.);
            rt.frame(0.01);
            let after = rt
                .state
                .note()
                .unwrap()
                .blocks
                .iter()
                .find(|b| b.id == id)
                .unwrap();
            assert!(
                (after.x - before.x - 30. / zoom).abs() < 0.1,
                "edge {at}, zoom {zoom}, selected {:?}: {after:?}",
                rt.state.selected
            );
            assert!((after.y - before.y - 18. / zoom).abs() < 0.1);
            assert_eq!(
                (after.w, after.h, after.min_h),
                (before.w, before.h, before.min_h)
            );
        }
    }
}
#[test]
fn width_height_and_corner_controls_resize_persist_and_undo() {
    let (f, mut rt, id) = body_at_zoom(1.5);
    rt.update(|s, _| s.note_mut().unwrap().blocks.retain(|b| b.id == id));
    rt.frame(0.01);
    type_text(&mut rt, "尺寸调整后仍能编辑中文");
    tap(&mut rt, &format!("body-{id}"));
    let initial = rt
        .state
        .note()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.id == id)
        .unwrap()
        .clone();
    drag_key(&mut rt, &format!("size-handle-{id}"), 90., 120.);
    let sized = rt
        .state
        .note()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.id == id)
        .unwrap()
        .clone();
    assert!((sized.w - initial.w - 60.).abs() < 0.1);
    assert!((sized.h - initial.h - 80.).abs() < 0.1);
    assert!((sized.min_h - sized.h).abs() < 0.1);
    drag_key(&mut rt, &format!("height-handle-{id}"), 0., 60.);
    let taller = rt
        .state
        .note()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.id == id)
        .unwrap()
        .clone();
    assert!((taller.h - sized.h - 40.).abs() < 0.1);
    assert_eq!(taller.w, sized.w);
    rt.state.undo(false);
    rt.frame(0.01);
    rt.frame(0.01);
    assert!(
        (rt.state
            .note()
            .unwrap()
            .blocks
            .iter()
            .find(|b| b.id == id)
            .unwrap()
            .h
            - sized.h)
            .abs()
            < 0.1
    );
    rt.state.undo(true);
    rt.frame(0.01);
    rt.frame(0.01);
    rt.state.save();
    drop(rt);
    let restored = State::from_root(f.0.clone());
    let b = restored
        .note()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.id == id)
        .unwrap();
    assert!((b.w - taller.w).abs() < 0.1);
    assert!((b.min_h - taller.min_h).abs() < 0.1);
    assert_eq!(embedded_text(&restored, &id), "尺寸调整后仍能编辑中文");
}

#[test]
fn image_corner_keeps_aspect_and_edge_controls_adjust_dimensions_independently() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.note_mut().unwrap().blocks.retain(|b| b.id == "photo");
    let mut rt = runtime(s, (1420., 1046.));
    tap(&mut rt, "block-photo");
    let before = rt.state.note().unwrap().blocks[0].clone();
    drag_key(&mut rt, "size-handle-photo", 68., 80.);
    let sized = rt.state.note().unwrap().blocks[0].clone();
    assert!((sized.w - before.w - 68.).abs() < 0.1);
    assert!((sized.w / sized.h - before.w / before.h).abs() < 0.001);
    drag_key(&mut rt, "height-handle-photo", 0., 50.);
    let taller = rt.state.note().unwrap().blocks[0].clone();
    assert!((taller.h - sized.h - 50.).abs() < 0.1);
    assert_eq!(taller.w, sized.w);
    drag_key(&mut rt, "width-handle-photo", -40., 0.);
    let narrower = &rt.state.note().unwrap().blocks[0];
    assert!((narrower.w - taller.w + 40.).abs() < 0.1);
    assert_eq!(narrower.h, taller.h);
}





#[test]
fn canvas_slash_mouse_press_survives_intermediate_frames() {
    for (query, command, expected) in [
        ("/", "todo", rune::Block::Task(0, false)),
        ("/todo", "todo", rune::Block::Task(0, false)),
        ("/", "h2", rune::Block::Heading(2)),
        ("、", "code", rune::Block::Code),
    ] {
      for zoom in [0.75, 1., 1.4] {
        let (_f, mut rt, id) = body_at_zoom(zoom);
        for ch in query.chars() { type_text(&mut rt, &ch.to_string()); }
        rt.frame(0.5);
        let item = format!("slash-item-{command}");
        let (x, y) = rt.find(&item).unwrap_or_else(|| panic!("missing {item} query={query} zoom={zoom} focused={} slash={:?} text={:?} message={:?} owner={:?}", rt.state.editor.active.focused, rt.state.editor.active.slash, rt.state.editor.active.core.doc.to_plain_text(), rt.state.message, rt.state.editor.canvas_owner()));
        rt.pointer_move(x, y); rt.frame(0.1);
        rt.pointer_down(x, y); rt.frame(0.1);
        assert!(rt.find(&item).is_some(), "menu must survive mouse down: {query}/{command}/{zoom}");
        rt.pointer_up(x, y); rt.frame(0.5);
        assert_eq!(embedded_active_kind(&rt.state, &id), expected);
        assert!(embedded_text(&rt.state, &id).is_empty());
        type_text(&mut rt, "点击后继续写");
        assert_eq!(embedded_text(&rt.state, &id), "点击后继续写");
      }
    }
}




#[test]
fn slash_menu_fits_window_at_zoom_and_bottom_edge() {
    for zoom in [0.8, 1.5] {
        let f = Fixture::new();
        let mut s = State::from_root(f.0.clone());
        s.note_mut().unwrap().zoom = zoom;
        let id = s.create_at(790. / zoom, 820. / zoom + 72.);
        let mut rt = runtime(s, (1420., 1046.));
        rt.set_system_reduced_motion(zoom < 1.);
        rt.frame(0.);
        type_text(&mut rt, "/");
        rt.frame(1.);
        let [x, y, w, h] = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("doc-slash-menu"))
            .expect("anchored menu")
            .rect;
        assert!(x >= 532. && x + w <= 1421., "zoom {zoom}: {x} {w}");
        assert!(y >= 0. && y + h <= 1047., "zoom {zoom}: {y} {h}");
        assert!(rt.state.editor.active.focused);
        press(&mut rt, Key::Down);
        press(&mut rt, Key::Enter);
        assert_eq!(
            embedded_active_kind(&rt.state, &id),
            rune::Block::Heading(1)
        );
    }
}

#[test]
fn slash_image_cancel_and_accept_stay_inside_owner() {
    let (f, mut rt, id) = fresh_body();
    let count = rt.state.note().unwrap().blocks.len();
    type_text(&mut rt, "/"); type_text(&mut rt, "tp");
    press(&mut rt, Key::Enter);
    assert!(rt.file_dialog().is_some());
    rt.resolve_file_dialog(None).unwrap();
    rt.frame(0.1);
    assert_eq!(embedded_text(&rt.state, &id), "");
    assert!(rt.state.editor.active.focused);
    type_text(&mut rt, "/");
    type_text(&mut rt, "image");
    press(&mut rt, Key::Enter);
    let source = f.0.join("source.png");
    std::fs::write(&source, include_bytes!("../assets/architecture.png")).unwrap();
    rt.resolve_file_dialog(Some(source)).unwrap();
    rt.frame(0.2);
    assert_eq!(rt.state.note().unwrap().blocks.len(), count);
    let core = &rt.state.editor.active.core;
    let image = core.doc.blocks.iter().find_map(|b| match &b.kind {
        crate::doc_core::DocBlockKind::Image { src, .. } => Some(src), _ => None
    }).expect("image inside the original card");
    assert!(f.0.join(image).exists());
    rt.state.finish_edit(); // explicit durable boundary
    let reopened = State::from_root(f.0.clone());
    assert!(crate::canvas_document::session(&reopened, &id).unwrap().core.doc.blocks.iter()
        .any(|b| matches!(b.kind, crate::doc_core::DocBlockKind::Image { .. })));
}












fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing {key}"))
        .rect
}
fn assert_navigation_selected(rt: &Runtime<State>, key: &str) {
    let elements = rt.elements();
    let e = elements.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap();
    let expected = crate::ui::nav_selection(&rt.state);
    assert_eq!(e.fill, expected, "selected row {key}");
}
#[test]
fn navigation_backplates_switch_immediately_even_after_edit_focus() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.edit("intro");
    let mut rt = runtime(s, (1420., 1046.));
    rt.frame(1.);
    for target in ["note-sample-4", "book-3", "book-5"] {
        let (x, y) = rt.find(target).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.001);
        assert_navigation_selected(&rt, target);
    }
}

#[test]
fn selection_switches_immediately_and_stays_attached_when_list_scrolls() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let template = s.notes[0].clone();
    for index in 0..20 {
        let mut n = template.clone();
        n.id = format!("extra-{index}");
        n.title = format!("笔记 {index}");
        s.notes.push(n);
    }
    let mut rt = runtime(s, (1420., 736.));
    rt.frame(1.);
    let (x, y) = rt.find("note-sample-4").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    let (x, y) = rt.find("note-sample-1").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.001);
    assert_navigation_selected(&rt, "note-sample-1");
    rt.frame(0.8);
    assert_navigation_selected(&rt, "note-sample-1");
    let [x, y, w, h] = rect(&rt, "notes-scroll");
    rt.wheel(x + w / 2., y + h / 2., -100.);
    rt.frame(0.02);
    assert_eq!(rt.state.current, "sample-1");

}






#[test]
#[ignore = "manual navigation timing on an isolated data fixture"]
fn profile_navigation_switch_cpu() {
    let f = Fixture::new();
    let mut s = if let Ok(root) = std::env::var("NOTEFLOW_BENCH_ROOT") {
        State::from_root(PathBuf::from(root))
    } else {
        State::from_root(f.0.clone())
    };
    let ids = s
        .notes
        .iter()
        .filter(|n| n.notebook != "回收站")
        .take(6)
        .map(|n| n.id.clone())
        .collect::<Vec<_>>();
    let mut samples = vec![];
    for _ in 0..8 {
        for id in &ids {
            let start = std::time::Instant::now();
            s.select_note(id);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "NAV_CPU_MS count={} median={:.3} p95={:.3} max={:.3}",
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100],
        samples[samples.len() - 1]
    );
}
#[test]
fn navigation_motion_settles_without_an_idle_redraw_loop() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1420., 1046.));
    rt.frame(1.);
    tap(&mut rt, "note-sample-4");
    rt.pointer_move(150., 50.);
    rt.frame(1.);
    rt.frame(1.);
    assert!(
        !rt.animating,
        "a settled navigation marker must not keep drawing"
    );
    tap(&mut rt, "book-3");
    rt.pointer_move(150., 50.);
    rt.frame(1.);
    rt.frame(1.);
    assert!(!rt.animating, "an empty notebook must also become idle");
}

#[test]
fn unified_appearance_ignores_old_themes_and_preserves_notes() {
    let f = Fixture::new();
    let state = State::from_root(f.0.clone());
    state.store.set_preference("theme", "sky").unwrap();
    state.store.set_preference("sidebar_material", "{\"transparency\":83,\"grain\":66}").unwrap();
    drop(state);
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    let before = rt.state.notes.clone();
    assert_eq!(rt.state.sidebar_material.grain, 0);
    assert_eq!(crate::ui::editor_colors(rt.state.theme()).page, 0xffffff);
    tap(&mut rt, "settings");
    for key in ["theme-mist", "theme-sage", "theme-sky"] { assert!(rt.find(key).is_none()); }
    assert!(rt.find("mode-thumbnail-canvas").is_none());
    assert!(rt.find("mode-thumbnail-document").is_none());
    tap(&mut rt, "settings-tab-shortcuts");
    assert!(rt.find("settings-shortcuts-scroll").is_some());
    tap(&mut rt, "settings-tab-data");
    assert!(rt.find("export-markdown").is_none());
    assert!(rt.find("open-data-folder").is_some());
    tap(&mut rt, "settings-tab-trash");
    assert!(rt.find("trash-empty").is_some());
    press(&mut rt, Key::Escape);
    assert!(rt.state.settings.is_none());
    assert_eq!(State::from_root(f.0.clone()).notes, before);
}

#[test]
fn sidebar_material_settings_preview_persist_and_keep_text_opaque() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1280., 943.));
    let before = rt.state.notes.clone();
    assert_eq!(rt.state.sidebar_material, crate::material::Settings::default());
    tap(&mut rt, "settings");
    rt.frame(1.);
    let [x, y, w, h] = rect(&rt, "sidebar-transparency");
    let px = x + 8. + (w - 16.) * 0.7;
    rt.pointer_down(px, y + h / 2.); rt.frame(0.02);
    assert_eq!(rt.state.sidebar_material.transparency, 70);
    assert!(rt.state.store.preference("sidebar_material_light").is_none(), "drag previews without disk writes");
    assert!((crate::ui::sidebar_surface(&rt.state).a - 0.412).abs() < 0.001);
    rt.pointer_up(px, y + h / 2.); rt.frame(0.02);
    assert_eq!(State::from_root(f.0.clone()).sidebar_material.transparency, 70);
    // Grabbing the edge of the thumb must not reposition its centre.
    rt.frame(1.);
    rt.pointer_down(px + 5., y + h / 2.); rt.frame(0.02);
    assert_eq!(rt.state.sidebar_material.transparency, 70);
    rt.pointer_move(px + 5. - (w - 16.) * 0.2, y + h / 2.); rt.frame(0.02);
    assert_eq!(rt.state.sidebar_material.transparency, 50);
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.sidebar_material.transparency, 70, "Escape cancels the preview");
    assert_eq!(State::from_root(f.0.clone()).sidebar_material.transparency, 70);
    let elements = rt.elements();
    assert_eq!(elements.iter().find(|e| e.key.as_deref() == Some("settings-page")).unwrap().fill.a, 0.);
    assert_eq!(elements.iter().find(|e| e.key.as_deref() == Some("settings-body")).unwrap().fill.a, 1.);
    assert_eq!(elements.iter().find(|e| e.key.as_deref() == Some("settings-rail")).unwrap().opacity, 1.);
    rt.update(|_, tx| tx.focus("sidebar-transparency", false));
    press(&mut rt, Key::End); press(&mut rt, Key::Right);
    assert_eq!(rt.state.sidebar_material.transparency, 100);
    press(&mut rt, Key::Home); press(&mut rt, Key::Left);
    assert_eq!(rt.state.sidebar_material.transparency, 0);
    press(&mut rt, Key::PageUp);
    assert_eq!(rt.state.sidebar_material.transparency, 10);
    press(&mut rt, Key::PageDown);
    assert_eq!(rt.state.sidebar_material.transparency, 0);
    rt.update(|_, tx| tx.focus("sidebar-grain", false));
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.sidebar_material.grain, 0);
    let expected = rt.state.sidebar_material.clone();
    drop(rt);
    let restored = State::from_root(f.0.clone());
    assert_eq!(restored.sidebar_material, expected);
    assert_eq!(restored.notes, before);
}

#[test]
fn compact_zoom_control_anchors_and_resets_at_both_window_sizes() {
    for (w, h) in [(1000., 736.), (1420., 1046.)] {
        let f = Fixture::new();
        let mut rt = runtime(State::from_root(f.0.clone()), (w, h));
        let (x, y) = rt.find("zoom-control").unwrap();
        assert!(x > w - 145. && y > h - 55.);
        assert!(rt.find("tools").is_none());
        tap(&mut rt, "zoom-in");
        assert!((rt.state.note().unwrap().zoom - 1.1).abs() < 0.001);
        tap(&mut rt, "zoom-out");
        tap(&mut rt, "zoom-out");
        assert!(rt.state.note().unwrap().zoom < 1.);
        tap(&mut rt, "zoom-reset");
        assert_eq!(rt.state.note().unwrap().zoom, 1.);
    }
}



#[test]
fn settings_return_restores_scrolled_canvas_and_note_list() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    for i in 0..50 {
        let mut extra = s.notes[0].clone();
        extra.id = format!("long-list-{i}");
        s.notes.push(extra);
    }
    let mut rt = runtime(s, (1000., 736.));
    let (x, y) = rt.find("notes-scroll").unwrap();
    rt.wheel(x, y, -280.);
    rt.frame(0.05);
    rt.wheel(750., 580., -165.);
    rt.frame(0.05);
    let list = rt.state.list_offset.get();
    let pan = rt.state.scroll_pos.get().offset;
    assert!(list[1] > 0. && pan[1] > 0., "list={list:?} pan={pan:?}");
    tap(&mut rt, "settings");
    tap(&mut rt, "settings-tab-shortcuts");
    rt.frame(1.);
    let first = rect(&rt, "shortcut-groups")[1];
    rt.wheel(600., 480., -750.);
    rt.frame(0.08);
    assert!(rect(&rt, "shortcut-groups")[1] < first - 100.);
    let [x, y, w, h] = rect(&rt, "settings-back");
    let rail = rect(&rt, "settings-rail");
    assert!(x >= rail[0] && x + w <= rail[0] + rail[2]);
    assert!(y >= rail[1] && y + h <= rail[1] + rail[3]);
    assert!((x - rail[0] - 12.).abs() < 1.);
    assert!((w - (rail[2] - 24.)).abs() < 1.);
    assert!((y + h - (rail[1] + rail[3])).abs() < 1.);
    assert!(h >= 44.);
    rt.pointer_down(x + w - 4., y + h / 2.);
    rt.pointer_up(x + w - 4., y + h / 2.);
    rt.frame(0.01);
    assert!(rt.state.settings.is_none());
    rt.frame(0.15);
    assert_eq!(rt.state.list_offset.get(), list);
    assert_eq!(rt.state.scroll_pos.get().offset, pan);
}

#[test]
fn both_sidebar_columns_rename_with_enter_and_context_menu() {
    for menu in [false, true] {
        let f = Fixture::new();
        let mut rt = runtime(State::from_root(f.0.clone()), (1420., 1046.));
        for (key, name) in [("book-0", "项目笔记本"), ("note-sample-0", "第一周日志")] {
            if menu {
                let (x, y) = rt.find(key).unwrap();
                rt.secondary_down(x, y);
                rt.secondary_up(x, y);
                rt.frame(0.01);
                rt.resolve_popup(Some("rename")).unwrap();
                rt.frame(0.01);
            } else {
                tap(&mut rt, key);
                press(&mut rt, Key::Enter);
            }
            assert!(rt.find("rename-field").is_some(), "{key} menu={menu}");
            type_text(&mut rt, name);
            press(&mut rt, Key::Enter);
        }
        let restored = State::from_root(f.0.clone());
        assert_eq!(restored.book_name("工作"), "项目笔记本");
        assert_eq!(restored.note().unwrap().title, "第一周日志");
        assert_eq!(restored.note().unwrap().blocks[0].title, rt.state.note().unwrap().blocks[0].title);
    }
}

#[test]
fn new_note_names_in_sidebar_and_escape_cancels_a_later_rename() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1420., 1046.));
    tap(&mut rt, "body-intro");
    command(&mut rt, 'n', false);
    choose_new_page(&mut rt, WorkspaceMode::Canvas);
    assert!(rt.state.note().unwrap().blocks.is_empty());
    type_text(&mut rt, "会议笔记");
    press(&mut rt, Key::Enter);
    let key = format!("note-{}", rt.state.current);
    tap(&mut rt, &key);
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "取消修改");
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.note().unwrap().title, "会议笔记");
    assert!(rt.find("rename-field").is_none());
}

fn isolated_block(kind: Kind, zoom: f32) -> (Fixture, Runtime<State>, String) {
    let fixture = Fixture::new();
    let mut state = State::from_root(fixture.0.clone());
    let mut block = Block::new(kind.clone(), 60., 160., 320., 180., "", "已有内容");
    block.min_h = 180.;
    if kind == Kind::Table {
        block.table = Some(table_data::TableData::default());
    }
    let id = block.id.clone();
    let note = state.note_mut().unwrap();
    note.blocks = vec![block.clone()];
    note.zoom = zoom;
    state.rebuild_editors();
    (fixture, runtime(state, (1420., 1046.)), id)
}

#[test]
fn all_block_interiors_consume_clicks_without_creating_another_block() {
    for kind in [
        Kind::Text,
        Kind::Tasks,
        Kind::Code,
        Kind::Callout,
        Kind::Table,
        Kind::Image,
        Kind::Handwritten,
        Kind::Shape,
    ] {
        for zoom in [0.7, 1.5] {
            let (_fixture, mut rt, id) = isolated_block(kind.clone(), zoom);
            for (fx, fy) in [(0.03, 0.10), (0.50, 0.50), (0.90, 0.80)] {
                let [x, y, w, h] = rect(&rt, &format!("block-{id}"));
                rt.pointer_down(x + w * fx, y + h * fy);
                rt.pointer_up(x + w * fx, y + h * fy);
                rt.frame(0.02);
                assert_eq!(
                    rt.state.note().unwrap().blocks.len(),
                    1,
                    "{kind:?} at {fx}, {fy}"
                );
                assert!(rt.state.draft.is_none(), "{kind:?} created a draft");
            }
        }
    }
}



#[test]
fn canvas_blocks_follow_edge_scrolling_beyond_the_original_viewport() {
    for kind in [
        Kind::Text,
        Kind::Tasks,
        Kind::Code,
        Kind::Callout,
        Kind::Table,
        Kind::Image,
        Kind::Handwritten,
        Kind::Shape,
    ] {
        for zoom in [0.7, 1.5] {
            let (fixture, mut rt, id) = isolated_block(kind.clone(), zoom);
            let [vx, vy, vw, vh] = rect(&rt, &rt.state.scroll_key());
            let (sx, sy) = rt.find(&format!("move-edge-top-{id}")).unwrap();
            let target = [vx + vw - 4., vy + vh - 4.];
            rt.pointer_down(sx, sy);
            rt.pointer_move(target[0], target[1]);
            for _ in 0..45 { rt.frame(1. / 60.); }
            let at = rt.state.scroll_pos.get().offset;
            assert!(at[0] > 200. && at[1] > 200., "{kind:?} {zoom}: {at:?}");
            let (hx, hy) = rt.find(&format!("move-edge-top-{id}")).unwrap();
            assert!((hx-target[0]).abs() < 2. && (hy-target[1]).abs() < 2.,
                "grab drifted at zoom {zoom}: {hx},{hy} vs {target:?}");
            rt.pointer_up(target[0], target[1]);
            rt.frame(0.1);
            let block = rt.state.block_mut(&id).unwrap().clone();
            assert!((block.y-crate::canvas::ORIGIN_Y)*zoom > vh);
            rt.frame(0.4);
            assert_eq!(rt.state.scroll_pos.get().offset, at);
            drop(rt);
            let reopened = State::from_root(fixture.0.clone());
            let saved = reopened.note().unwrap().blocks.iter().find(|b| b.id == id).unwrap();
            assert_eq!((saved.x, saved.y), (block.x, block.y));
        }
    }
}

#[test]
fn canvas_horizontal_scrollbar_reveals_offscreen_cards_in_both_directions() {
    let (_fixture, mut rt, id) = isolated_block(Kind::Shape, 1.);
    assert_eq!(rect(&rt, "scroll-bar-x")[2], 0.);
    rt.update(|s, _| s.note_mut().unwrap().blocks[0].x = 1600.);
    for _ in 0..12 { rt.frame(0.016); }
    rt.update(|s, tx| tx.scroll_to_instant(s.scroll_key(), 0., 40.));
    rt.pointer_move(4., 4.);
    for _ in 0..80 { rt.frame(0.016); }
    let [x, y, w, h] = rect(&rt, &rt.state.scroll_key());
    let zoom = rect(&rt, "zoom-control");
    let card = format!("block-{id}");
    assert!(rect(&rt, &card)[0] > x + w);
    assert!(rect(&rt, "scroll-bar")[3] > 0.);
    let horizontal_thumb = |rt: &Runtime<State>| rt.elements().into_iter()
        .find(|e| e.path.ends_with("/scroll-bar-x/thumb-slot/thumb")).unwrap();
    let [left, top, width, height] = horizontal_thumb(&rt).rect;
    for hover in [true, false, true] {
        if hover { rt.pointer_move(left + width / 2., top + height / 2.); }
        else { rt.pointer_move(4., 4.); }
        for _ in 0..24 {
            rt.frame(0.016);
            let frame_width = horizontal_thumb(&rt).rect[2];
            assert!((frame_width - width).abs() < 0.1, "hover={hover}, width {width} became {frame_width}");
        }
        let current = horizontal_thumb(&rt).rect;
        assert!((current[3] - if hover { 5. } else { 3. }).abs() < 0.1);
    }
    for right in [true, false] {
        let thumb = horizontal_thumb(&rt);
        let [left, top, width, height] = thumb.rect;
        assert!(width > 0. && width < w && thumb.opacity > 0.99);
        assert!(top > zoom[1] + zoom[3] && (y + h - top - height).abs() < 4.);
        let py = top + height / 2.;
        let end = if right { x + w - 2. } else { x + 2. };
        rt.pointer_down(left + width / 2., py);
        rt.frame(0.016);
        rt.pointer_move(end, py);
        rt.frame(0.016);
        rt.pointer_up(end, py);
        for _ in 0..8 { rt.frame(0.016); }
        let pos = rt.state.scroll_pos.get();
        assert!((pos.offset[0] - if right { pos.max[0] } else { 0. }).abs() < 1.);
        assert_eq!(pos.offset[1], 40.);
        let block = rect(&rt, &card);
        if right { assert!(block[0] >= x && block[0] + block[2] <= x + w); }
        else { assert!(block[0] > x + w); }
        assert_eq!(rt.state.note().unwrap().blocks[0].x, 1600.);
        assert_eq!(rt.state.note().unwrap().zoom, 1.);
    }
}

#[test]
fn canvas_edge_drag_scrolls_back_and_escape_restores_the_start() {
    let (_fixture, mut rt, id) = isolated_block(Kind::Shape, 1.);
    let start = rt.state.note().unwrap().blocks[0].clone();
    let [x, y, w, h] = rect(&rt, &rt.state.scroll_key());
    let (sx, sy) = rt.find(&format!("move-edge-top-{id}")).unwrap();
    rt.pointer_down(sx, sy);
    rt.pointer_move(x + w - 4., y + h - 4.);
    for _ in 0..30 { rt.frame(1. / 60.); }
    let away = rt.state.scroll_pos.get().offset;
    rt.pointer_move(x + 4., y + 4.);
    for _ in 0..12 { rt.frame(1. / 60.); }
    let back = rt.state.scroll_pos.get().offset;
    assert!(back[0] < away[0] && back[1] < away[1]);
    press(&mut rt, Key::Escape);
    rt.frame(0.05);
    let b = &rt.state.note().unwrap().blocks[0];
    assert_eq!((b.x, b.y), (start.x, start.y));
    assert_eq!(rt.state.scroll_pos.get().offset, [0., 0.]);
}

#[test]
fn dragging_and_resizing_can_overlap_a_neighbor() {
    let (_fixture, mut rt, id) = isolated_block(Kind::Text, 1.);
    let neighbor = Block::new(Kind::Shape, 500., 160., 150., 180., "", "");
    rt.update(|s, _| s.note_mut().unwrap().blocks.push(neighbor));
    rt.frame(0.01);
    drag_key(&mut rt, &format!("move-edge-top-{id}"), 400., 0.);
    let b = rt.state.block_mut(&id).unwrap();
    assert!(b.x < 650. && b.x + b.w > 500.);
    let width = b.w;
    drag_key(&mut rt, &format!("width-handle-{id}"), 50., 0.);
    let b = rt.state.block_mut(&id).unwrap();
    assert!(b.w > width);
}

#[test]
fn scrolled_canvas_uses_visible_edges_for_resizing() {
    let (_fixture, mut rt, id) = isolated_block(Kind::Text, 1.5);
    rt.update(|s, _| s.block_mut(&id).unwrap().y = 1000.);
    rt.frame(0.02);
    rt.wheel(900., 700., -1000.);
    rt.frame(0.02);
    rt.frame(0.02); // Rune scroll observation wakes the next poll to mount visible cards.
    let offset = rt.state.scroll_pos.get().offset[1];
    assert!(offset > 0.);
    tap(&mut rt, &format!("move-edge-top-{id}"));
    let [vx, vy, vw, vh] = rect(&rt, &rt.state.scroll_key());
    drag_key(&mut rt, &format!("size-handle-{id}"), 5000., 5000.);
    let [x, y, w, h] = rect(&rt, &format!("block-{id}"));
    assert!(x >= vx && y >= vy && x + w <= vx + vw && y + h <= vy + vh);
}

#[test]
fn blank_canvas_creation_keeps_the_requested_position_even_when_blocks_overlap() {
    let (_fixture, mut rt, id) = isolated_block(Kind::Shape, 1.);
    // This click is outside the existing block, but a default-width block would cover it.
    let [x, y, _, _] = rect(&rt, &format!("block-{id}"));
    rt.pointer_down(x - 20., y + 60.);
    rt.pointer_up(x - 20., y + 60.);
    rt.frame(0.02);
    let draft = rt.state.draft.clone().unwrap();
    let blocks = &rt.state.note().unwrap().blocks;
    let old = blocks.iter().find(|b| b.id == id).unwrap();
    let new = blocks.iter().find(|b| b.id == draft).unwrap();
    assert!((new.x - (old.x - 20. - crate::doc_editor::CARD_TEXT_INSET).max(0.)).abs() < 1.);
    assert!((new.y - (old.y + 44.)).abs() < 1.);
}

#[test]
fn arrange_button_removes_overlap_preserves_blocks_and_undoes_as_one_action() {
    for (width, zoom) in [(1420., 1.), (1000., 1.5)] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        let note = state.note_mut().unwrap();
        note.zoom = zoom;
        for b in &mut note.blocks {
            b.x = 48.;
            b.y = 150.;
        }
        let mut rt = runtime(state, (width, 1046.));
        rt.frame(0.1);
        let before = rt.state.note().unwrap().blocks.clone();
        let history = rt.state.undo.len();
        tap(&mut rt, "canvas-more");
        rt.resolve_popup(Some("arrange")).unwrap();
        rt.frame(0.02);
        rt.frame(0.1);
        let arranged = rt.state.note().unwrap().blocks.clone();
        assert_eq!(arranged.len(), before.len());
        for (a, b) in arranged.iter().zip(&before) {
            let mut original = a.clone();
            original.x = b.x;
            original.y = b.y;
            assert_eq!(original, *b, "arranging changes only position");
        }
        for (i, a) in arranged.iter().enumerate() {
            for b in arranged.iter().skip(i + 1) {
                assert!(
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
                );
            }
        }
        assert_eq!(rt.state.undo.len(), history + 1);
        let restored = State::from_root(f.0.clone());
        assert_eq!(restored.note().unwrap().blocks, arranged);
        tap(&mut rt, "canvas-more");
        rt.resolve_popup(Some("arrange")).unwrap();
        rt.frame(0.02);
        assert_eq!(rt.state.note().unwrap().blocks, arranged);
        assert_eq!(
            rt.state.undo.len(),
            history + 1,
            "repeated arrange has no extra undo step"
        );
        command(&mut rt, 'z', false);
        assert_eq!(rt.state.note().unwrap().blocks, before);
        command(&mut rt, 'z', true);
        assert_eq!(rt.state.note().unwrap().blocks, arranged);
    }
}

#[test]
fn arrange_button_stays_inside_header_with_long_names_and_an_empty_note() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.note_mut().unwrap().title = "很长的笔记名称".repeat(20);
    let mut rt = runtime(state, (1000., 736.));
    let [hx, hy, hw, hh] = rect(&rt, "editor-toolbar");
    let [x, y, w, h] = rect(&rt, "canvas-more");
    assert!(x >= hx && y >= hy && x + w <= hx + hw && y + h <= hy + hh);
    rt.update(|s, _| s.note_mut().unwrap().blocks.clear());
    rt.frame(0.02);
    let history = rt.state.undo.len();
    assert!(rt.find("arrange-blocks").is_none());
    assert!(rt.find("canvas-start-hint").is_some());
    assert!(rt.state.note().unwrap().blocks.is_empty());
    assert_eq!(rt.state.undo.len(), history);
}

#[test]
fn arrange_blocks_animates_smoothly_and_settles() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    if let Some(b) = state.block_mut("callout") {
        b.x = 640.;
        b.y = 720.;
    }
    let mut rt = runtime(state, (1280., 943.));
    let before = rect(&rt, "block-callout");
    tap(&mut rt, "canvas-more");
    rt.resolve_popup(Some("arrange")).unwrap();
    rt.frame(0.08);
    assert!(rt.animating, "arrange must animate blocks instead of snapping");
    let mid = rect(&rt, "block-callout");
    let target_block = rt
        .state
        .note()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.id == "callout")
        .unwrap()
        .clone();
    assert!(
        (mid[0] - before[0]).abs() > 1. && (mid[0] - (before[0] + (target_block.x - 640.))).abs() > 1.,
        "block-callout x should be mid-flight: before={before:?}, mid={mid:?}, target={}",
        target_block.x
    );
    for _ in 0..20 {
        rt.frame(0.05);
    }
    assert!(!rt.animating, "arrange animation must settle cleanly");
    assert!(rt.state.arrange_anim_until.is_none());
}

#[test]
fn legacy_database_requires_explicit_migration() {
    let f = Fixture(std::env::temp_dir().join(format!("echo-legacy-{}",uid())));
    std::fs::create_dir_all(&f.0).unwrap();
    std::fs::write(f.0.join("echo.sqlite3"), b"legacy database").unwrap();
    assert!(crate::storage::Store::open(f.0.clone()).is_err());
    assert_eq!(std::fs::read(f.0.join("echo.sqlite3")).unwrap(), b"legacy database");
}

#[test]
fn new_page_picker_creates_both_types_in_one_notebook_and_settings_has_no_mode_switch() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1280., 943.));
    let book = rt.state.book.clone();
    let existing = rt.state.current.clone();
    let count = rt.state.notes.len();
    tap(&mut rt, "editor-new-page");
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.notes.len(), count);
    assert_eq!(rt.state.current, existing);
    command(&mut rt, 'n', false);
    choose_new_page(&mut rt, WorkspaceMode::Document);
    let doc = rt.state.current.clone();
    type_text(&mut rt, "Unified document"); press(&mut rt, Key::Enter);
    type_text(&mut rt, "Document stays intact");
    tap(&mut rt, "editor-new-page");
    choose_new_page(&mut rt, WorkspaceMode::Canvas);
    let canvas = rt.state.current.clone();
    type_text(&mut rt, "Unified canvas"); press(&mut rt, Key::Enter);
    assert_eq!(rt.state.book, book);
    assert!(rt.find(&format!("note-{doc}")).is_some());
    assert!(rt.find(&format!("note-{canvas}")).is_some());
    let edge = rect(&rt, "note-list");
    tap(&mut rt, &format!("note-{doc}"));
    assert_eq!(rt.state.mode, WorkspaceMode::Document);
    assert_eq!(rect(&rt, "note-list"), edge);
    assert!(rt.state.editor.active.core.doc.to_plain_text().contains("Document stays intact"));
    tap(&mut rt, "settings");
    assert!(rt.find("mode-canvas").is_none() && rt.find("mode-document").is_none());
    assert!(rt.find("mode-preview").is_none());
    tap(&mut rt, "settings-back");
    tap(&mut rt, &format!("note-{canvas}"));
    assert_eq!(rt.state.mode, WorkspaceMode::Canvas);
    let reopened = State::from_root(f.0.clone());
    assert_eq!(reopened.current, canvas);
    assert_eq!(reopened.mode, WorkspaceMode::Canvas);
    assert!(reopened.notes.iter().any(|n| n.id == doc && n.notebook == book));
}

#[test]
fn document_mode_supports_multi_level_tree_full_page_editor_outline_and_slash() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));

    create_same_type(&mut rt);
    let root_id = rt.state.current.clone();
    assert!(rt.state.editor.active.focused);
    assert_eq!(rt.state.editor.active.core.sel.head.block, crate::doc_core::TITLE_ID);
    type_text(&mut rt, "结构化文档架构");
    rt.frame(0.02);
    assert_eq!(rt.state.note().unwrap().title, "结构化文档架构");
    assert!(rt.find("doc-outline-toggle").is_none(), "a title alone has no outline");

    press(&mut rt, Key::Enter);
    rt.frame(0.02);
    let body_id = rt.state.editor.active.core.doc.blocks[0].id;
    assert_eq!(rt.state.editor.active.core.sel.head.block, body_id);

    type_text(&mut rt, "#");
    type_text(&mut rt, " ");
    type_text(&mut rt, "核心设计");
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "连续整页文档正文内容");
    rt.frame(0.04);
    assert!(
        rt.find("outline-item-0").is_none(),
        "Outline starts collapsed"
    );
    let before = rt.state.editor.active.text_layout(body_id).unwrap().rect;
    tap(&mut rt, "doc-outline-toggle");
    rt.frame(0.02);
    assert!(rt.find("outline-item-0").is_some());
    let after = rt.state.editor.active.text_layout(body_id).unwrap().rect;
    assert!(after[2] < before[2], "Outline reserves its own column");
    let outline = rect(&rt, "doc-outline-pane");
    assert!(after[0] + after[2] <= outline[0], "Pinned outline sits right of text");

    // Create a child document and a grandchild document in the multi-level tree
    tap(&mut rt, "doc-more");
    rt.resolve_popup(Some("new")).unwrap();
    choose_new_page(&mut rt, WorkspaceMode::Document);
    rt.frame(0.02);
    let child_id = rt.state.current.clone();
    assert_eq!(
        rt.state.note().unwrap().parent_id.as_deref(),
        Some(root_id.as_str())
    );
    type_text(&mut rt, "二级子文档");
    press(&mut rt, Key::Enter);
    rt.frame(0.02);

    tap(&mut rt, &format!("page-more-{child_id}"));
    rt.resolve_popup(Some("new-child")).unwrap();
    choose_new_page(&mut rt, WorkspaceMode::Document);
    rt.frame(0.02);
    let grandchild_id = rt.state.current.clone();
    let crumbs: Vec<String> = rt
        .state
        .note_breadcrumbs(&grandchild_id)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    assert_eq!(crumbs, vec!["结构化文档架构", "二级子文档", "无标题文档"]);

    // Promote grandchild up one level so its parent becomes root_id
    rt.update(|s, _| s.promote_note(&grandchild_id));
    assert_eq!(
        rt.state
            .notes
            .iter()
            .find(|n| n.id == grandchild_id)
            .and_then(|n| n.parent_id.as_deref()),
        Some(root_id.as_str())
    );

    // Ancestor navigation remains available in the quiet document menu.
    tap(&mut rt, "doc-more");
    rt.resolve_popup(Some(&format!("ancestor:{root_id}"))).unwrap();
    rt.frame(0.02);
    assert_eq!(rt.state.current, root_id);
}

#[test]
fn sidebar_collapses_and_expands_in_both_modes_and_persists() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));

    assert!(!rt.state.sidebar_collapsed);
    assert!(rt.find("sidebar").is_some());
    assert!(rt.find("toggle-sidebar").is_some());
    assert!(rect(&rt, "toggle-sidebar")[1] < rect(&rt, "settings")[1]);
    assert!(rect(&rt, "toggle-sidebar")[0] < rect(&rt, "editor-surface")[0]);
    assert!(rt.find("sort-notes").is_none());
    assert!(rt.find("rail-new-page").is_none());

    tap(&mut rt, "toggle-sidebar");
    rt.frame(1.);
    assert!(rt.state.sidebar_collapsed);
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("sidebar") && e.inert && e.visible[2] < 0.1));
    assert!(rt.find("toggle-sidebar").is_some());
    assert_eq!(rect(&rt, "canvas-pane")[2], 1280. - 2. * crate::ui::WINDOW_INSET);

    // Reopen from disk and confirm collapsed state persists
    drop(rt);
    let reopened = State::from_root(f.0.clone());
    assert!(reopened.sidebar_collapsed);
    let mut rt = runtime(reopened, (1280., 943.));
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("sidebar") && e.inert && e.visible[2] < 0.1));

    // Pressing Cmd+\ expands the sidebar again
    command(&mut rt, '\\', false);
    rt.frame(1.);
    assert!(!rt.state.sidebar_collapsed);
    assert!(rt.find("sidebar").is_some());

    // Also works in Canvas mode
    rt.update(|s, _| s.choose_mode(WorkspaceMode::Canvas));
    assert!(rt.find("sidebar").is_some());
    tap(&mut rt, "toggle-sidebar");
    rt.frame(1.);
    assert!(rt.state.sidebar_collapsed);
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("sidebar") && e.inert && e.visible[2] < 0.1));
    assert_eq!(rect(&rt, "canvas")[2], 1280. - 2. * crate::ui::WINDOW_INSET);
    tap(&mut rt, "toggle-sidebar");
    rt.frame(1.);
    assert!(!rt.state.sidebar_collapsed);
    assert!(rt.find("sidebar").is_some());
}

#[test]
fn settings_trash_manages_both_formats_and_supports_undo() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    let canvas_id = state.current.clone();
    state.trash_note();
    state.choose_mode(WorkspaceMode::Document);
    state.new_note();
    let doc_id = state.current.clone();
    state.trash_note();
    let current = state.current.clone();
    let mut rt = runtime(state, (1280., 943.));
    assert!(rt.find("book-6-toggle").is_none());
    tap(&mut rt, "settings");
    tap(&mut rt, "settings-tab-trash");
    assert!(rt.find(&format!("trash-note-{canvas_id}")).is_some());
    assert!(rt.find(&format!("trash-note-{doc_id}")).is_some());
    tap(&mut rt, &format!("restore-note-{canvas_id}"));
    assert_eq!(rt.state.current, current, "restoring another format must preserve the current document");
    assert_ne!(rt.state.notes.iter().find(|n| n.id == canvas_id).unwrap().notebook, "回收站");
    command(&mut rt, 'z', false);
    assert_eq!(rt.state.notes.iter().find(|n| n.id == canvas_id).unwrap().notebook, "回收站");
    tap(&mut rt, &format!("delete-forever-{doc_id}"));
    assert!(!rt.state.notes.iter().any(|n| n.id == doc_id));
    command(&mut rt, 'z', false);
    assert!(rt.state.notes.iter().any(|n| n.id == doc_id));
    tap(&mut rt, "empty-trash");
    assert!(!rt.state.notes.iter().any(|n| n.notebook == "回收站"));
    assert!(rt.find("trash-empty").is_some());
    command(&mut rt, 'z', false);
    assert!(rt.find(&format!("trash-note-{canvas_id}")).is_some());
    assert!(rt.find(&format!("trash-note-{doc_id}")).is_some());
    tap(&mut rt, &format!("restore-note-{doc_id}"));
    assert_eq!(rt.state.current, current, "restoring a file preserves the open page regardless of type");
    tap(&mut rt, "settings-back");
    tap(&mut rt, &format!("note-{doc_id}"));
    assert_eq!(rt.state.note().unwrap().format, crate::doc_core::NoteFormat::Document);
    rt.update(|s, _| s.choose_mode(WorkspaceMode::Canvas));
    assert!(rt.find("book-6").is_none());
}

#[test]
fn document_mode_drag_handles_reorder_every_block_type_and_undo() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));
    rt.frame(0.05);

    // Append an empty trailing paragraph block so we also verify the empty '+' capsule handle
    rt.update(|s, _| {
        let empty = s
            .editor.active
            .core
            .doc
            .new_block(crate::doc_core::DocBlockKind::Text, "");
        s.editor.active.core.doc.blocks.push(empty);
        crate::doc_editor::sync_to_state(s);
    });
    rt.frame(0.05);

    let initial_order: Vec<u64> = rt
        .state
        .editor.active
        .core
        .doc
        .blocks
        .iter()
        .map(|b| b.id)
        .collect();
    assert!(
        initial_order.len() >= 4,
        "Default note should have multiple block types"
    );
    let first_id = initial_order[0];
    let third_id = initial_order[2];
    let empty_tail_id = *initial_order.last().unwrap();

    // Hover first block to verify the handle capsule:
    // Non-empty block handle is a 42x26pt pill at left = -48pt, centered on the first line.
    let [bx, by, _, _] = rect(&rt, &format!("doc-blk-{first_id}"));
    rt.pointer_move(bx + 24., by + 12.);
    rt.frame(0.03);
    let [hx, hy, hw, hh] = rect(&rt, &format!("doc-handle-{first_id}"));
    assert!((hw - 42.).abs() < 0.5 && (hh - 26.).abs() < 0.5);
    assert!(
        (hx - (bx - 48.)).abs() < 0.5,
        "Non-empty block handle should sit at left = -48pt (got hx={hx}, bx={bx})"
    );
    assert!(
        (hy - rect(&rt, &format!("doc-text-{first_id}"))[1]).abs() < 0.5,
        "Non-empty text block handle should align with top margin + first line (got hy={hy}, by={by})"
    );

    // Hover empty trailing text block to verify its 28x26pt '+' pill at left = -34pt.
    let [tbx, tby, _, _] = rect(&rt, &format!("doc-blk-{empty_tail_id}"));
    rt.pointer_move(tbx + 24., tby + 10.);
    rt.frame(0.03);
    let [thx, thy, thw, thh] = rect(&rt, &format!("doc-handle-{empty_tail_id}"));
    assert!((thw - 28.).abs() < 0.5 && (thh - 26.).abs() < 0.5);
    assert!(
        (thx - (tbx - 34.)).abs() < 0.5,
        "Empty block '+' handle should sit at left = -34pt (got thx={thx}, tbx={tbx})"
    );
    assert!((thy - rect(&rt, &format!("doc-text-{empty_tail_id}"))[1]).abs() < 0.5);

    // Move pointer from block interior -> across the -3pt gap -> onto the handle center
    // without losing hover_block (real pointer traversal regression test).
    rt.pointer_move(bx + 24., by + 12.);
    rt.frame(0.03);
    let [_, ty, _, th] = rect(&rt, &format!("doc-blk-{third_id}"));
    let start_x = hx + hw * 0.5;
    let start_y = hy + hh * 0.5;
    rt.pointer_move(bx - 3., by + 12.);
    rt.frame(0.02);
    assert!(
        rt.find(&format!("doc-handle-{first_id}")).is_some(),
        "Handle must remain visible while pointer crosses gutter gap"
    );
    rt.pointer_move(start_x, start_y);
    rt.frame(0.02);
    assert!(
        rt.find(&format!("doc-handle-{first_id}")).is_some(),
        "Handle must remain visible when pointer rests directly over the handle"
    );
    let target_y = ty + th - 2.;

    rt.pointer_down(start_x, start_y);
    rt.pointer_move(start_x, target_y);
    rt.frame(0.03);
    assert!(
        rt.find("doc-drop-indicator").is_some(),
        "Dragging a document block should display the accent blue drop indicator line"
    );
    assert!(
        rt.find("doc-drag-ghost").is_some(),
        "Dragging a document block should display the floating drag ghost card following the pointer"
    );
    rt.pointer_up(start_x, target_y);
    rt.frame(0.05);

    let after_order: Vec<u64> = rt
        .state
        .editor.active
        .core
        .doc
        .blocks
        .iter()
        .map(|b| b.id)
        .collect();
    assert_eq!(after_order[2], first_id);
    assert_ne!(after_order, initial_order);

    // Undo restores the exact previous block order
    command(&mut rt, 'z', false);
    rt.frame(0.05);
    let undone_order: Vec<u64> = rt
        .state
        .editor.active
        .core
        .doc
        .blocks
        .iter()
        .map(|b| b.id)
        .collect();
    assert_eq!(undone_order, initial_order);
}

#[test]
fn folder_and_note_drag_reordering_persists_across_reopen() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));
    rt.frame(0.05);

    create_same_type(&mut rt);
    type_text(&mut rt, "第一篇文档");
    press(&mut rt, Key::Enter);
    let note_a = rt.state.current.clone();

    create_same_type(&mut rt);
    type_text(&mut rt, "第二篇文档");
    press(&mut rt, Key::Enter);
    let note_b = rt.state.current.clone();
    rt.frame(0.05);

    let root_ids_before: Vec<String> = rt
        .state
        .notes
        .iter()
        .filter(|n| n.notebook == "工作" && n.parent_id.is_none())
        .map(|n| n.id.clone())
        .collect();
    let pos_a_before = root_ids_before.iter().position(|id| id == &note_a).unwrap();
    let pos_b_before = root_ids_before.iter().position(|id| id == &note_b).unwrap();
    assert!(pos_b_before < pos_a_before);

    // Drag note_a above note_b in the document tree
    let [nx, ny, nw, nh] = rect(&rt, &format!("note-{note_a}"));
    let [_, by, _, _] = rect(&rt, &format!("note-{note_b}"));
    rt.pointer_down(nx + nw * 0.5, ny + nh * 0.5);
    rt.pointer_move(nx + nw * 0.5, by + 2.);
    rt.frame(0.02);
    rt.pointer_up(nx + nw * 0.5, by + 2.);
    rt.frame(0.05);

    let root_ids_after: Vec<String> = rt
        .state
        .notes
        .iter()
        .filter(|n| n.notebook == "工作" && n.parent_id.is_none())
        .map(|n| n.id.clone())
        .collect();
    let pos_a_after = root_ids_after.iter().position(|id| id == &note_a).unwrap();
    let pos_b_after = root_ids_after.iter().position(|id| id == &note_b).unwrap();
    assert!(
        pos_a_after < pos_b_after,
        "Dragged note_a should now appear before note_b"
    );

    // Drag folder book-0 ("工作") below book-2 ("随笔")
    assert_eq!(rt.state.book_at(0), "工作");
    let [bx, by, bw, bh] = rect(&rt, "book-0");
    let [_, b2y, _, b2h] = rect(&rt, "book-2");
    rt.pointer_down(bx + bw * 0.5, by + bh * 0.5);
    rt.pointer_move(bx + bw * 0.5, b2y + b2h - 2.);
    rt.frame(0.02);
    assert!(rt.find("nav-drop-indicator").is_some());
    assert!(
        rt.find("nav-drag-ghost").is_some(),
        "Dragging a folder in sidebar should display the floating lifted row ghost card"
    );
    rt.pointer_up(bx + bw * 0.5, b2y + b2h - 2.);
    rt.frame(0.05);
    assert_eq!(rt.state.book_at(2), "工作");

    // Reopen from SQLite and verify both folder order and note order persisted
    drop(rt);
    let reopened = State::from_root(f.0.clone());
    assert_eq!(reopened.book_at(2), "工作");
    let reopened_ids: Vec<String> = reopened
        .notes
        .iter()
        .filter(|n| n.notebook == "工作" && n.parent_id.is_none())
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(reopened_ids, root_ids_after);
}

#[test]
fn document_and_canvas_formats_stay_strictly_isolated_without_conversion() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    let canvas_ids: Vec<String> = state
        .notes
        .iter()
        .filter(|n| n.format == crate::doc_core::NoteFormat::Canvas)
        .map(|n| n.id.clone())
        .collect();
    assert!(!canvas_ids.is_empty());
    assert!(state.notes.iter().all(|n| {
        n.format != crate::doc_core::NoteFormat::Canvas || n.doc.is_none()
    }));

    // Switch to Document mode: existing Canvas notes must NOT be converted to Document format
    state.choose_mode(WorkspaceMode::Document);
    let canvas_after: Vec<String> = state
        .notes
        .iter()
        .filter(|n| n.format == crate::doc_core::NoteFormat::Canvas)
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(canvas_after, canvas_ids);
    assert!(state.notes.iter().all(|n| {
        n.format != crate::doc_core::NoteFormat::Canvas || n.doc.is_none()
    }));

    // Document mode only sees Document-format notes, and switching back only sees Canvas notes
    let doc_ids: Vec<String> = state
        .notes
        .iter()
        .filter(|n| n.format == crate::doc_core::NoteFormat::Document)
        .map(|n| n.id.clone())
        .collect();
    assert!(!doc_ids.is_empty());
    assert!(doc_ids.contains(&state.current));

    state.choose_mode(WorkspaceMode::Canvas);
    assert!(canvas_ids.contains(&state.current));
    assert_eq!(state.active_format(), crate::doc_core::NoteFormat::Canvas);

    drop(state);
    let reopened = State::from_root(f.0.clone());
    assert!(reopened.notes.iter().all(|n| match n.format {
        crate::doc_core::NoteFormat::Canvas => n.doc.is_none(),
        crate::doc_core::NoteFormat::Document => n.doc.is_some(),
    }));
}

#[test]
fn document_mode_scheme_b_seamless_enter_cross_selection_slash_and_ime() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));

    // Create fresh document, type title via IME composition + commit, then Enter into body
    create_same_type(&mut rt);
    rt.update(|s, tx| {
        assert!(crate::doc_editor::on_ime(
            s,
            &rune::Ime::Preedit("wendang".into(), Some((7, 7))),
            tx
        ));
    });
    rt.frame(0.02);
    assert!(rt.find("doc-caret").is_some());
    assert!(rt.ime_area.is_some(), "doc-caret must expose ime_cursor area for OS IME candidate window");
    assert_eq!(rt.state.note().unwrap().title, "无标题文档");
    rt.update(|s, tx| {
        assert!(crate::doc_editor::on_ime(
            s,
            &rune::Ime::Commit("文档统一内核".into()),
            tx
        ));
    });
    rt.frame(0.02);
    assert_eq!(rt.state.note().unwrap().title, "文档统一内核");

    press(&mut rt, Key::Enter);
    type_text(&mut rt, "#");
    type_text(&mut rt, " ");
    type_text(&mut rt, "章节标题");
    assert!(matches!(
        rt.state.editor.active.core.doc.blocks[0].kind,
        crate::doc_core::DocBlockKind::Heading { level: 1 }
    ));

    // Shift+Enter soft break inside same block, Enter creates a new block
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods {
            shift: true,
            ..Default::default()
        },
    });
    rt.frame(0.01);
    type_text(&mut rt, "同块软换行");
    assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 1);
    assert!(rt.state.editor.active.core.doc.blocks[0].text.text.contains('\n'));

    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 2);

    // Dual slash trigger with Chinese pause mark '、' + filter opens slash menu and inserts Callout
    type_text(&mut rt, "、");
    type_text(&mut rt, "callout");
    rt.frame(0.02);
    assert!(rt.find("doc-slash-menu").is_some());
    tap(&mut rt, "slash-item-callout");
    rt.frame(0.02);
    assert!(matches!(
        rt.state.editor.active.core.doc.blocks[1].kind,
        crate::doc_core::DocBlockKind::Callout { .. }
    ));

    // Cmd+A twice selects whole document (Title + body), Delete keeps empty document + Undo restores
    command(&mut rt, 'a', false);
    command(&mut rt, 'a', false);
    assert!(rt.state.editor.active.core.selected_text().contains("章节标题"));
    press(&mut rt, Key::Backspace);
    rt.frame(0.02);
    assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 1);
    assert!(rt.state.editor.active.core.doc.blocks[0].text.is_empty());

    command(&mut rt, 'z', false);
    rt.frame(0.02);
    assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 2);
}

#[test]
fn document_mode_escapes_trailing_code_block_on_external_click_and_instant_caret() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut rt = runtime(state, (1280., 943.));
    create_same_type(&mut rt);
    rt.frame(0.03);

    // Insert a trailing Code block via slash menu + Tab confirmation
    let bid = rt.state.editor.active.core.doc.blocks[0].id;
    let [bx, by, _, _] = rect(&rt, &format!("doc-blk-{bid}"));
    rt.pointer_down(bx + 20., by + 10.);
    rt.pointer_up(bx + 20., by + 10.);
    rt.frame(0.02);
    type_text(&mut rt, "/");
    type_text(&mut rt, "code");
    rt.frame(0.02);
    assert!(rt.find("doc-slash-menu").is_some());
    assert!(rt.find("slash-item-code").is_some());
    press(&mut rt, Key::Tab);
    rt.frame(0.02);

    assert!(matches!(
        rt.state.editor.active.core.doc.blocks.last().unwrap().kind,
        crate::doc_core::DocBlockKind::Code { .. }
    ));
    type_text(&mut rt, "let x = 1;");
    rt.frame(0.02);

    // Caret moves instantaneously with no glide animation (zero interpolation after key/click)
    let caret_before = rect(&rt, "doc-caret");
    press(&mut rt, Key::Home);
    rt.frame(0.001);
    let caret_after_home = rect(&rt, "doc-caret");
    rt.frame(0.10);
    let caret_settled = rect(&rt, "doc-caret");
    assert!(
        caret_after_home[0] < caret_before[0],
        "Home should jump caret leftward immediately"
    );
    assert!(
        (caret_after_home[0] - caret_settled[0]).abs() < 0.05,
        "Caret must not animate/glide between frames (immediate={}, settled={})",
        caret_after_home[0],
        caret_settled[0]
    );

    // Clicking blank space below the trailing Code block escapes Code and creates a trailing Text block
    let code_id = rt.state.editor.active.core.doc.blocks.last().unwrap().id;
    let [cx, cy, _, ch] = rect(&rt, &format!("doc-blk-{code_id}"));
    let count_before = rt.state.editor.active.core.doc.blocks.len();
    rt.pointer_down(cx + 40., cy + ch + 36.);
    rt.pointer_up(cx + 40., cy + ch + 36.);
    rt.frame(0.02);
    assert_eq!(
        rt.state.editor.active.core.doc.blocks.len(),
        count_before + 1,
        "Clicking below trailing Code block must create a trailing paragraph"
    );
    let tail_id = rt.state.editor.active.core.doc.blocks.last().unwrap().id;
    assert_ne!(tail_id, code_id);
    assert_eq!(rt.state.editor.active.core.sel.head.block, tail_id);

    // Clicking external blank space again reuses the empty trailing paragraph instead of duplicating
    rt.pointer_down(cx - 24., cy + ch + 90.);
    rt.pointer_up(cx - 24., cy + ch + 90.);
    rt.frame(0.02);
    assert_eq!(rt.state.editor.active.core.doc.blocks.len(), count_before + 1);
    type_text(&mut rt, "Escaped below code");
    assert_eq!(
        rt.state.editor.active.core.doc.blocks.last().unwrap().text.text,
        "Escaped below code"
    );

    // Slash empty search renders doc-slash-empty gracefully and Escape dismisses it
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "/");
    type_text(&mut rt, "zzzznotfound");
    rt.frame(0.02);
    assert!(rt.find("doc-slash-empty").is_some());
    press(&mut rt, Key::Escape);
    rt.frame(0.02);
    assert!(rt.state.editor.active.slash.is_none());

    // Markdown hint query `/##` filters directly to h2
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "/");
    type_text(&mut rt, "##");
    rt.frame(0.02);
    assert!(rt.find("slash-item-h2").is_some());
    press(&mut rt, Key::Escape);
    rt.frame(0.02);

    // Cmd+Enter inside Code block exits to a new paragraph directly below it
    let [code_x, code_y, _, _] = rect(&rt, &format!("doc-blk-{code_id}"));
    rt.pointer_down(code_x + 48., code_y + 42.);
    rt.pointer_up(code_x + 48., code_y + 42.);
    rt.frame(0.02);
    assert_eq!(rt.state.editor.active.core.sel.head.block, code_id);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods {
            cmd: true,
            ..Default::default()
        },
    });
    rt.frame(0.02);
    assert_ne!(rt.state.editor.active.core.sel.head.block, code_id);
}

#[test]
fn table_text_selection_starts_on_the_first_press_in_both_editors() {
    use crate::doc_core::DocBlockKind;
    for (canvas, zoom) in [(false, 1.), (true, 0.75), (true, 1.25)] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        if canvas {
            state.create_canvas_note("工作", None);
            state.note_mut().unwrap().zoom = zoom;
            state.create_at(80., 110.);
        } else {
            state.create_doc_note("工作", None);
        }
        let mut table = table_data::TableData::default();
        table.rows[0][0] = "``abcdef`` 你好".into();
        table.rows[1][0] = "first word\n第二行".into();
        let block = state.editor.active.core.doc.new_block(DocBlockKind::Table(table), "");
        let id = block.id;
        state.editor.active.core.doc.blocks = vec![block];
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        let selected = |rt: &Runtime<State>| {
            let session = &rt.state.editor.active;
            let key = &session.editing_cell.as_ref().unwrap().3;
            session.cell_inputs[key].selected().to_string()
        };
        let [left, top, _, _] = rect(&rt, &format!("doc-cell-{id}-0-0"));
        let x = left + zoom;
        let y = top + 9. * zoom;
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_move(x + 24. * zoom, y);
        rt.frame(0.016);
        rt.pointer_up(x + 24. * zoom, y);
        rt.frame(0.016);
        let dragged = selected(&rt);
        assert!(!dragged.is_empty() && "abcdef".starts_with(&dragged), "canvas={canvas}: {dragged:?}");
        rt.pointer_down(x + 24. * zoom, y);
        rt.frame(0.016);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        assert_eq!(selected(&rt), dragged);
        let [left, top, _, _] = rect(&rt, &format!("doc-cell-{id}-1-0"));
        let (x, y) = (left + 7. * zoom, top + 9. * zoom);
        for count in 1..=3 {
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.016);
            let expected = ["", "first", "first word\n"][count - 1];
            assert_eq!(selected(&rt), expected, "click {count}, canvas={canvas}, zoom={zoom}");
        }
        type_text(&mut rt, "替换");
        let DocBlockKind::Table(t) = &rt.state.editor.active.core.block(id).unwrap().kind else { panic!() };
        assert_eq!(t.rows[1][0], "替换第二行");
        assert_eq!(t.rows[0][0], "``abcdef`` 你好");
    }
}

#[test]
fn structured_table_multiline_navigation_undo_and_persistence() {
    use crate::doc_core::{DocBlockKind, DocPos};
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    state.create_doc_note("工作", None);
    let block = state
        .editor.active
        .core
        .doc
        .new_block(DocBlockKind::Table(table_data::TableData::default()), "");
    let bid = block.id;
    state.editor.active.core.doc.blocks = vec![block];
    state.editor.active.core.set_head(DocPos::new(bid, 0), false);
    let mut rt = runtime(state, (1280., 943.));
    tap(&mut rt, &format!("doc-cell-box-{bid}-0-0"));
    type_text(&mut rt, "第一行");
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "第二行");
    press(&mut rt, Key::Tab);
    assert!(
        rt.state
            .editor.active
            .editing_cell
            .as_ref()
            .is_some_and(|(_, r, c, _)| (*r, *c) == (0, 1))
    );
    let DocBlockKind::Table(t) = &rt.state.editor.active.core.block(bid).unwrap().kind else {
        panic!()
    };
    assert_eq!(t.rows[0][0], "第一行\n第二行");
    press(&mut rt, Key::Escape);
    rt.update(|s, _| {
        assert!(s.editor.active.core.undo());
        crate::doc_editor::sync_to_state(s);
    });
    let DocBlockKind::Table(t) = &rt.state.editor.active.core.block(bid).unwrap().kind else {
        panic!()
    };
    assert_eq!(t.rows[0][0], "");
    rt.update(|s, tx| crate::doc_tables::begin(s, bid, 2, 2, tx));
    rt.frame(0.02);
    press(&mut rt, Key::Tab);
    let DocBlockKind::Table(t) = &rt.state.editor.active.core.block(bid).unwrap().kind else {
        panic!()
    };
    assert_eq!(t.rows.len(), 4);
    press(&mut rt, Key::Escape);
    rt.update(|s, _| crate::doc_tables::paste(s, bid, 0, 0, "甲\t乙\n丙\t丁"));
    rt.frame(0.02);
    let expected = rt.state.note().unwrap().doc.clone();
    let id = rt.state.current.clone();
    drop(rt);
    let restored = State::from_root(f.0.clone());
    assert_eq!(
        restored.notes.iter().find(|n| n.id == id).unwrap().doc,
        expected
    );
}

#[test]
fn structured_toolbar_alignment_links_and_menu_exclusion() {
    use crate::doc_core::{DocAlign, DocBlockKind, DocPos, DocSel};
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    state.create_doc_note("工作", None);
    let b = state
        .editor.active
        .core
        .doc
        .new_block(DocBlockKind::Text, "选中文字");
    let bid = b.id;
    let len = b.text.text.len();
    state.editor.active.core.doc.blocks = vec![b];
    state.editor.active.core.sel = DocSel {
        anchor: DocPos::new(bid, 0),
        head: DocPos::new(bid, len),
    };
    state.editor.active.focused = true;
    let mut rt = runtime(state, (1280., 943.));
    rt.update(|s, _| s.editor.active.typing = false);
    rt.frame(0.02);
    assert!(rt.find("doc-sel-toolbar").is_some());
    tap(&mut rt, "tb-align");
    tap(&mut rt, "居中对齐");
    assert_eq!(
        rt.state.editor.active.core.block(bid).unwrap().align,
        DocAlign::Center
    );
    rt.update(|s, _| {
        s.editor.active.popover = Some(crate::doc_editor::DocPopover::BlockMenu { block_id: bid })
    });
    rt.frame(0.02);
    assert!(rt.find("doc-block-menu").is_some());
    assert!(rt.find("doc-sel-toolbar").is_none());
    rt.update(|s, _| s.editor.active.popover = None);
    rt.frame(0.02);
    tap(&mut rt, "tb-link");
    type_text(&mut rt, "https://example.com");
    press(&mut rt, Key::Enter);
    assert!(
        rt.state
            .editor.active
            .core
            .block(bid)
            .unwrap()
            .text
            .runs
            .iter()
            .any(|r| r.marks.link.as_deref() == Some("https://example.com"))
    );
}


#[test]
fn structured_table_region_resize_and_undo_at_minimum_window() {
    use crate::doc_core::{DocBlockKind,DocPos};
    let f=Fixture::new();let mut state=State::from_root(f.0.clone());state.choose_mode(WorkspaceMode::Document);state.create_doc_note("工作",None);
    let block=state.editor.active.core.doc.new_block(DocBlockKind::Table(table_data::TableData::default()),"");let bid=block.id;state.editor.active.core.doc.blocks=vec![block];state.editor.active.core.set_head(DocPos::new(bid,0),false);
    let mut rt=runtime(state,(1000.,736.));
    tap(&mut rt,&format!("doc-cell-box-{bid}-0-0"));press(&mut rt,Key::Escape);
    rt.physical_input(rune::PhysicalInput::Modifiers(Mods{shift:true,..Mods::default()}));
    tap(&mut rt,&format!("doc-cell-box-{bid}-2-2"));
    rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
    let sel=rt.state.editor.active.table_selection.unwrap();assert_eq!(sel.anchor,(0,0));assert_eq!(sel.head,(2,2));assert!(rt.state.editor.active.editing_cell.is_none());
    let (x,y)=rt.find(&format!("doc-table-resize-{bid}-0-0")).unwrap();rt.pointer_down(x,y);rt.frame(0.02);rt.pointer_move(x+30.,y);rt.frame(0.02);rt.pointer_up(x+30.,y);rt.frame(0.02);
    let DocBlockKind::Table(t)=&rt.state.editor.active.core.block(bid).unwrap().kind else{panic!()};assert!(t.widths[0]>t.widths[1]);
    rt.update(|s,_|{s.editor.active.core.undo();});rt.frame(0.02);
    let DocBlockKind::Table(t)=&rt.state.editor.active.core.block(bid).unwrap().kind else{panic!()};assert_eq!(t.widths,vec![1.;3]);
    tap(&mut rt,&format!("doc-table-add-col-{bid}"));
    let DocBlockKind::Table(t)=&rt.state.editor.active.core.block(bid).unwrap().kind else{panic!()};assert_eq!(t.cols(),4);
    rt.update(|s,_|crate::doc_tables::paste(s,bid,0,0,"甲\t乙\n丙\t丁"));rt.frame(0.02);
    let DocBlockKind::Table(t) = &rt.state.editor.active.core.block(bid).unwrap().kind else { panic!() };
    assert_eq!(t.rows[0][..2], ["甲", "乙"]);
    assert_eq!(t.rows[1][..2], ["丙", "丁"]);
}


#[test]
fn shared_insert_menu_matches_in_both_modes_and_accepts_chinese_trigger() {
    let (_canvas_fixture, mut canvas, canvas_id) = fresh_body();
    type_text(&mut canvas, "/");
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let mut doc = runtime(state, (1420., 1046.));
    create_same_type(&mut doc);
    press(&mut doc, Key::Enter);
    type_text(&mut doc, "/");
    let menu_keys = |rt: &Runtime<State>| -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.key)
            .filter(|k| k.starts_with("slash-item-")).collect()
    };
    assert_eq!(menu_keys(&canvas), menu_keys(&doc));
    assert_eq!(menu_keys(&canvas).len(), 20);
    assert!(menu_keys(&canvas).contains(&"slash-item-mermaid".into()));
    assert!(menu_keys(&canvas).contains(&"slash-item-math".into()));
    assert_eq!(rect(&canvas, "doc-slash-menu")[2], rect(&doc, "doc-slash-menu")[2]);
    for rt in [&mut canvas, &mut doc] {
        press(rt, Key::Backspace);
        type_text(rt, "、");
        type_text(rt, "h2");
        assert_eq!(menu_keys(rt), vec!["slash-item-h2"], "mode={:?}", rt.state.mode);
        press(rt, Key::Enter);
        type_text(rt, "同一个二级标题");
    }
    assert!(canvas.state.note().unwrap().blocks.iter().find(|b| b.id == canvas_id).unwrap().body.contains("## 同一个二级标题"));
    assert!(matches!(doc.state.editor.active.core.doc.blocks[0].kind, crate::doc_core::DocBlockKind::Heading { level: 2 }));
}

#[test]
fn canvas_slash_divider_persists_exports_and_undoes() {
    let (f, mut rt, id) = fresh_body();
    let count = rt.state.note().unwrap().blocks.len();
    type_text(&mut rt, "/"); type_text(&mut rt, "divider");
    press(&mut rt, Key::Enter);
    assert!(rt.state.editor.active.core.doc.blocks.iter().any(|b| matches!(b.kind, crate::doc_core::DocBlockKind::Divider)));
    assert_eq!(rt.state.note().unwrap().blocks.len(), count);
    assert!(rt.find(&format!("block-{id}")).is_some());
    rt.state.finish_edit(); // explicit durable boundary
    let reopened = State::from_root(f.0.clone());
    assert!(crate::canvas_document::session(&reopened,&id).unwrap().core.doc.blocks.iter()
        .any(|b| matches!(b.kind, crate::doc_core::DocBlockKind::Divider)));
    let path = f.0.join("divider.md");
    rt.state.export_markdown(&path);
    assert!(std::fs::read_to_string(path).unwrap().contains("---"));
    rt.update(|s, _| crate::canvas_document::activate(s, &id));
    command(&mut rt, 'z', false);
    assert_eq!(embedded_text(&rt.state, &id), "/divider");
}

#[test]
fn pane_dividers_resize_clamp_and_persist_in_both_modes() {
    use crate::panes::{canvas_widths, document_width, Boundary};
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1280., 943.));
    rt.frame(1.);
    let before = canvas_widths(&rt.state, 1280.);
    let (x, y) = rt.find("split-notes").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.01);
    assert_eq!(rt.state.pane_hover, Some(Boundary::Notes));
    assert_eq!(before[0], crate::navigation::RAIL_WIDTH);
    assert!(rt.find("split-folders").is_none(), "the compact rail has a fixed width");
    drag_key(&mut rt, "split-notes", 60., 0.);
    assert!((canvas_widths(&rt.state, 1280.)[1] - before[1] - 60.).abs() < 0.1);
    drag_key(&mut rt, "split-notes", 800., 0.);
    let widths = canvas_widths(&rt.state, 1000.);
    assert!(1000. - widths[0] - widths[1] >= 379.99, "{widths:?}");
    rt.update(|s, _| { s.choose_mode(WorkspaceMode::Document); s.pane_widths.documents = None; });
    rt.frame(1.);
    let before = document_width(&rt.state, 1280.);
    drag_key(&mut rt, "split-documents", 70., 0.);
    assert!((document_width(&rt.state, 1280.) - before - 70.).abs() < 0.1);
    assert!(rt.state.pane_resize.is_none());
    let reopened = State::from_root(f.0.clone());
    assert_eq!(reopened.pane_widths, rt.state.pane_widths);
    let saved = rt.state.pane_widths;
    let (x, y) = rt.find("split-documents").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x - 45., y);
    rt.frame(0.01);
    press(&mut rt, Key::Escape);
    rt.frame(0.01);
    assert_eq!(rt.state.pane_widths, saved, "cancelled resize restores the original width");
    rt.frame(1.);
    tap(&mut rt, "split-documents");
    tap(&mut rt, "split-documents");
    assert_eq!(rt.state.pane_widths.documents, None, "double click restores adaptive width");
    rt.update(|_, tx| tx.focus("split-documents", false));
    let before = document_width(&rt.state, 1280.);
    press(&mut rt, Key::Right);
    assert!((document_width(&rt.state, 1280.) - before - 16.).abs() < 0.1);
}

#[test]
fn document_selection_plate_switches_immediately_to_notes_and_folders() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let first = state.create_doc_note("工作", None);
    let second = state.create_doc_note("工作", None);
    state.select_note(&second);
    let mut rt = runtime(state, (1280., 943.));
    rt.frame(1.);
    tap(&mut rt, &format!("note-{first}"));
    rt.frame(0.001);
    assert_navigation_selected(&rt, &format!("note-{first}"));
    rt.frame(0.8);
    assert_navigation_selected(&rt, &format!("note-{first}"));
    rt.set_system_reduced_motion(true);
    tap(&mut rt, "book-1");
    assert!(crate::page_tree::children_of(&rt.state, &rt.state.book, None).is_empty());
    assert!(rt.find("editor-new-page").is_some());
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("empty-folder")));
    tap(&mut rt, "book-5");
    assert_navigation_selected(&rt, "book-5");
    let book = rt.state.book.clone();
    assert!(rt.state.note().is_none_or(|n| n.notebook == book));
    let new_id = rt.state.create_doc_note(&book, None);
    assert_eq!(rt.state.nav_focus, Some(format!("note:{new_id}")));
}

#[test]
fn navigation_mouse_release_switches_selection_across_separate_frames() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.choose_mode(mode);
        let note_target = if mode == WorkspaceMode::Document {
            let id = state.create_doc_note("工作", None);
            state.select_book("工作");
            format!("note-{id}")
        } else {
            "note-sample-4".to_owned()
        };
        let mut rt = runtime(state, (1280., 943.));
        for target in [note_target, "book-5".to_owned()] {
            rt.frame(1.);
            let (x, y) = rt.find(&target).unwrap();
            rt.pointer_move(x, y);
            rt.frame(0.02);
            rt.pointer_down(x, y);
            rt.frame(0.02);
            rt.frame(0.10);
            // Real clicks can include a tiny pointer move in the release frame.
            rt.pointer_move(x + 1., y + 1.);
            rt.pointer_up(x + 1., y + 1.);
            rt.frame(0.016);
            assert_navigation_selected(&rt, &target);
            rt.frame(0.06);
            assert_navigation_selected(&rt, &target);
            rt.frame(0.8);
            assert_navigation_selected(&rt, &target);
        }
    }
}

#[test]
fn settings_cards_fit_compact_and_wide_windows() {
    for (w, h) in [(640., 600.), (800., 600.), (1000., 736.), (1280., 943.)] {
        let f = Fixture::new();
        let mut rt = runtime(State::from_root(f.0.clone()), (w, h));
        tap(&mut rt, "settings");
        let content = rect(&rt, "settings-content");
        let rail = rect(&rt, "settings-rail");
        for key in ["notebook-style-select", "language-select", "look-color-field"] {
            let r = rect(&rt, key);
            assert!(r[0] >= content[0] - 1. && r[0] + r[2] <= content[0] + content[2] + 1., "{key} overflows at {w}");
            assert!(r[0] >= rail[2] && r[0] + r[2] < w);
        }
        tap(&mut rt, "settings-tab-trash");
        assert!(rt.find("trash-empty").is_some());
    }
}

#[test]
fn shared_navigation_search_nested_pages_and_moves_work_in_both_modes() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone()); state.choose_mode(mode);
        let mut rt = runtime(state, (1000., 736.));
        create_same_type(&mut rt); type_text(&mut rt, "导航审查父页面"); press(&mut rt, Key::Enter);
        let parent = rt.state.current.clone();
        tap(&mut rt, &format!("page-more-{parent}"));
        rt.resolve_popup(Some("new-child")).unwrap(); rt.frame(0.02);
        choose_new_page(&mut rt, mode);
        type_text(&mut rt, "导航审查子页面"); press(&mut rt, Key::Enter);
        let child = rt.state.current.clone();
        assert_eq!(rt.state.note().unwrap().parent_id.as_deref(), Some(parent.as_str()));
        assert!(rect(&rt, &format!("note-{child}"))[0] > rect(&rt, &format!("note-{parent}"))[0]);
        tap(&mut rt, &format!("tree-toggle-{parent}")); rt.frame(1.);
        assert!(rt.find(&format!("note-{child}")).is_none());
        tap(&mut rt, &format!("tree-toggle-{parent}"));
        command(&mut rt, 'k', false); type_text(&mut rt, "导航审查子页面");
        assert!(rt.find(&format!("search-result-{child}")).is_some());
        press(&mut rt, Key::Escape); assert!(!rt.state.search_open); assert!(rt.state.query.text.is_empty());
        tap(&mut rt, &format!("page-more-{parent}"));
        assert!(!format!("{:?}", rt.popup_menu().unwrap()).contains("move:"));
        rt.resolve_popup(None).unwrap(); rt.frame(0.02);
        let [x, y, w, h] = rect(&rt, &format!("note-{parent}"));
        let [bx, by, bw, bh] = rect(&rt, "book-3");
        rt.pointer_down(x + w / 2., y + h / 2.);
        rt.pointer_move(bx + bw / 2., by + bh / 2.); rt.frame(0.02);
        assert!(matches!(&rt.state.nav_drag, Some(crate::model::NavDrag::Note {
            drop_into_book: Some(target), valid_drop: true, .. }) if target == "学习"));
        rt.pointer_up(bx + bw / 2., by + bh / 2.); rt.frame(0.02);
        assert!(rt.state.notes.iter().filter(|n| n.id == parent || n.id == child).all(|n| n.notebook == "学习"));
        let fmt = rt.state.active_format();
        assert!(rt.state.notes.iter().filter(|n| n.id == parent || n.id == child).all(|n| n.format == fmt));
        tap(&mut rt, "book-3");
        assert!(rt.find(&format!("note-{parent}")).is_some());
        assert_eq!(rect(&rt, "sidebar")[2], crate::navigation::RAIL_WIDTH);
    }
}
#[test]
fn page_reordering_does_not_count_pages_from_the_other_mode() {
    let f = Fixture::new(); let mut state = State::from_root(f.0.clone());
    let canvas_ids: Vec<_> = state.notes.iter().map(|n| n.id.clone()).collect();
    state.choose_mode(WorkspaceMode::Document);
    let a = state.create_doc_note("工作", None); let b = state.create_doc_note("工作", None);
    assert!(state.reorder_note(&a, "工作", None, 0));
    let docs: Vec<_> = crate::page_tree::children_of(&state, "工作", None).iter().map(|n| n.id.clone()).collect();
    assert!(docs.iter().position(|id| id == &a) < docs.iter().position(|id| id == &b));
    let after: Vec<_> = state.notes.iter().filter(|n| n.format == crate::doc_core::NoteFormat::Canvas).map(|n| n.id.clone()).collect();
    assert_eq!(canvas_ids, after);
}

#[test]
fn navigation_header_remains_usable_in_a_narrow_canvas() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.book_names.insert("工作".into(), "非常长的工作笔记本名称".repeat(8));
    state.note_mut().unwrap().title = "非常长的页面标题".repeat(20);
    let mut rt = runtime(state, (860., 660.));
    for collapsed in [false, true] {
        rt.update(|s, _| s.sidebar_collapsed = collapsed);
        rt.frame(1.);
        for key in ["toggle-sidebar", "canvas-more"] {
            let [hx, hy, hw, hh] = rect(&rt, if key == "toggle-sidebar" && !collapsed { "pages-header" } else { "editor-toolbar" });
            let [x, y, w, h] = rect(&rt, key);
            assert!(x >= hx && x + w <= hx + hw + 1. && y >= hy && y + h <= hy + hh + 1., "{key} must stay in its toolbar");
        }
    }
}

#[test]
fn language_switch_is_persistent_and_preserves_user_content() {
    use crate::i18n::Language;
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.book_names.insert("工作".into(), "正式工作".into());
    state.store.set_preference("book-name/工作", "正式工作").unwrap();
    let mut rt = runtime(state, (1000., 736.));
    tap(&mut rt, "settings");
    rt.state.save();
    let before = rt.state.notes.clone();
    rt.update(|_,tx|tx.scroll_into_view("language-select")); rt.frame(1.); tap(&mut rt, "language-select"); rt.resolve_popup(Some("en")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.language, Language::English);
    assert_eq!(rt.state.book_name("工作"), "正式工作");
    assert_eq!(rt.state.book_name("产品"), "Product");
    assert_eq!(rt.state.tr("设置"), "Settings");
    assert_eq!(rt.state.notes, before);
    for section in ["shortcuts", "data", "trash", "appearance"] {
        tap(&mut rt, &format!("settings-tab-{section}"));
    }
    drop(rt);
    let restored = State::from_root(f.0.clone());
    assert_eq!(restored.language, Language::English);
    assert_eq!(restored.notes, before);
    let mut rt = runtime(restored, (1000., 736.));
    tap(&mut rt, "settings");
    rt.update(|_,tx|tx.scroll_into_view("language-select")); rt.frame(1.); tap(&mut rt, "language-select"); rt.resolve_popup(Some("zh-CN")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.language, Language::Chinese);
    assert_eq!(rt.state.book_name("产品"), "产品");
    assert_eq!(rt.state.book_name("工作"), "正式工作");
    assert_eq!(rt.state.notes, before);
}

#[test]
fn english_navigation_search_and_new_pages_work_in_both_modes() {
    use crate::i18n::Language;
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.choose_mode(mode);
        state.set_language(Language::English);
        let mut rt = runtime(state, (860., 660.));
        let [x, y, w, h] = rect(&rt, "open-search");
        let toolbar = rect(&rt, "pages-header");
        assert!(x >= toolbar[0] && x + w <= toolbar[0] + toolbar[2]);
        assert!(y >= toolbar[1] && y + h <= toolbar[1] + toolbar[3] && h >= 28.);
        tap(&mut rt, "open-search");
        assert!(rt.state.search_open && rt.find("search-field").is_some());
        press(&mut rt, Key::Escape);
        create_same_type(&mut rt);
        assert_eq!(rt.state.note().unwrap().title, if mode == WorkspaceMode::Canvas { "Untitled note" } else { "Untitled document" });
        for cmd in crate::insert_menu::SLASH_CMDS {
            assert!(!Language::English.text(cmd.label).is_empty());
            assert!(!Language::English.text(cmd.desc).is_empty());
        }
        let menu = crate::insert_menu::filtered("Numbered list");
        assert!(menu.iter().any(|c| c.id == "ordered"));
        tap(&mut rt, "toggle-sidebar");
        command(&mut rt, 'k', false);
        assert!(rt.state.sidebar_collapsed && rt.state.search_open);
    }
}

#[test]
fn structured_rename_updates_markdown_title_and_file_path() {
    let f=Fixture::new();let mut state=State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);state.new_note();
    let id=state.current.clone();state.begin_rename(RenameTarget::Note(id.clone()));
    state.name_input=TextEdit::new("Renamed 中文");state.end_rename(true);
    assert!(!state.document_save_failed,"{:?}",state.message);
    let note=state.store.load_note(&id).unwrap();
    assert_eq!(note.title,"Renamed 中文");assert_eq!(note.doc.unwrap().title.text,"Renamed 中文");
    let index:serde_json::Value=serde_json::from_slice(&std::fs::read(f.0.join(".echo/index.json")).unwrap()).unwrap();
    let entry=index["entries"].as_array().unwrap().iter().find(|e|e["id"]==id).unwrap();
    assert!(entry["path"].as_str().unwrap().contains("Renamed 中文"));
}

#[test]
fn window_gradient_previews_persists_and_preserves_document_content() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1280., 943.));
    let before = rt.state.notes.clone();
    tap(&mut rt, "settings");
    tap(&mut rt, "look-open");
    tap(&mut rt, "look-next");
    tap(&mut rt, "look-preset-1");
    let chosen = rt.state.sidebar_material.clone();
    assert_ne!(chosen.colors[0], chosen.colors[1]);
    assert_eq!(State::from_root(f.0.clone()).sidebar_material, chosen);
    press(&mut rt, Key::Escape);
    let [x, y, w, h] = rect(&rt, "look-color-0");
    rt.pointer_down(x + w/2., y + h/2.);
    rt.pointer_move(x + w/2. + 20., y + h/2. + 12.);
    rt.frame(0.02);
    assert_ne!(rt.state.sidebar_material.colors[0], chosen.colors[0]);
    assert_eq!(State::from_root(f.0.clone()).sidebar_material, chosen, "drag stays a preview until release");
    rt.pointer_up(x + w/2. + 20., y + h/2. + 12.);
    rt.frame(0.02);
    assert_eq!(State::from_root(f.0.clone()).sidebar_material, rt.state.sidebar_material);
    assert_eq!(rt.state.notes, before);
    assert_eq!(rt.elements().iter().find(|e|e.key.as_deref()==Some("settings-body")).unwrap().fill.a, 1.);
    rt.update(|_,tx|tx.focus("look-color-1",false));
    let old = rt.state.sidebar_material.colors[1];
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.sidebar_material.colors[1].hue, old.hue + 3);
    tap(&mut rt, "look-open");
    tap(&mut rt, "look-next");
    tap(&mut rt, "look-preset-0");
    assert_eq!(rt.state.sidebar_material.colors, crate::material::reference_colors());
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        rt.update(|s, _| s.mode = mode);
        assert_eq!(rt.state.sidebar_material.colors, crate::material::reference_colors());
    }
    assert_eq!(State::from_root(f.0.clone()).sidebar_material.colors, crate::material::reference_colors());
    assert_eq!(rt.state.notes, before);
}

#[test]
fn unified_window_keeps_editor_inset_and_reserves_navigation_traffic_lights() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        for dark in [false, true] {
            for width in [860., 1280.] {
                let f = Fixture::new();
                let mut state = State::from_root(f.0.clone());
                state.choose_mode(mode);
                state.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
                let mut rt = runtime(state, (width, 660.));
                for collapsed in [false, true] {
                    rt.update(|s,_|s.sidebar_collapsed=collapsed); rt.frame(1.);
                    let toggle = rect(&rt, "toggle-sidebar");
                    let toolbar = rect(&rt, "editor-toolbar");
                    let pane = rect(&rt, "editor-surface");
                    let body = rect(&rt, if mode == WorkspaceMode::Canvas { "canvas" } else { "canvas-pane" });
                    assert_eq!(body[1], pane[1]);
                    assert_eq!(toolbar[1], 0.);
                    assert_eq!(pane[1], crate::ui::editor_top(&rt.state));
                    let search = rect(&rt, "open-search");
                    let more = rect(&rt, if mode == WorkspaceMode::Canvas { "canvas-more" } else { "doc-more" });
                    let native = rune::Chrome { traffic_lights: Some((14., crate::ui::TRAFFIC_LIGHT_Y)), ..Default::default() }.traffic_light_rects().unwrap()[0];
                    let center = native[1] + native[3] / 2.;
                    for r in [toggle, search] {
                        assert!((r[1] + r[3] / 2. - center).abs() < 0.1, "chrome must align with native lights: {r:?}");
                        assert!(r[1] + r[3] <= crate::ui::CHROME_HEIGHT);
                    }
                    assert!(search[0] + search[2] < more[0]);
                    assert!(more[0] >= pane[0] && more[0] + more[2] <= pane[0] + pane[2]);
                    if collapsed {
                        assert!(toggle[0] + toggle[2] < search[0]);
                    } else {
                        assert!(search[0] + search[2] < toggle[0]);
                        assert!(toggle[0] + toggle[2] < pane[0]);
                        assert_eq!(pane[1], crate::ui::WINDOW_INSET);
                    }
                    assert_eq!(660. - pane[1] - pane[3], crate::ui::WINDOW_INSET);
                    assert_eq!(width - pane[0] - pane[2], crate::ui::WINDOW_INSET);
                    if collapsed {
                        assert_eq!(pane[0], crate::ui::WINDOW_INSET);
                        assert!(toggle[0] >= crate::platform::CHROME_LEADING);
                    } else {
                        let header = rect(&rt, "pages-header");
                        assert_eq!(header[1] + header[3] / 2., center);
                        assert_eq!(rect(&rt, "book-0")[1], crate::ui::CHROME_HEIGHT + 6.);
                        assert!(rt.find("new-note").is_none());
                        assert!(rt.find("editor-new-page").is_some());
                    }
                    for key in ["editor-column", "editor-toolbar", "note-list", "sidebar"] {
                        assert_eq!(rt.elements().iter().find(|e| e.key.as_deref() == Some(key)).unwrap().fill.a, 0.);
                    }
                    assert!(rt.find("canvas-breadcrumbs").is_none());
                    assert!(rt.find("arrange-blocks").is_none());
                }
            }
        }
    }
}

#[test]
fn navigation_settings_and_collapse_preserve_content_edge_and_actions() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        for width in [860., 1280.] {
            let f = Fixture::new();
            let mut state = State::from_root(f.0.clone());
            state.choose_mode(mode);
            state.language = crate::i18n::Language::English;
            let mut rt = runtime(state, (width, 760.));
            let pane_key = "editor-surface";
            let before = rect(&rt, pane_key);
            let toggle = rect(&rt, "toggle-sidebar");
            assert!(toggle[0] >= crate::navigation::RAIL_WIDTH && toggle[0] + toggle[2] < before[0]);
            assert!(toggle[2] >= 28. && toggle[3] >= 28.);
            tap(&mut rt, "settings");
            let settings = rect(&rt, "settings-body");
            assert_eq!(settings[0], before[0], "settings must keep the workspace edge");
            assert_eq!(settings[1], before[1]);
            tap(&mut rt, "settings-back");
            assert_eq!(rect(&rt, pane_key), before);
            tap(&mut rt, "toggle-sidebar");
            assert!(rt.state.sidebar_collapsed);
            rt.frame(1.);
            let restore = rect(&rt, "toggle-sidebar");
            let toolbar = rect(&rt, "editor-toolbar");
            assert!(restore[0] >= crate::platform::CHROME_LEADING && restore[1] + restore[3] <= toolbar[1] + toolbar[3]);
            tap(&mut rt, "toggle-sidebar");
            assert!(!rt.state.sidebar_collapsed);
            rt.frame(1.);
            assert_eq!(rect(&rt, pane_key), before);
            tap(&mut rt, "open-search");
            assert!(rt.find("search-field").is_some());
        }
    }
}

#[test]
fn look_popover_add_remove_modes_and_keyboard_dismiss() {
    let f=Fixture::new();
    let mut rt=runtime(State::from_root(f.0.clone()),(1280.,943.));
    tap(&mut rt,"settings");
    tap(&mut rt,"look-open");
    assert!(rt.find("look-popover").is_some());
    assert!(rt.find("look-glass").is_none());
    let [x,y,w,h]=rect(&rt,"look-popover");
    assert!(x>=0. && y>=32. && x+w<=1280. && y+h<=943.);
    tap(&mut rt,"look-preset-7");
    assert_eq!(rt.state.sidebar_material.colors.len(),1);
    press(&mut rt,Key::Escape);
    tap(&mut rt,"look-add");tap(&mut rt,"look-add");
    assert_eq!(rt.state.sidebar_material.colors.len(),3);
    let main=rt.state.sidebar_material.colors[0];
    rt.update(|s,_|s.look.chosen=1);
    tap(&mut rt,"look-remove");
    assert_eq!(rt.state.sidebar_material.colors.len(),2);
    assert_eq!(rt.state.sidebar_material.colors[0],main);
    tap(&mut rt,"look-mode-dark");
    assert!(crate::ui::nav_dark(&rt.state));
    assert_eq!(crate::ui::nav_ink(&rt.state),0xf4f4f5);
    let expected=rt.state.sidebar_material.clone();
    assert_eq!(State::from_root(f.0.clone()).sidebar_material,expected);
    assert!(!rt.state.look.open && rt.state.settings.is_some());
    press(&mut rt,Key::Escape);
    assert!(rt.state.settings.is_none());
}

#[test]
fn legacy_glass_setting_respects_the_saved_transparency() {
    let old = r#"{"transparency":75,"grain":3,"glass":true}"#;
    let material: crate::material::Settings = serde_json::from_str(old).unwrap();
    assert!((material.opacity() - 0.37).abs() < 0.001);
    assert!(serde_json::to_value(&material).unwrap().get("glass").is_none());
    assert!(crate::platform::window_options().transparent);
    assert_eq!(crate::i18n::Language::Chinese.text("透明度"), "透明度");
}

#[test]
fn legacy_two_color_preferences_migrate_and_keep_their_rgb() {
    let value:crate::material::Settings=serde_json::from_str(r#"{"transparency":7,"grain":7,"colors":[{"hue":211,"saturation":58},{"hue":185,"saturation":58}]}"#).unwrap();
    assert_eq!(value.colors.len(),2);
    for (tint,hue) in value.colors.into_iter().zip([211_f32,185.]) {
        let c=tint.color(); let h=hue/60.;let chroma=0.58*0.28; let x=chroma*(1.-(h%2.-1.).abs());
        let expected=[0.96-chroma,0.96-chroma+x,0.96].map(rune::color::srgb_decode);
        for (actual,expected) in [c.r,c.g,c.b].into_iter().zip(expected) {assert!((actual-expected).abs()<0.035,"legacy color drift");}
    }
    assert!(serde_json::from_str::<crate::material::Tint>(r#"{"hue":500,"saturation":50}"#).is_err());
    let white=crate::material::Tint::from_hex(0xffffff).color();
    assert!(white.r>0.99 && white.g>0.99 && white.b>0.99);
    let black=crate::material::Tint::from_hex(0).color();
    assert!(black.r<0.001 && black.g<0.001 && black.b<0.001);
}


fn assert_dark_surfaces(rt: &Runtime<State>) {
    for e in rt.elements() {
        if e.fill.a > 0.85 && e.rect[2] > 100. && e.rect[3] > 32. {
            let l = 0.2126 * e.fill.r + 0.7152 * e.fill.g + 0.0722 * e.fill.b;
            assert!(l < 0.25, "bright surface in dark mode: {:?} {}", e.key, e.path);
        }
    }
}

#[test]
fn rich_task_palette_tracks_appearance_and_keeps_check_contrast() {
    use crate::doc_core::DocBlockKind;
    for theme in [Theme::Light, Theme::Dark] {
        let f = Fixture::new();
        let mut s = State::from_root(f.0.clone());
        s.create_doc_note("工作", None);
        s.sidebar_material.mode = if theme.is_dark() { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
        let todo = s.editor.active.core.doc.new_block(DocBlockKind::Todo { done: true }, "待办");
        let id = todo.id;
        s.editor.active.core.doc.blocks = vec![todo];
        let mut rt = runtime(s, (1280., 943.));
        assert!(rt.find(&format!("todo-chk-{id}")).is_some());
        let mark = crate::ui::c(theme.choose(0xffffff, crate::ui::editor_colors(theme).raised));
        let fill = crate::ui::c(crate::ui::accent(&rt.state));
        let luminance = |c: rune::Color| 0.2126*c.r+0.7152*c.g+0.0722*c.b;
        let (mark,fill) = (luminance(mark),luminance(fill));
        assert!((mark.max(fill)+0.05)/(mark.min(fill)+0.05)>3.);
        tap(&mut rt, &format!("todo-chk-{id}"));
        assert!(matches!(rt.state.editor.active.core.block(id).unwrap().kind, DocBlockKind::Todo { done: false }));
    }
}

#[test]
fn dark_mode_covers_canvas_cards_settings_and_look() {
    for kind in [Kind::Text, Kind::Tasks, Kind::Code, Kind::Callout, Kind::Table, Kind::Shape] {
        let (_f, mut rt, _id) = isolated_block(kind, 1.);
        let before = rt.state.notes.clone();
        rt.update(|s, tx| { s.set_look_mode(crate::material::LookMode::Dark); tx.instant(); });
        rt.frame(0.1);
        assert_dark_surfaces(&rt);
        assert_eq!(rt.state.notes, before, "appearance must not mutate note data");
        tap(&mut rt, "settings");
        tap(&mut rt, "look-open");
        assert_dark_surfaces(&rt);
        let popover = rt.elements().into_iter().find(|e|e.key.as_deref()==Some("look-popover")).unwrap();
        assert_eq!(popover.radius, 16.);
        press(&mut rt, Key::Escape);
        for key in ["settings-tab-shortcuts", "settings-tab-data", "settings-tab-trash"] {
            tap(&mut rt, key);
            assert_dark_surfaces(&rt);
        }
    }
}

#[test]
fn appearance_follows_system_and_explicit_modes_override_it() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    rt.update(|s, _| s.set_look_mode(crate::material::LookMode::Auto));
    rt.set_system_dark(true); rt.frame(0.1);
    assert_eq!(rt.state.theme(), Theme::Dark);
    assert_dark_surfaces(&rt);
    rt.set_system_dark(false); rt.frame(0.1);
    assert_eq!(rt.state.theme(), Theme::Light);
    rt.update(|s, _| s.set_look_mode(crate::material::LookMode::Dark)); rt.frame(0.1);
    assert_eq!(rt.state.theme(), Theme::Dark);
    rt.update(|s, _| s.set_look_mode(crate::material::LookMode::Light));
    rt.set_system_dark(true); rt.frame(0.1);
    assert_eq!(rt.state.theme(), Theme::Light);
}

#[test]
fn dark_document_formatting_and_popovers_keep_contrast() {
    use crate::doc_core::{DocBlockKind, DocPos, DocSel};
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    state.create_doc_note("工作", None);
    let b=state.editor.active.core.doc.new_block(DocBlockKind::Text, "选中文字");
    let bid=b.id; let len=b.text.text.len();
    state.editor.active.core.doc.blocks=vec![b];
    state.editor.active.core.sel=DocSel {anchor:DocPos::new(bid,0),head:DocPos::new(bid,len)};
    state.editor.active.focused=true;
    state.set_look_mode(crate::material::LookMode::Dark);
    let mut rt=runtime(state,(1280.,943.));
    rt.update(|s,_|s.editor.active.typing=false); rt.frame(0.1);
    assert!(rt.find("doc-sel-toolbar").is_some());
    assert_dark_surfaces(&rt);
    rt.update(|s,_|s.editor.active.popover=Some(crate::doc_editor::DocPopover::BlockMenu{block_id:bid}));
    rt.frame(0.1); assert_dark_surfaces(&rt);
    for name in ["gray","red","orange","yellow","green","blue","purple"] {
        let fg=crate::ui::c(crate::editor_palette::text_color_hex(name,Theme::Dark).unwrap());
        let bg=crate::ui::c(crate::editor_palette::bg_color_hex(name,Theme::Dark).unwrap());
        let lum=|c:rune::Color|0.2126*c.r+0.7152*c.g+0.0722*c.b;
        assert!((lum(fg)+0.05)/(lum(bg)+0.05)>4.5,"low contrast for {name}");
    }
}


#[test]
fn dark_window_wash_preserves_pastel_chroma_instead_of_amplifying_it() {
    let mut settings=crate::material::Settings::default();
    settings.colors=vec![crate::material::Tint::from_hex(0xd7e8ff)];
    settings.mode=crate::material::LookMode::Dark;
    let tint=settings.colors[0];
    let original_chroma=crate::material::Tint::gamut(tint.lightness as f32 / 100.,(tint.hue as f32).to_radians())*tint.saturation as f32/100.;
    let c=settings.color(0,false);
    let dark=crate::material::Tint::from_linear([c.r,c.g,c.b]);
    let dark_chroma=crate::material::Tint::gamut(dark.lightness as f32 / 100.,(dark.hue as f32).to_radians())*dark.saturation as f32/100.;
    assert!(dark.lightness<=35);
    assert!((dark_chroma-original_chroma*0.6).abs()<0.004);
}

#[test]
fn custom_notebook_dialog_validates_and_persists_empty_books_and_pages() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    let count = rt.state.notes.len();
    tap(&mut rt, "new-notebook");
    press(&mut rt, Key::Enter);
    assert!(rt.state.library_dialogs.error.is_some());
    type_text(&mut rt, "工作");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.book_order.len(), 6);
    assert!(rt.state.library_dialogs.error.is_some());
    press(&mut rt, Key::Escape);
    assert!(!rt.state.library_dialogs.new_book_open);
    tap(&mut rt, "new-notebook");
    type_text(&mut rt, "研究计划");
    press(&mut rt, Key::Enter);
    assert!(!rt.state.library_dialogs.new_book_open);
    let book = rt.state.book.clone();
    assert_eq!(rt.state.book_name(&book), "研究计划");
    assert_eq!(rt.state.book_order.len(), 7);
    assert_eq!(rt.state.notes.len(), count, "creating a notebook must not create an empty note");
    drop(rt);
    let mut state = State::from_root(f.0.clone());
    assert_eq!(state.book, book);
    assert!(state.current.is_empty());
    assert_eq!(state.book_name(&book), "研究计划");
    assert!(state.move_book(6, 0));
    let id = state.create_doc_note(&book, None);
    state.save();
    drop(state);
    let restored = State::from_root(f.0.clone());
    assert_eq!(restored.book_order[0], book);
    assert_eq!(restored.book_name(&book), "研究计划");
    assert!(restored.notes.iter().any(|note| note.id == id && note.notebook == book));
}

#[test]
fn search_dialog_keyboard_opens_cross_mode_results_without_expanding_sidebar() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    let canvas = state.current.clone();
    state.note_mut().unwrap().title = "跨模式索引 Canvas".into();
    state.choose_mode(WorkspaceMode::Document);
    let doc = state.create_doc_note("产品", None);
    state.note_mut().unwrap().title = "跨模式索引 Document".into();
    state.save();
    state.sidebar_collapsed = true;
    let mut rt = runtime(state, (1000., 736.));
    command(&mut rt, 'k', false);
    assert!(rt.state.search_open);
    assert!(rt.state.sidebar_collapsed);
    type_text(&mut rt, "跨模式索引");
    let ids = crate::library_ui::search_ids(&rt.state);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&canvas) && ids.contains(&doc));
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.current, ids[1]);
    assert!(!rt.state.search_open);
    assert!(rt.state.sidebar_collapsed);
    let expected_mode = if ids[1] == canvas { WorkspaceMode::Canvas } else { WorkspaceMode::Document };
    assert_eq!(rt.state.mode, expected_mode);
    command(&mut rt, 'k', false);
    type_text(&mut rt, "不存在的搜索词");
    assert!(crate::library_ui::search_ids(&rt.state).is_empty());
    press(&mut rt, Key::Escape);
    assert!(!rt.state.search_open);
    assert!(rt.state.query.text.is_empty());
    assert_eq!(rt.state.current, ids[1]);
}

#[test]
fn notebook_delete_requires_confirmation_and_moves_both_formats_to_trash() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);
    let doc = state.create_doc_note("工作", None);
    let child = state.create_doc_note("工作", Some(doc.clone()));
    let ids: Vec<_> = state.notes.iter().filter(|n| n.notebook == "工作").map(|n| n.id.clone()).collect();
    assert!(state.notes.iter().any(|n| n.notebook == "工作" && n.format == crate::doc_core::NoteFormat::Canvas));
    let mut rt = runtime(state, (1000., 736.));
    let open = |rt: &mut Runtime<State>| {
        let slot = rt.state.book_order.iter().position(|b| b == "工作").unwrap();
        let (x,y) = rt.find(&format!("book-{slot}")).unwrap();
        rt.secondary_down(x,y); rt.secondary_up(x,y); rt.frame(0.02);
        rt.resolve_popup(Some("delete")).unwrap(); rt.frame(0.02);
    };
    open(&mut rt);
    assert!(rt.find("delete-notebook-dialog").is_some());
    assert!(rt.state.book_order.iter().any(|b| b == "工作"));
    press(&mut rt, Key::Enter); // Default focus is Cancel, never the destructive action.
    assert!(rt.state.library_dialogs.delete_book.is_none());
    assert!(ids.iter().all(|id| rt.state.notes.iter().any(|n| &n.id == id && n.notebook == "工作")));
    open(&mut rt); press(&mut rt, Key::Escape);
    assert!(rt.state.book_order.iter().any(|b| b == "工作"));
    open(&mut rt); tap(&mut rt, "delete-notebook-confirm");
    assert!(rt.state.library_dialogs.delete_book.is_none());
    assert!(!rt.state.book_order.iter().any(|b| b == "工作"));
    assert_ne!(rt.state.book, "工作");
    assert!(ids.iter().all(|id| rt.state.notes.iter().any(|n| &n.id == id && n.notebook == "回收站" && n.original_book == "工作")));
    assert!(rt.state.undo.is_empty());
    drop(rt);
    let mut restored = State::from_root(f.0.clone());
    assert!(!restored.book_order.iter().any(|b| b == "工作"));
    assert_eq!(restored.notes.iter().find(|n| n.id == child).unwrap().parent_id.as_deref(), Some(doc.as_str()));
    restored.restore_note_by_id(&child);
    assert!(restored.book_order.iter().any(|b| b == "工作"));
    assert_eq!(restored.notes.iter().find(|n| n.id == doc).unwrap().notebook, "工作");
    restored.restore_note_by_id(&child);
    assert_eq!(restored.notes.iter().find(|n| n.id == child).unwrap().parent_id.as_deref(), Some(doc.as_str()));
}

#[test]
fn deleting_last_notebook_keeps_an_empty_library_after_relaunch() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    let original_count = state.notes.len();
    for book in state.book_order.clone() { state.delete_notebook(&book).unwrap(); }
    assert!(state.book_order.is_empty());
    assert!(state.current.is_empty() && state.book.is_empty());
    assert_eq!(state.notes.len(), original_count);
    drop(state);
    let mut state = State::from_root(f.0.clone());
    assert!(state.book_order.is_empty());
    assert!(state.current.is_empty() && state.book.is_empty());
    state.empty_all_trash();
    drop(state);
    let state = State::from_root(f.0.clone());
    assert!(state.notes.is_empty(), "deleting all content must not reseed sample notes");
    assert!(state.book_order.is_empty());
    let mut rt = runtime(state, (1000.,736.));
    assert!(rt.find("empty-new-notebook").is_some());
    command(&mut rt, 'n', false);
    assert!(rt.state.library_dialogs.new_book_open);
    type_text(&mut rt, "重新开始"); press(&mut rt, Key::Enter);
    assert_eq!(rt.state.book_order.len(), 1);
    create_same_type(&mut rt); type_text(&mut rt, "新的记录"); press(&mut rt, Key::Enter);
    assert_eq!(rt.state.notes.len(), 1);
    assert_eq!(rt.state.note().unwrap().title, "新的记录");
}

#[test]
fn deleted_custom_notebook_name_survives_restart_and_restore() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000.,736.));
    tap(&mut rt,"new-notebook"); type_text(&mut rt,"阅读摘记"); press(&mut rt,Key::Enter);
    let book = rt.state.book.clone();
    create_same_type(&mut rt); type_text(&mut rt,"摘录一"); press(&mut rt,Key::Enter);
    let id = rt.state.current.clone();
    rt.state.delete_notebook(&book).unwrap();
    drop(rt);
    let mut state = State::from_root(f.0.clone());
    state.set_language(crate::i18n::Language::English);
    assert_eq!(state.book_name(&book), "阅读摘记");
    assert!(!state.book_order.contains(&book));
    state.restore_note_by_id(&id);
    assert!(state.book_order.contains(&book));
    assert_eq!(state.notes.iter().find(|n| n.id == id).unwrap().notebook,book);
    drop(state);
    let restored = State::from_root(f.0.clone());
    assert!(restored.book_order.contains(&book));
    assert_eq!(restored.book_name(&book),"阅读摘记");
}

#[test]
fn notebook_deletion_save_conflict_keeps_notebook_and_content() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    let before = state.notes.clone();
    let path = f.0.join(".echo/settings.json");
    std::fs::write(&path, "{\"external-change\":\"1\"}").unwrap();
    assert!(state.delete_notebook("工作").is_err());
    assert!(state.book_order.iter().any(|b| b == "工作"));
    assert_eq!(state.notes, before);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{\"external-change\":\"1\"}");
}

#[test]
fn notebook_icon_style_switches_in_both_modes_and_persists_without_changing_notes() {
    use crate::notebook_icons::Style;
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    assert_eq!(state.notebook_icon_style, Style::Solid);
    assert_eq!(state.sidebar_material.colors, crate::material::PAPER_COLORS.map(crate::material::Tint::from_hex));
    assert_eq!(crate::ui::page_surface(&state), 0xffffff);
    state.choose_mode(WorkspaceMode::Document);
    state.finish_edit();
    let before = state.notes.clone();
    let order = state.book_order.clone();
    let mut rt = runtime(state,(1000.,800.));
    tap(&mut rt,"settings");
    rt.update(|_,tx|tx.scroll_into_view("notebook-style-select")); rt.frame(1.); tap(&mut rt,"notebook-style-select"); rt.resolve_popup(Some("color")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.notebook_icon_style,Style::Color);
    tap(&mut rt,"notebook-style-select");
    rt.resolve_popup(Some("solid")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.notebook_icon_style,Style::Solid);
    tap(&mut rt,"notebook-style-select");
    rt.resolve_popup(Some("ink")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.notebook_icon_style,Style::Ink);
    tap(&mut rt,"notebook-style-select");
    rt.resolve_popup(Some("journal")).unwrap(); rt.frame(0.02);
    assert_eq!(rt.state.notebook_icon_style, Style::Journal);
    tap(&mut rt, "notebook-style-select");
    rt.resolve_popup(Some("monochrome")).unwrap();
    rt.frame(0.02);
    rt.update(|s,_| s.choose_mode(WorkspaceMode::Canvas));
    assert_eq!(rt.state.notebook_icon_style,Style::Monochrome);
    rt.update(|s,_| s.choose_mode(WorkspaceMode::Document));
    assert_eq!(rt.state.notebook_icon_style,Style::Monochrome);
    assert_eq!(rt.state.notes,before);
    assert_eq!(rt.state.book_order,order);
    drop(rt);
    let state = State::from_root(f.0.clone());
    assert_eq!(state.notebook_icon_style,Style::Monochrome);
    let mut rt = runtime(state,(860.,660.));
    tap(&mut rt,"settings");
    // The setting can live below the fold in compact windows; selecting it still persists.
    rt.update(|_, tx| tx.scroll_into_view("notebook-style-select"));
    rt.frame(1.);
    tap(&mut rt, "notebook-style-select");
    rt.resolve_popup(Some("pals")).unwrap();
    rt.frame(0.02);
    assert_eq!(rt.state.store.preference("notebook_icon_style").as_deref(), Some("pals"));
    drop(rt);
    let state = State::from_root(f.0.clone());
    assert_eq!(state.notebook_icon_style, Style::Pals);
    state.store.set_preference("notebook_icon_style", "paper").unwrap();
    drop(state);
    assert_eq!(State::from_root(f.0.clone()).notebook_icon_style, Style::Pals);
}

#[test]
fn look_single_color_stays_solid_in_light_dark_and_transparency() {
    use crate::material::{LookMode, Settings, Tint};
    for rgb in [0xffffff, 0x000000, 0x808080, 0x74bfd6, 0xec8fbb] {
        let mut material = Settings { colors: vec![Tint::from_hex(rgb)], ..Settings::default() };
        for mode in [LookMode::Light, LookMode::Dark] {
            material.mode = mode;
            let opaque = material.surface_stops(false);
            assert_eq!(opaque.len(), 2);
            assert_eq!(opaque[0].1, opaque[1].1, "one color must not introduce artificial light and dark bands");
            material.transparency = 70;
            for ((_, before), (_, after)) in opaque.iter().zip(material.surface_stops(false)) {
                assert_eq!([before.r, before.g, before.b], [after.r, after.g, after.b]);
                assert!((after.a - 0.412).abs() < 0.001);
            }
            material.transparency = 0;
        }
        let dark = material.surface_stops(false);
        material.mode = LookMode::Auto;
        assert_eq!(material.surface_stops(true), dark);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_window_material_follows_appearance_without_rewriting_saved_colors() {
    let f = Fixture::new();
    let mut state = State::from_root(f.0.clone());
    state.sidebar_material.mode = crate::material::LookMode::Auto;
    state.sidebar_material.transparency = 38;
    let saved = state.sidebar_material.clone();
    let mut rt = runtime(state, (1280., 943.));
    let mut palettes = Vec::new();
    for dark in [true, false, true] {
        rt.set_system_dark(dark);
        rt.frame(1.);
        let rune::SystemMaterial::WindowBackdrop(native) = rt.state.sidebar_material.native_backdrop(dark) else { panic!("native window material") };
        assert_eq!(native.is_dark(), dark);
        assert!((native.opacity() - 0.62).abs() < 0.0001);
        assert!(!native.is_glass());
        palettes.push(native.colors());
        assert_eq!(rt.state.sidebar_material, saved);
        assert!(rt.find("window-material-veil").is_none(), "the veil belongs to AppKit, not Metal");
    }
    assert_eq!(palettes[0], palettes[2]);
    assert_ne!(palettes[0], palettes[1]);
}

#[cfg(target_os = "macos")]
#[test]
fn window_material_does_not_paint_a_second_coating_in_metal() {
    for dark in [false, true] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.sidebar_material.mode = if dark {crate::material::LookMode::Dark} else {crate::material::LookMode::Light};
        let mut rt = runtime(state, (1280., 943.));
        let no_coating = |rt: &Runtime<State>| {
            assert!(!rt.instances.iter().any(|i| i.params[1] == rune::render::KIND_RADIAL && i.rect == [0.,0.,1280.,943.]), "native colors must not be composited a second time by Metal");
        };
        rt.frame(1.); no_coating(&rt);
        rt.update(|s,tx| {s.pane_widths.documents=Some(440.);tx.instant();}); rt.frame(1.);
        no_coating(&rt);
        for mode in [WorkspaceMode::Document, WorkspaceMode::Canvas] {
            rt.update(move|s,tx| {s.mode=mode;tx.instant();});rt.frame(1.);
            for key in ["sidebar", "note-list", "pages-header"] {
                let e=rt.elements().into_iter().find(|e|e.key.as_deref()==Some(key)).unwrap();
                assert_eq!(e.fill.a,0.,"{key} must reveal the same native coating");
            }
            no_coating(&rt);
        }
        tap(&mut rt,"settings");rt.frame(1.);no_coating(&rt);
    }
}

#[test]
fn sidebar_toggle_animates_and_reverses_without_remounting_the_editor() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.choose_mode(mode);
        let mut rt = runtime(state, (1280., 943.));
        rt.frame(1.);
        let pane = if mode == WorkspaceMode::Canvas { "canvas" } else { "canvas-pane" };
        let before = rect(&rt, pane);
        let notes = rt.state.notes.clone();
        tap(&mut rt, "toggle-sidebar");
        rt.frame(0.05);
        let closing = rect(&rt, pane);
        assert!(closing[0] > 0. && closing[0] < before[0], "closing must be mid-flight: {before:?} -> {closing:?}");
        assert!(rt.animating);
        // Reverse with the same shortcut users can press while the button is moving.
        command(&mut rt, '\\', false);
        rt.frame(0.04);
        let reversing = rect(&rt, pane);
        assert!(reversing[0] > 0. && reversing[0] < before[0], "reversal must not snap: {reversing:?}");
        for _ in 0..30 { rt.frame(0.02); }
        assert!((rect(&rt, pane)[0] - before[0]).abs() < 0.1);
        tap(&mut rt, "toggle-sidebar");
        for _ in 0..30 { rt.frame(0.02); }
        assert!((rect(&rt, pane)[0] - crate::ui::WINDOW_INSET).abs() < 0.1);
        let hidden = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("settings")).unwrap();
        assert!(hidden.visible[2] < 0.1, "hidden controls must be clipped");
        rt.update(|_, tx| tx.focus("settings", false));
        rt.frame(0.01);
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("settings") && e.focused), "hidden controls must not take focus");
        tap(&mut rt, "toggle-sidebar");
        rt.frame(0.05);
        let opening = rect(&rt, pane);
        assert!(opening[0] > 0. && opening[0] < before[0]);
        for _ in 0..30 { rt.frame(0.02); }
        assert!((rect(&rt, pane)[0] - before[0]).abs() < 0.1);
        assert_eq!(rt.state.notes, notes);
    }
}

#[test]
fn search_panel_animates_both_directions_and_keeps_typing_layout_stable() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        for dark in [false, true] {
            let f = Fixture::new();
            let mut state = State::from_root(f.0.clone());
            state.choose_mode(mode);
            state.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
            let mut rt = runtime(state, (1000., 736.));
            rt.frame(1.);
            command(&mut rt, 'k', false);
            rt.frame(0.04);
            let entering = rt.elements().into_iter().find(|e|e.key.as_deref()==Some("library-search-dialog")).unwrap();
            assert!(entering.opacity > 0. && entering.opacity < 1.);
            rt.frame(0.6);
            let settled = rect(&rt, "library-search-dialog");
            assert!((entering.rect[1] - settled[1]).abs() > 0.1);
            assert!(settled[0] >= 0. && settled[1] >= 32. && settled[0]+settled[2]<=1000. && settled[1]+settled[3]<=736.);
            type_text(&mut rt, "no matching document 74631");
            assert_eq!(rect(&rt, "library-search-dialog"), settled);
            assert!(crate::library_ui::search_ids(&rt.state).is_empty());
            press(&mut rt, Key::Escape);
            rt.frame(0.03);
            let exiting = rt.elements().into_iter().find(|e|e.key.as_deref()==Some("library-search-dialog")).unwrap();
            assert!(exiting.exiting && exiting.opacity > 0. && exiting.opacity < 1.);
            rt.frame(0.6);
            assert!(rt.find("library-search-dialog").is_none());
            command(&mut rt, 'k', false);
            rt.frame(0.6);
            tap(&mut rt, "search-close");
            assert!(!rt.state.search_open);
            rt.frame(0.6);
            command(&mut rt, 'k', false);
            rt.frame(0.6);
            rt.pointer_down(12., 360.); rt.pointer_up(12., 360.); rt.frame(0.03);
            assert!(!rt.state.search_open, "clicking outside dismisses the modal");
        }
    }
}

#[test]
fn page_drag_tracks_pointer_without_reflow_and_cancels_outside_or_on_escape() {
    for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone()); state.choose_mode(mode);
        let mut rt = runtime(state, (1000., 736.));
        create_same_type(&mut rt); type_text(&mut rt, "Drag A"); press(&mut rt, Key::Enter);
        let a = rt.state.current.clone();
        create_same_type(&mut rt); type_text(&mut rt, "Drag B"); press(&mut rt, Key::Enter);
        let b = rt.state.current.clone(); rt.frame(1.);
        let before = rt.state.notes.clone();
        let ar = rect(&rt, &format!("note-{a}"));
        let br = rect(&rt, &format!("note-{b}"));
        let x = ar[0] + ar[2] / 2.;
        rt.pointer_down(x, ar[1] + ar[3] / 2.);
        rt.pointer_move(x, br[1] + 2.); rt.frame(0.02);
        assert_eq!(rect(&rt, &format!("note-{a}")), ar);
        assert_eq!(rect(&rt, &format!("note-{b}")), br);
        assert_eq!(rect(&rt, "nav-drop-indicator")[3], 2.);
        let ghost = rect(&rt, "nav-drag-ghost");
        assert!((ghost[0] - x - 14.).abs() < 1.);
        rt.pointer_move(800., 400.); rt.frame(0.02);
        assert!(rect(&rt, "nav-drag-ghost")[0] > ghost[0]);
        assert!(matches!(rt.state.nav_drag, Some(crate::model::NavDrag::Note { valid_drop: false, .. })));
        assert!(rt.find("nav-drop-indicator").is_none());
        rt.pointer_up(800., 400.); rt.frame(0.02);
        assert_eq!(rt.state.notes, before);
        assert!(rt.find("nav-drag-ghost").is_none());
        rt.pointer_down(x, ar[1] + ar[3] / 2.);
        rt.pointer_move(x, br[1] + 2.); rt.frame(0.02);
        press(&mut rt, Key::Escape);
        rt.pointer_move(x + 5., br[1] + 3.); rt.frame(0.02);
        assert!(rt.state.nav_drag.is_none());
        rt.pointer_up(x + 5., br[1] + 3.); rt.frame(0.02);
        assert_eq!(rt.state.notes, before);
    }
}


#[test]
fn mixed_page_tree_reorders_moves_subtrees_and_restores_file_types() {
    use crate::doc_core::NoteFormat;
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let canvas = s.current.clone();
    let doc = s.create_doc_note("工作", None);
    let child = s.create_canvas_note("工作", Some(doc.clone()));
    s.end_rename(true);
    let grandchild = s.create_doc_note("工作", Some(child.clone()));
    assert!(crate::page_tree::children_of(&s, "工作", Some(&doc)).iter().any(|n| n.id == child));
    assert!(crate::page_tree::children_of(&s, "工作", Some(&child)).iter().any(|n| n.id == grandchild));
    assert!(!s.reorder_note(&doc, "工作", Some(grandchild.clone()), 0), "cycles are rejected across file types");
    assert!(s.reorder_note(&canvas, "工作", None, 0));
    assert_eq!(crate::page_tree::children_of(&s, "工作", None)[0].id, canvas);
    assert!(s.move_note_to_book(&doc, "学习"));
    assert_eq!(s.current, grandchild);
    assert_eq!(s.book, "学习", "moving an ancestor keeps its selected descendant in view");
    for id in [&doc, &child, &grandchild] { assert_eq!(s.notes.iter().find(|n| &n.id == id).unwrap().notebook, "学习"); }
    s.select_note(&child);
    assert_eq!(s.mode, WorkspaceMode::Canvas);
    s.store.set_preference("workspace_mode", "document").unwrap();
    drop(s);
    let mut s = State::from_root(f.0.clone());
    assert_eq!(s.current, child);
    assert_eq!(s.mode, WorkspaceMode::Canvas, "legacy global mode cannot override a file's type");
    assert_eq!(s.note().unwrap().format, NoteFormat::Canvas);
    s.select_note(&grandchild);
    assert_eq!(s.mode, WorkspaceMode::Document);
    assert_eq!(s.note().unwrap().parent_id.as_deref(), Some(child.as_str()));
}

#[test]
fn new_page_dialog_keyboard_works_in_english_and_dark_appearance() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.set_language(crate::i18n::Language::English);
    s.sidebar_material.mode = crate::material::LookMode::Dark;
    let mut rt = runtime(s, (800., 600.));
    command(&mut rt, 'n', false);
    assert!(rt.find("new-page-dialog").is_some());
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.mode, WorkspaceMode::Document);
    assert!(rt.state.library_dialogs.new_page.is_none());
    type_text(&mut rt, "Keyboard document");
    assert_eq!(rt.state.note().unwrap().title, "Keyboard document");
    command(&mut rt, 'n', false);
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.mode, WorkspaceMode::Canvas);
    type_text(&mut rt, "Keyboard canvas");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.note().unwrap().title, "Keyboard canvas");
}

#[test]
#[ignore = "explicit ECHO_LIBRARY_AUDIT_SOURCE; opens an isolated copy, never the source library"]
fn existing_library_survives_unified_navigation_byte_for_byte() {
    use std::collections::BTreeMap;
    fn files(root: &std::path::Path, dir: &std::path::Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { files(root, &path, out); }
            else { out.insert(path.strip_prefix(root).unwrap().to_owned(), std::fs::read(path).unwrap()); }
        }
    }
    let source = PathBuf::from(std::env::var("ECHO_LIBRARY_AUDIT_SOURCE").expect("pass an explicit backup path"));
    let mut before = BTreeMap::new();
    files(&source, &source, &mut before);
    let f = Fixture(std::env::temp_dir().join(format!("echo-existing-library-audit-{}", uid())));
    for (path, bytes) in &before {
        let target = f.0.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
    let store = crate::storage::Store::open(f.0.clone()).unwrap();
    let originals = store.load().unwrap();
    drop(store);
    let mut s = State::from_root(f.0.clone());
    assert_eq!(s.notes, originals, "startup preserves all note models, including trash");
    for original in originals.iter().filter(|n| n.notebook != "回收站") {
        s.select_note(&original.id);
        assert_eq!(s.current, original.id);
        assert_eq!(s.mode, WorkspaceMode::from_format(original.format));
        assert_eq!(s.note().unwrap(), original);
        assert!(!s.document_save_failed);
    }
    s.finish_edit();
    drop(s);
    let reopened = State::from_root(f.0.clone());
    assert_eq!(reopened.notes, originals);
    let mut after = BTreeMap::new(); files(&f.0, &f.0, &mut after);
    let content = |path: &PathBuf| path.extension().is_some_and(|ext| ext == "md")
        || path.to_string_lossy().ends_with(".canvas.json") || path.starts_with("images");
    let original_content: BTreeMap<_,_> = before.iter().filter(|(p,_)| content(p)).collect();
    let resulting_content: BTreeMap<_,_> = after.iter().filter(|(p,_)| content(p)).collect();
    assert_eq!(original_content.keys().collect::<Vec<_>>(), resulting_content.keys().collect::<Vec<_>>(), "content file list is unchanged");
    for (path, bytes) in &original_content {
        assert!(*bytes == resulting_content[path], "content bytes changed: {}", path.display());
    }
    eprintln!("Verified {} notes and {} content files on an isolated copy", originals.len(), original_content.len());
}


fn assert_overlay_entering(rt: &mut Runtime<State>, key: &str) {
    rt.frame(0.035);
    let entering = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).expect(key);
    assert!(entering.opacity > 0. && entering.opacity < 1., "{key} must enter progressively: {}", entering.opacity);
    let y = entering.rect[1];
    rt.frame(0.5);
    let settled = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap();
    assert!((settled.opacity - 1.).abs() < 0.01, "{key} must reach full opacity");
    assert!((settled.rect[1] - y).abs() > 0.1, "{key} has a restrained entrance offset");
}
fn assert_overlay_exiting(rt: &mut Runtime<State>, key: &str) {
    rt.frame(0.025);
    let exiting = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).expect(key);
    assert!(exiting.exiting && exiting.inert && exiting.opacity > 0. && exiting.opacity < 1., "{key} exits visibly but cannot intercept input: {:?}", (exiting.exiting,exiting.inert,exiting.opacity));
    rt.frame(0.5);
    assert!(rt.elements().iter().all(|e| e.key.as_deref() != Some(key)), "{key} must unmount after exit");
}

#[test]
fn all_library_dialogs_and_look_animate_open_close_and_remain_keyboard_safe() {
    for dark in [false, true] {
        let f = Fixture::new(); let mut s = State::from_root(f.0.clone());
        s.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
        let mut rt = runtime(s, (1000.,736.)); rt.frame(1.);
        let before = rt.state.notes.clone();
        for key in ["new-page-dialog", "new-notebook-dialog", "delete-notebook-dialog", "library-search-dialog"] {
            match key {
                "new-page-dialog" => command(&mut rt, 'n', false),
                "new-notebook-dialog" => tap(&mut rt, "new-notebook"),
                "delete-notebook-dialog" => rt.update(|s,tx| crate::library_ui::open_delete_notebook(s, &s.book.clone(), tx)),
                _ => command(&mut rt, 'k', false),
            }
            assert_overlay_entering(&mut rt, key);
            if dark { assert_dark_surfaces(&rt); }
            if key == "new-notebook-dialog" {
                let outer = rect(&rt,"notebook-name-field"); let inner = rect(&rt,"notebook-name");
                assert!((outer[1]+outer[3]/2.-inner[1]-inner[3]/2.).abs()<1., "input aligns vertically");
            }
            if key == "delete-notebook-dialog" { press(&mut rt,Key::Enter); } else { press(&mut rt,Key::Escape); }
            assert_overlay_exiting(&mut rt,key);
            assert_eq!(rt.state.notes,before,"cancelling overlays never mutates documents");
        }
        tap(&mut rt,"settings"); rt.frame(1.);
        // The material studio is embedded; on compact windows its palette button is below the fold.
        rt.update(|_, tx| tx.scroll_to_instant("settings-appearance-scroll", 0., 400.));
        rt.frame(0.1);
        tap(&mut rt,"look-open");
        assert_overlay_entering(&mut rt,"look-popover");
        press(&mut rt,Key::Escape); assert_overlay_exiting(&mut rt,"look-popover");
        assert!(rt.state.settings.is_some());
        rt.set_system_reduced_motion(true); tap(&mut rt,"look-open"); rt.frame(0.02);
        let pop = rt.elements().into_iter().find(|e|e.key.as_deref()==Some("look-popover")).unwrap();
        assert!(pop.opacity > 0. && pop.opacity < 1.);
        let reduced_rect = pop.rect; rt.frame(0.5);
        assert_eq!(rect(&rt,"look-popover"), reduced_rect, "reduced motion keeps the fade but removes travel");
        press(&mut rt,Key::Escape); assert_overlay_exiting(&mut rt,"look-popover");
    }
}

#[test]
fn document_overlay_family_animates_and_stays_inside_editor() {
    use crate::doc_core::{DocBlockKind as K, DocDocument, DocPos, DocSel};
    use crate::doc_editor::DocPopover as P;
    let mut doc = DocDocument::empty();
    let text = doc.new_block(K::Text,"A sentence to format."); let bid=text.id;
    let code = doc.new_block(K::Code{lang:"Rust".into(),wrap:true},"let x = 1;"); let cid=code.id;
    let callout = doc.new_block(K::Callout{emoji:"💡".into(),palette:"orange".into()},"An idea"); let aid=callout.id;
    doc.blocks=vec![text,code,callout];
    let (_f,mut rt)=layout_test_runtime(doc,(1000.,736.)); rt.frame(1.);
    rt.update(|s,tx|{s.editor.active.core.sel=DocSel{anchor:DocPos::new(bid,0),head:DocPos::new(bid,8)};tx.animated();});
    assert_overlay_entering(&mut rt,"doc-sel-toolbar");
    for (pop,key) in [
        (P::BlockMenu{block_id:bid},"doc-block-menu"), (P::HeadingMenu,"doc-heading-popover"),
        (P::ColorMenu,"doc-color-popover"), (P::AlignMenu,"doc-align-menu"),
        (P::MoreMenu,"doc-more-menu"), (P::LinkMenu,"doc-link-menu"),
        (P::CodeLang{block_id:cid},"doc-codelang-popover"), (P::CalloutStyle{block_id:aid},"doc-callout-popover"),
    ] {
        rt.update(move |s,tx|{s.editor.active.popover=Some(pop.clone());tx.animated();});
        assert_overlay_entering(&mut rt,key);
        let panel=rect(&rt,key); let viewport=rect(&rt,"doc-editor-stage");
        assert!(panel[0]>=viewport[0] && panel[1]>=viewport[1] && panel[0]+panel[2]<=1000. && panel[1]+panel[3]<=736.,"{key} fits: {panel:?}");
        press(&mut rt,Key::Escape); assert_overlay_exiting(&mut rt,key);
    }
    rt.update(|s,tx|{s.editor.active.core.sel=DocSel::caret(DocPos::new(bid,0)); tx.animated();});
    assert_overlay_exiting(&mut rt,"doc-sel-toolbar");
    rt.update(|s,tx|{s.message=Some("Changes saved".into());tx.animated();});
    assert_overlay_entering(&mut rt,"message"); tap(&mut rt,"message-close"); assert_overlay_exiting(&mut rt,"message");
}


#[test]
fn material_studio_preserves_settings_and_validates_custom_colors() {
    let f=Fixture::new(); let mut rt=runtime(State::from_root(f.0.clone()),(1280.,943.));
    let notes=rt.state.notes.clone();
    rt.update(|s,tx|{s.sidebar_material.grain=17;s.sidebar_material.transparency=24;s.set_look_mode(crate::material::LookMode::Dark);tx.instant();});
    tap(&mut rt,"settings"); tap(&mut rt,"material-paper");
    assert_eq!(rt.state.sidebar_material.grain,17);
    assert_eq!(rt.state.sidebar_material.transparency,24);
    assert_eq!(rt.state.sidebar_material.colors,[0xf4f5f5,0xfbfaf8].map(crate::material::Tint::from_hex));
    assert_eq!(State::from_root(f.0.clone()).sidebar_material,rt.state.sidebar_material);
    assert_dark_surfaces(&rt);
    let original=rt.state.sidebar_material.clone();
    tap(&mut rt,"look-chip-0");
    rt.update(|s,tx|{s.look.color_input=rune::TextEdit::new("#bad-value");tx.focus("look-hex-0",false);});
    press(&mut rt,Key::Enter);
    assert!(rt.state.look.color_error);
    assert_eq!(rt.state.sidebar_material,original,"invalid input cannot change persisted material");
    assert!(rt.elements().iter().any(|e|e.key.as_deref()==Some("look-color-error")));
    press(&mut rt,Key::Backspace);
    assert!(!rt.state.look.color_error,"editing clears the stale validation message");
    rt.update(|s,tx|{s.look.color_input=rune::TextEdit::new("#DCE8F3");tx.focus("look-hex-0",false);});
    press(&mut rt,Key::Enter);
    assert!(!rt.state.look.color_error);
    assert_eq!(rt.state.sidebar_material.colors[0],crate::material::Tint::from_hex(0xdce8f3));
    assert_eq!(State::from_root(f.0.clone()).sidebar_material,rt.state.sidebar_material);
    tap(&mut rt,"look-chip-1"); press(&mut rt,Key::Escape);
    assert!(rt.state.settings.is_some(),"Escape from hex editing only cancels that editor");
    assert!(rt.state.look.editing_color.is_none());
    tap(&mut rt,"look-add"); assert_eq!(rt.state.sidebar_material.colors.len(),3);
    let colors=rt.state.sidebar_material.colors.clone();tap(&mut rt,"look-add");assert_eq!(rt.state.sidebar_material.colors,colors);
    tap(&mut rt,"look-reset");
    assert_eq!(rt.state.sidebar_material,crate::material::Settings{mode:crate::material::LookMode::Dark,..Default::default()});
    assert_eq!(rt.state.notes,notes);
}

#[test]
fn material_studio_controls_fit_both_languages_and_dark_light() {
    for width in [640.,1000.,1280.,1440.] {
        for english in [false,true] {
            for dark in [false,true] {
                let f=Fixture::new();let mut s=State::from_root(f.0.clone());
                // Exercise all three color inputs even though the default uses two colors.
                s.sidebar_material.colors=crate::material::reference_colors();
                s.language=if english {crate::i18n::Language::English}else{crate::i18n::Language::Chinese};
                s.sidebar_material.mode=if dark{crate::material::LookMode::Dark}else{crate::material::LookMode::Light};
                let mut rt=runtime(s,(width,943.));tap(&mut rt,"settings");rt.frame(1.);
                let content=rect(&rt,"settings-content");
                for key in ["material-gallery","material-porcelain","material-warm-white","material-dawn","material-mist","material-custom","look-field-drag","look-chip-2","look-add","look-mode-auto","language-select","notebook-style-select"] {
                    let r=rect(&rt,key);
                    assert!(r[0]>=content[0]-1. && r[0]+r[2]<=content[0]+content[2]+1.,"{width}/{english}/{dark}: {key} {r:?} outside {content:?}");
                }
                let studio=rect(&rt,"material-studio");
                let more=rect(&rt,"look-open");
                assert!((more[0]-studio[0]).abs()<1.,"More colors belongs at the left content edge");
                for i in 0..3 {
                    let chip_key=format!("look-chip-{i}");
                    rt.update(|_,tx|tx.scroll_into_view(chip_key.clone()));rt.frame(1.);
                    let before=rect(&rt,&chip_key);
                    let value=rt.state.sidebar_material.colors[i].color().hex()[..7].to_uppercase();
                    tap(&mut rt,&chip_key);rt.frame(0.2);
                    let input=rt.elements().into_iter().find(|e|e.key.as_deref()==Some(&format!("look-hex-{i}"))).unwrap_or_else(||panic!("missing editor: {width}/{english}/{dark}/{i}"));
                    assert_eq!(input.text.as_deref(),Some(value.as_str()));
                    assert_eq!(input.text_color,Some(crate::ui::c(crate::ui::theme_ink(rt.state.theme()))),"HEX text must remain readable after entering edit mode");
                    assert_eq!(rect(&rt,&chip_key),before,"editing must not resize the color chip");
                    assert!(input.rect[0]>=before[0] && input.rect[0]+input.rect[2]<=before[0]+before[2]);
                    assert!(input.rect[2]>=55.,"six hex digits and # must fit: {width}/{english}/{dark}/{i}: {:?}",input.rect);
                    press(&mut rt,Key::Escape);
                }
                rt.update(|_,tx|tx.scroll_into_view("look-open"));rt.frame(1.);
                let more=rect(&rt,"look-open");
                tap(&mut rt,"look-open");rt.frame(0.5);
                assert!((rect(&rt,"look-popover")[0]-more[0]).abs()<1.,"palette popover aligns with its left trigger");
                press(&mut rt,Key::Escape);rt.frame(0.5);
                if dark{assert_dark_surfaces(&rt);}
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_window_palette_is_independent_of_window_and_sidebar_size() {
    let material = crate::material::Settings::default();
    let original = material.native_backdrop(true);
    for (width, height) in [(1280., 943.), (943., 1280.), (1000., 1000.)] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.sidebar_material = material.clone();
        let mut rt = runtime(state, (width, height));
        rt.frame(1.);
        assert_eq!(rt.state.sidebar_material.native_backdrop(true), original);
        assert!(!rt.instances.iter().any(|i| i.params[1] == rune::render::KIND_RADIAL && i.rect == [0.,0.,width,height]));
    }
}


#[test]
fn dark_gradient_keeps_equally_pale_color_and_neutral_stops_at_one_lightness() {
    use crate::material::{LookMode, Settings, Tint};
    fn lightness(c: rune::Color) -> f32 {
        let l = (0.41222147*c.r+0.53633255*c.g+0.051445995*c.b).cbrt();
        let m = (0.2119035*c.r+0.6806995*c.g+0.10739696*c.b).cbrt();
        let s = (0.08830246*c.r+0.28171885*c.g+0.6299787*c.b).cbrt();
        0.21045426*l+0.7936178*m-0.004072047*s
    }
    let green = Tint { hue:115, saturation:50, lightness:93 };
    let pink = Tint { hue:26, saturation:54, lightness:95 };
    for colors in [vec![green,pink],vec![pink,green],vec![green,pink,Tint::default()]] {
        let settings = Settings {colors, mode:LookMode::Dark, ..Settings::default()};
        let saved = settings.clone();
        let stops: Vec<_> = (0..settings.colors.len()).map(|i|lightness(settings.color(i,false))).collect();
        assert!(stops.iter().all(|l|(l-0.35).abs()<0.001),"one dark tone across the palette: {stops:?}");
        for (i,tint) in settings.colors.iter().enumerate() {
            let light = Settings {mode:LookMode::Light, ..settings.clone()};
            assert_eq!(light.color(i,true),tint.color(),"day colors remain untouched");
        }
        assert_eq!(settings,saved,"derived colors must not rewrite saved colors");
    }
    let mut gray = Settings {colors:vec![Tint::default()], mode:LookMode::Dark, ..Settings::default()};
    assert!((lightness(gray.color(0,false))-0.24).abs()<0.001);
    gray.colors[0] = Tint {hue:26,saturation:0,lightness:95};
    let mut previous=lightness(gray.color(0,false));
    for saturation in 1..=100 {
        gray.colors[0].saturation=saturation;
        let next=lightness(gray.color(0,false));
        assert!((next-previous).abs()<0.005,"adjacent picker values must not jump brightness at {saturation}: {previous}->{next}");
        previous=next;
    }
}


#[cfg(target_os = "macos")]
#[test]
fn look_drag_crosses_neutral_without_jumping_the_window_coating() {
    use crate::material::{LookMode, Settings, Tint};
    for transparency in [0, 75] {
        for chosen in [0,1] {
            let f=Fixture::new();
            let mut state=State::from_root(f.0.clone());
            state.sidebar_material=Settings {colors:vec![
                Tint{hue:115,saturation:50,lightness:93},
                Tint{hue:26,saturation:54,lightness:95},
            ],mode:LookMode::Dark,transparency,..Settings::default()};
            let mut rt=runtime(state,(1280.,943.));tap(&mut rt,"settings");rt.frame(1.);
            let original=rt.state.sidebar_material.clone();
            let [x,y,w,h]=rect(&rt,&format!("look-color-{chosen}"));
            let start=[x+w/2.,y+h/2.];
            let field=rect(&rt,"look-field-drag");
            let center=[field[0]+field[2]/2.,field[1]+field[3]/2.];
            let end=[center[0]*2.-start[0],center[1]*2.-start[1]];
            let mut previous=rt.state.sidebar_material.native_backdrop(false);
            let mut largest=0.0f32;
            rt.pointer_down(start[0],start[1]);rt.frame(0.016);
            for step in 1..=400 {
                let t=step as f32/400.;
                rt.pointer_move(start[0]+(end[0]-start[0])*t,start[1]+(end[1]-start[1])*t);
                rt.frame(0.016);
                let next=rt.state.sidebar_material.native_backdrop(false);
                let (rune::SystemMaterial::WindowBackdrop(a),rune::SystemMaterial::WindowBackdrop(b))=(previous,next) else {panic!("native backdrop")};
                let jump=a.colors().iter().flatten().zip(b.colors().iter().flatten()).map(|(a,b)|(a-b).abs()).fold((a.opacity()-b.opacity()).abs(),f32::max);
                largest=largest.max(jump);
                assert!(jump<0.035,"subpixel drag must stay continuous: transparency={transparency}, chosen={chosen}, step={step}, jump={jump}, colors={:?}",rt.state.sidebar_material.colors);
                previous=next;
            }
            rt.pointer_up(end[0],end[1]);rt.frame(0.1);
            assert_ne!(rt.state.sidebar_material.colors,original.colors);
            assert_eq!(State::from_root(f.0.clone()).sidebar_material,rt.state.sidebar_material);
            println!("transparency={transparency}, chosen={chosen}, maximum channel/opacity step={largest}");
        }
    }
}

#[test]
fn appearance_accents_keep_contrast_for_custom_hues_and_both_modes() {
    use crate::ui::{accent, accent_soft, accent_on_dark, c, editor_colors, theme_ink};
    use crate::material::{Tint, LookMode};
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    let contrast = |a: rune::Color, b: rune::Color| {
        let (a, b) = (a.luminance(), b.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    for mode in [LookMode::Light, LookMode::Dark] {
        s.sidebar_material.mode = mode;
        for hue in (0..360).step_by(15) {
            for saturation in [0, 30, 100] {
                s.sidebar_material.colors = vec![Tint { hue, saturation, lightness: 95 }];
                let a = c(accent(&s));
                for surface in [editor_colors(s.theme()).page, editor_colors(s.theme()).raised, editor_colors(s.theme()).inset] {
                    assert!(contrast(a,c(surface)) >= 4.5, "accent {hue}/{saturation} on {surface:x}");
                }
                assert!(contrast(a, c(accent_soft(&s))) >= 4.5, "selected menu text {hue}/{saturation}");
                assert!(contrast(c(theme_ink(s.theme())),c(accent_soft(&s))) >= 4.5);
                assert!(contrast(c(accent_on_dark(&s)), c(0x1b1f2a)) >= 4.5);
                let kit = crate::ui::kit(&s);
                assert_eq!(kit.c.border_focus, a);
                assert_eq!(kit.c.bg_accent, a);
                assert_eq!(kit.c.text_accent, a);
                let stable = accent(&s);
                s.sidebar_material.colors.push(Tint::from_hex(0xee44aa));
                assert_eq!(accent(&s),stable,"secondary gradient stops do not alter the interaction hue");
            }
        }
    }
    s.sidebar_material.colors = crate::material::SAND_COLORS.map(Tint::from_hex).to_vec();
    let warm = c(accent(&s));
    assert!(warm.r > warm.b, "apricot must not inherit a blue interaction accent");
    s.sidebar_material.mode = LookMode::Light;
    s.sidebar_material.colors = crate::material::PORCELAIN_COLORS.map(Tint::from_hex).to_vec();
    assert!(contrast(c(accent(&s)), c(0xffffff)) >= 12.0, "porcelain white uses crisp graphite accent");
    s.sidebar_material.colors = crate::material::WARM_WHITE_COLORS.map(Tint::from_hex).to_vec();
    assert!(s.sidebar_material.is_warm_white());
    assert!(crate::ui::nav_selection_border(&s).is_some());
    assert!(crate::ui::page_surface_border(&s).is_some());
    assert!(contrast(c(accent(&s)), c(0xffffff)) >= 12.0, "warm white uses crisp graphite accent");
}


#[test]
fn page_creation_toolbar_and_tree_disclosure_geometry() {
    for english in [false, true] {
        for mode in [WorkspaceMode::Canvas, WorkspaceMode::Document] {
            let f = Fixture::new();
            let mut s = State::from_root(f.0.clone());
            s.language = if english { crate::i18n::Language::English } else { crate::i18n::Language::Chinese };
            s.choose_mode(mode);
            let root = s.create_doc_note("工作", None);
            let child = s.create_doc_note("工作", Some(root.clone()));
            s.tree_expanded.insert(root.clone());
            s.select_note(&root);
            for i in 0..35 {
                let mut note = s.note().unwrap().clone();
                note.id = format!("navigation-overflow-{i}");
                s.notes.push(note);
            }
            if mode == WorkspaceMode::Canvas { s.select_note("sample-0"); }
            let mut rt = runtime(s, (860., 660.));
            rt.frame(0.5);
            let center = |r: [f32; 4]| r[0] + r[2] / 2.;
            let search = rect(&rt, "open-search");
            let create = rect(&rt, "editor-new-page");
            let marker = rect(&rt, &format!("page-marker-{root}"));
            let nested = rect(&rt, &format!("page-marker-{child}"));
            assert!((center(search) - center(marker)).abs() < 0.1, "search aligns with the page-icon column");
            assert!(rt.find("new-page-inline").is_none());
            let more = rect(&rt, if mode == WorkspaceMode::Canvas { "canvas-more" } else { "doc-more" });
            assert!(create[0] + create[2] < more[0]);
            assert!((create[1] + create[3]/2. - more[1] - more[3]/2.).abs() < 0.1);
            assert!((center(nested) - center(marker) - 14.).abs() < 0.1);
            let parent_row = rect(&rt, &format!("note-{root}"));
            let toggle = rect(&rt, &format!("tree-toggle-{root}"));
            assert!(toggle[0] >= parent_row[0] + 2.);
            assert!(toggle[0] + toggle[2] <= marker[0] - 2., "disclosure cannot overlap the page icon");
            assert!(toggle[1] >= parent_row[1] && toggle[1] + toggle[3] <= parent_row[1] + parent_row[3],
                "the complete disclosure target stays inside its selection background");
            tap(&mut rt, &format!("tree-toggle-{root}"));
            assert!(!rt.state.tree_expanded.contains(&root));
            tap(&mut rt, &format!("tree-toggle-{root}"));
            assert!(rt.state.tree_expanded.contains(&root));
            let viewport = rect(&rt,"notes-scroll");

            rt.wheel(viewport[0]+viewport[2]/2.,viewport[1]+80.,-600.);
            rt.frame(0.2);
            assert!(rt.state.list_offset.get()[1] > 0.);
            assert_eq!(rect(&rt,"editor-new-page"),create,"creation stays available while browsing long notebooks");
            assert_eq!(rect(&rt,"open-search"),search);
            tap(&mut rt,"editor-new-page");
            assert!(rt.state.library_dialogs.new_page.is_some());
            assert!(rt.find("new-page-document").is_some());
            assert!(rt.find("new-page-canvas").is_some());
        }
    }
}

#[test]
fn new_page_notebook_select_routes_creation_and_preserves_cancel() {
    for dark in [false, true] {
        for mode in [WorkspaceMode::Document, WorkspaceMode::Canvas] {
            let f = Fixture::new();
            let mut s = State::from_root(f.0.clone());
            s.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
            let parent = s.create_doc_note("工作", None);
            let mut rt = runtime(s, (800., 600.));
            let count = rt.state.notes.len();
            tap(&mut rt, "editor-new-page");
            assert_eq!(rt.state.library_dialogs.new_page.as_ref().unwrap().book, "工作");
            let field = rect(&rt, "new-page-notebook-trigger");
            let document = rect(&rt, "new-page-document");
            let canvas = rect(&rt, "new-page-canvas");
            assert!((document[0] - field[0]).abs() < 1.);
            assert!((document[2] - canvas[2]).abs() < 1.);
            assert!((document[1] - canvas[1]).abs() < 1.);
            assert!((canvas[0] + canvas[2] - field[0] - field[2]).abs() < 1.);
            tap(&mut rt, "new-page-notebook-trigger");
            assert!(rt.find("new-page-notebook-content").is_some());
            let listbox = rect(&rt, "new-page-notebook-content");
            assert!((listbox[0] - field[0]).abs() < 1. && (listbox[2] - field[2]).abs() < 1.,
                "trigger={field:?}, listbox={listbox:?}");
            press(&mut rt, Key::Escape);
            assert!(!rt.state.library_dialogs.new_page.as_ref().unwrap().book_select_open);
            assert!(rt.find("new-page-dialog").is_some());
            press(&mut rt, Key::Escape);
            assert!(rt.state.library_dialogs.new_page.is_none());
            assert_eq!(rt.state.notes.len(), count);
            rt.update(|s, tx| crate::library_ui::open_new_page(s, "工作", Some(parent.clone()), tx));
            rt.frame(0.3);
            tap(&mut rt, "new-page-notebook-trigger");
            tap(&mut rt, "option1");
            let target = rt.state.library_dialogs.new_page.as_ref().unwrap();
            assert_eq!(target.book, "产品");
            assert!(target.parent.is_none());
            assert_eq!(rt.state.book, "工作", "choosing a destination must not navigate before creation");
            tap(&mut rt, &format!("new-page-{}", mode.key()));
            assert_eq!(rt.state.book, "产品");
            assert_eq!(rt.state.mode, mode);
            assert!(rt.state.note().unwrap().parent_id.is_none());
            assert_eq!(rt.state.notes.len(), count + 1);
            assert_eq!(State::from_root(f.0.clone()).note().unwrap().notebook, "产品");
        }
    }
}

#[test]
fn sidebar_footer_and_settings_return_align_to_content_bottom_edge() {
    for size in [(800., 600.), (1420., 1046.)] {
        let f = Fixture::new();
        let mut s = State::from_root(f.0.clone());
        let doc = s.create_doc_note("工作", None);
        let mut rt = runtime(s, size);
        let bottom = |r: [f32;4]| r[1] + r[3];
        for note in [doc.as_str(), "sample-0"] {
            rt.update(|s,_|s.select_note(note));
            rt.frame(0.5);
            let edge = bottom(rect(&rt, "editor-surface"));
            assert!((bottom(rect(&rt,"settings"))-edge).abs() < 0.1);
            for key in ["settings", "new-notebook"] {
                let button = rect(&rt, key);
                let glyph = rect(&rt, &format!("{key}-glyph"));
                for axis in 0..2 {
                    assert!((button[axis] + button[axis + 2] / 2. - glyph[axis] - glyph[axis + 2] / 2.).abs() < 0.1,
                        "footer glyph must remain centered inside its button on both axes");
                }
            }
            assert_eq!(edge, size.1 - crate::ui::WINDOW_INSET);
        }
        tap(&mut rt,"settings");
        rt.frame(0.5);
        assert!((bottom(rect(&rt,"settings-back"))-bottom(rect(&rt,"settings-body"))).abs() < 0.1);
        tap(&mut rt,"settings-back");
        rt.frame(0.5);
        assert!(rt.state.settings.is_none());
    }
}



#[test]
fn shared_inline_toolbar_structured_colours_clear_and_undo() {
    use crate::doc_core::{DocBlockKind,DocPos,DocSel};
    let f=Fixture::new();let mut state=State::from_root(f.0.clone());
    state.choose_mode(WorkspaceMode::Document);state.create_doc_note("工作",None);
    let block=state.editor.active.core.doc.new_block(DocBlockKind::Text,"中文 format 😀");let bid=block.id;let len=block.text.len();
    state.editor.active.core.doc.blocks=vec![block];state.editor.active.core.sel=DocSel{anchor:DocPos::new(bid,0),head:DocPos::new(bid,len)};
    state.editor.active.focused=true;state.editor.active.typing=false;
    let mut rt=runtime(state,(1280.,943.));rt.update(|s,_|s.editor.active.typing=false);rt.frame(0.5);
    tap(&mut rt,"tb-color");rt.frame(0.5);tap(&mut rt,"text-color-red");rt.frame(0.5);
    assert!(rt.state.editor.active.core.text_of(bid).unwrap().all_have(0..len,|m|m.color.as_deref()==Some("red")));
    tap(&mut rt,"tb-color");rt.frame(0.5);tap(&mut rt,"highlight-color-yellow");rt.frame(0.5);
    assert!(rt.state.editor.active.core.text_of(bid).unwrap().all_have(0..len,|m|m.bg.as_deref()==Some("yellow")));
    tap(&mut rt,"tb-more");rt.frame(0.5);tap(&mut rt,"selection-clear");rt.frame(0.5);
    assert!(rt.state.editor.active.core.text_of(bid).unwrap().all_have(0..len,|m|m.color.is_none()&&m.bg.is_none()));
    command(&mut rt,'z',false);
    assert!(rt.state.editor.active.core.text_of(bid).unwrap().all_have(0..len,|m|m.bg.as_deref()==Some("yellow")));
}





#[test]
fn code_copy_preserves_source_from_button_and_keyboard() {
    use crate::doc_core::{DocBlockKind, DocDocument, DocPos, DocSel};
    let mut doc = DocDocument::empty();
    let code = "type Note = { title: string; done: boolean };\n\nconst today: Note = {\n  title: \"记录一个好想法\",\n  done: false,\n};\n\nconsole.log(today.title);";
    let block = doc.new_block(DocBlockKind::Code { lang: "TypeScript".into(), wrap: true }, code);
    let note = "说明：a & b < c. color:red;";
    let paragraph = doc.new_block(DocBlockKind::Text, note);
    let (id, after) = (block.id, paragraph.id);
    doc.blocks = vec![block, paragraph];
    let (_f, mut rt) = layout_test_runtime(doc, (1280., 943.));
    tap(&mut rt, &format!("code-copy-{id}"));
    assert_eq!(rt.copy_request.take().unwrap().content().text(), code);
    for range in [0..code.len(), code.find("  title:").unwrap()..code.find("};\n\nconsole").unwrap()] {
        rt.update(|s, _| {
            s.editor.active.core.sel = DocSel { anchor: DocPos::new(id, range.end), head: DocPos::new(id, range.start) };
        });
        command(&mut rt, 'c', false);
        let copied = rt.copy_request.take().unwrap();
        assert_eq!(copied.content().text(), &code[range]);
        assert!(copied.content().html().is_none());
    }
    rt.update(|s, _| {
        s.editor.active.core.sel = DocSel { anchor: DocPos::new(id, 0), head: DocPos::new(after, note.len()) };
    });
    command(&mut rt, 'c', false);
    let copied = rt.copy_request.take().unwrap();
    assert_eq!(copied.content().text(), format!("{code}\n{note}"));
    let fragment = format!("<root>{}</root>", copied.content().html().unwrap());
    let doc = roxmltree::Document::parse(&fragment).unwrap();
    assert_eq!(doc.descendants().find(|n| n.has_tag_name("code")).unwrap().text(), Some(code));
    assert_eq!(doc.descendants().find(|n| n.has_tag_name("p")).unwrap().text(), Some(note));
}

#[test]
fn shared_rich_clipboard_preserves_colours_between_document_and_canvas() {
    use crate::doc_core::{DocCore,DocDocument,DocPos,DocSel};
    let mut core=DocCore::new(DocDocument::empty());let id=core.doc.blocks[0].id;
    core.sel=DocSel::caret(DocPos::new(id,0));core.insert_text("中文 <&> 😀");
    let len=core.len_of(id);core.sel=DocSel{anchor:DocPos::new(id,0),head:DocPos::new(id,len)};
    core.update_selected_marks(|m|{m.bold=true;m.color=Some("red".into());m.bg=Some("yellow".into());m.link=Some("https://example.com?a=1&b=2".into());});
    let clipboard=core.clipboard_content();let native=rune::TextEdit::from_html(clipboard.html().unwrap());
    assert_eq!(native.text,"中文 <&> 😀");
    assert!(native.marks().iter().any(|m|m.mark==rune::Mark::Color(0xe22e28)));
    let mut destination=DocCore::new(DocDocument::empty());let dest=destination.doc.blocks[0].id;
    destination.sel=DocSel::caret(DocPos::new(dest,0));destination.paste_native(&native);
    let text=destination.text_of(dest).unwrap();assert_eq!(text.text,native.text);
    assert!(text.all_have(0..text.len(),|m|m.bold&&m.color.as_deref()==Some("#e22e28")&&m.bg.as_deref()==Some("#fff5c2")&&m.link.is_some()));
    assert!(destination.undo());assert!(destination.text_of(dest).unwrap().is_empty());
    assert!(destination.redo());assert_eq!(destination.text_of(dest).unwrap().text,native.text);
    assert_eq!(crate::editor_palette::text_color_hex("#e22e28",Theme::Dark),Some(0xff8d86));
}



#[test]
fn navigation_destinations_fade_as_one_surface_and_remain_responsive() {
    for dark in [false, true] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.sidebar_material.mode = if dark { crate::material::LookMode::Dark } else { crate::material::LookMode::Light };
        let second_book = state.book_at(1).to_owned();
        let other = state.create_doc_note(&second_book, None);
        let document = state.create_doc_note("工作", None);
        state.select_note("sample-0");
        let mut rt = runtime(state, (1280., 943.));
        rt.frame(1.);
        let surface = |rt: &Runtime<State>, key: &str| rt.elements().into_iter()
            .find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap();
        // A focused editor used to turn the whole navigation commit into an instant cut.
        tap(&mut rt, "body-intro");
        for id in [document.as_str(), "sample-4", document.as_str()] {
            let (x, y) = rt.find(&format!("note-{id}")).unwrap();
            rt.pointer_down(x, y);
            rt.frame(0.02);
            rt.pointer_move(x + 1., y + 1.);
            rt.pointer_up(x + 1., y + 1.);
            rt.frame(0.03);
            assert_eq!(rt.state.current, id, "destination is active immediately");
            assert_navigation_selected(&rt, &format!("note-{id}"));
            let key = format!("page-content-{id}");
            let midway = surface(&rt, &key);
            assert!(midway.opacity > 0. && midway.opacity < 0.99,
                "page must fade, got {}", midway.opacity);
            let fixed = midway.rect;
            rt.frame(0.7);
            let final_frame = surface(&rt, &key);
            assert!((final_frame.opacity - 1.).abs() < 0.01);
            assert_eq!(final_frame.rect, fixed, "reading surface does not slide or resize");
        }
        tap(&mut rt, "book-1");
        rt.frame(0.03);
        assert_eq!(rt.state.current, other);
        let list_key = format!("notebook-content-{second_book}");
        assert!(surface(&rt, &list_key).opacity < 0.99, "new notebook list fades as a group");
        // Reverse before the first transition finishes; never block or queue the click.
        tap(&mut rt, "book-0");
        tap(&mut rt, &format!("note-{document}"));
        assert_eq!(rt.state.current, document);
        rt.pointer_move(140., 40.);
        rt.frame(1.);
        rt.frame(1.);
        assert!(!rt.animating, "navigation must return to idle");
        assert_eq!(rt.elements().iter().filter(|e| e.key.as_deref()
            .is_some_and(|k| k.starts_with("page-content-"))).count(), 1,
            "outgoing surfaces must be released");
        assert_eq!(rt.state.mode, WorkspaceMode::Document);
    }
}

#[test]
fn navigation_destination_fade_respects_reduced_motion_and_empty_books() {
    let f = Fixture::new();
    let mut rt = runtime(State::from_root(f.0.clone()), (1000., 736.));
    rt.set_system_reduced_motion(true);
    rt.frame(1.);
    for target in ["book-1", "book-0", "note-sample-4"] {
        tap(&mut rt, target);
        rt.frame(0.5);
        let key = format!("page-content-{}", rt.state.current);
        let e = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&key) && !e.exiting).unwrap();
        assert!((e.opacity - 1.).abs() < 0.01);
        assert!(rt.find("editor-new-page").is_some());
        assert_navigation_selected(&rt, target);
    }
}


// Assert against the shared model after the native editor has been retired.
fn embedded_active_kind(s: &State, owner: &str) -> rune::Block {
    use crate::doc_core::DocBlockKind as K;
    let core = &crate::canvas_document::session(s, owner).expect("embedded editor").core;
    match &core.block(core.sel.head.block).expect("selected body block").kind {
        K::Text => rune::Block::Paragraph,
        K::Heading { level } => rune::Block::Heading(*level),
        K::Todo { done } => rune::Block::Task(0, *done),
        K::Code { .. } => rune::Block::Code,
        k => panic!("unexpected kind {k:?}"),
    }
}
fn embedded_text(s: &State, owner: &str) -> String {
    let core = &crate::canvas_document::session(s, owner).expect("embedded editor").core;
    core.units().iter().filter_map(|u| core.text_of(u.id).map(|t| t.text.clone()))
        .collect::<Vec<_>>().join("\n")
}

#[test]
fn unified_canvas_all_slash_items_match_document_and_keep_outer_geometry() {
    use crate::doc_core::{DocDocument, DocCore, DocPos, DocSel};
    for command in crate::insert_menu::SLASH_CMDS {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.create_canvas_note("工作", None);
        let id = state.create_at(52., 154.);
        let old = state.note().unwrap().blocks[0].clone();
        let mut rt = runtime(state, (1420., 1046.));
        type_text(&mut rt, "/");
        let query = if command.id == "inline_code" { "行内代码" } else { command.label };
        for ch in query.chars() { type_text(&mut rt, &ch.to_string()); }
        rt.frame(0.4);
        let key = format!("slash-item-{}", command.id);
        let (x,y) = rt.find(&key).unwrap_or_else(|| panic!("{} menu missing focused={} slash={:?} caret={:?} vp={:?} text={} keys={:?}",command.id,rt.state.editor.active.focused,rt.state.editor.active.slash,rt.state.editor.active.caret_window_rect(),rt.state.editor.active.viewport_rect,embedded_text(&rt.state,&id),rt.elements().iter().filter_map(|e|e.key.clone()).collect::<Vec<_>>()));
        rt.pointer_down(x,y);rt.frame(0.05);rt.pointer_up(x,y);rt.frame(0.3);
        if command.id == "image" {
            assert!(rt.file_dialog().is_some());
            let source = f.0.join("slash-image.png");
            std::fs::write(&source,include_bytes!("../assets/architecture.png")).unwrap();
            rt.resolve_file_dialog(Some(source)).unwrap();rt.frame(0.2);
        }
        let outer = &rt.state.note().unwrap().blocks;
        assert_eq!(outer.len(),1,"{} must stay in current card",command.id);
        assert_eq!((&outer[0].id,outer[0].x,outer[0].y,outer[0].w),(&old.id,old.x,old.y,old.w));
        // Compare command results directly with the same action on a full document.
        if command.id != "image" {
            let mut doc = DocDocument::empty();
            let bid = doc.blocks[0].id;
            doc.blocks[0].text.text=format!("/{query}");
            let mut reference = State::from_root(Fixture::new().0.clone());
            reference.mode=WorkspaceMode::Document;
            reference.editor.active.core=DocCore::new(doc);
            reference.editor.active.core.sel=DocSel::caret(DocPos::new(bid,query.len()+1));
            reference.editor.active.slash=Some(crate::doc_editor::DocSlash{
                block:bid,trigger_offset:0,trigger:'/',query:query.into(),selected:0});
            crate::doc_editor::execute_slash_cmd(&mut reference,command.id,&mut rune::Tx::default());
            let mut actual=rt.state.editor.active.core.doc.clone();
            actual.next_id=reference.editor.active.core.doc.next_id;
            fn shape(blocks:&[crate::doc_core::DocBlock])->Vec<(String,String,usize)> {
                let mut out=Vec::new();
                fn walk(bs:&[crate::doc_core::DocBlock],depth:usize,out:&mut Vec<(String,String,usize)>){
                    for b in bs {out.push((format!("{:?}",b.kind),b.text.text.clone(),depth));walk(&b.children,depth+1,out);}
                }
                walk(blocks,0,&mut out);out
            }
            assert_eq!(shape(&actual.blocks),shape(&reference.editor.active.core.doc.blocks),"{}",command.id);
            assert_eq!(rt.state.editor.active.core.pending_marks,reference.editor.active.core.pending_marks,"{}",command.id);
        }
        type_text(&mut rt,"继续写");
        rt.state.finish_edit();
        let reopened=State::from_root(f.0.clone());
        let loaded=crate::canvas_document::session(&reopened,&id).expect("saved embedded content");
        assert!(loaded.core.doc.preview_text().contains("继续写"),"{}",command.id);
    }
}

#[test]
fn welcome_library_initializes_once_and_deleted_examples_stay_deleted() {
    let root=std::env::temp_dir().join(format!("echo-welcome-ui-{}",uid()));
    let s=State::from_root(root.clone());
    assert_eq!(s.book_order, crate::welcome::NOTEBOOKS.split(',').collect::<Vec<_>>());
    assert_eq!(s.notes.len(),2);
    assert_eq!(s.current,"welcome-document");
    let reopened=State::from_root(root.clone());
    assert_eq!(reopened.notes.len(),2);
    assert_eq!(reopened.notes[0].doc,s.notes[0].doc);
    assert_eq!(reopened.notes[1],s.notes[1]);
    let mut rt=runtime(s,(1280.,943.));
    rt.frame(0.4);
    assert!(rt.find("doc-text-0").is_some());
    rt.update(|s,_|s.select_note("welcome-canvas"));
    for _ in 0..30 { rt.frame(1./60.); }
    assert!(rt.find("block-welcome-card-0").is_some());
    let bs=&rt.state.note().unwrap().blocks;
    for b in bs { println!("{} y={} h={} w={}",b.id,b.y,b.h,b.w); }
    for (i,a) in bs.iter().enumerate() {
        for b in &bs[i+1..] {
            assert!(a.x+a.w<=b.x || b.x+b.w<=a.x || a.y+a.h<=b.y || b.y+b.h<=a.y,
                "{} overlaps {}",a.id,b.id);
        }
    }
    rt.update(|s,_| {
        s.finish_edit();
        s.notes.clear();s.book_order.clear();s.current.clear();
        s.store.save_with_preferences(&[], "", &[("book_order","")]).unwrap();
    });
    let reopened=State::from_root(root.clone());
    assert!(reopened.notes.is_empty());assert!(reopened.book_order.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unified_canvas_zoom_clicks_ime_selection_and_card_switch_stay_scoped() {
    use crate::doc_core::{DocBlockKind, DocPos, DocSel};
    for zoom in [0.65, 1., 1.4] {
        let f=Fixture::new();
        let mut s=State::from_root(f.0.clone());
        s.create_canvas_note("工作",None);
        s.note_mut().unwrap().zoom=zoom;
        let first=s.create_at(40.,150.);
        let mut rt=runtime(s,(1420.,1046.));
        type_text(&mut rt,"/");type_text(&mut rt,"code");press(&mut rt,Key::Enter);
        type_text(&mut rt,"let value = 123;");
        let code=rt.state.editor.active.core.doc.blocks[0].id;
        let mut second=String::new();
        rt.update(|s,_|second=s.create_at(450.,150.));
        rt.frame(0.3);type_text(&mut rt,"/");type_text(&mut rt,"todo");press(&mut rt,Key::Enter);
        type_text(&mut rt,"中文测试内容");
        let todo=rt.state.editor.active.core.doc.blocks[0].id;
        assert_ne!(code,todo);
        tap(&mut rt,&format!("code-wrap-{code}"));
        assert_eq!(rt.state.editor.canvas_owner(),Some(first.as_str()));
        assert!(matches!(&rt.state.editor.active.core.block(code).unwrap().kind,DocBlockKind::Code{wrap:false,..}));
        let meta=crate::canvas_document::session(&rt.state,&second).unwrap().text_layout(todo).unwrap();
        let local=rune::text::doc_text::caret_rect(&meta.display_text,&meta.spans,meta.style.clone(),meta.max_width,meta.align,6);
        let (x,y)=(meta.rect[0]+local[0]*zoom+0.2,meta.rect[1]+(local[1]+local[3]*0.5)*zoom);
        rt.pointer_down(x,y);rt.pointer_up(x,y);rt.frame(0.1);
        assert_eq!(rt.state.editor.canvas_owner(),Some(second.as_str()));
        assert_eq!(rt.state.editor.active.core.sel,DocSel::caret(DocPos::new(todo,6)),"zoom {zoom}");
        rt.update(|s,tx|{assert!(crate::doc_editor::on_ime(s,&rune::Ime::Preedit("nihao".into(),Some((5,5))),tx));});
        rt.update(|s,tx|{assert!(crate::doc_editor::on_ime(s,&rune::Ime::Commit("你好".into()),tx));});
        assert_eq!(embedded_text(&rt.state,&second),"中文你好测试内容");
        command(&mut rt,'a',false);
        assert!(rt.state.editor.active.core.sel.anchor.block!=0);
        assert!(rt.find("doc-sel-toolbar").is_some());
        rt.paste("替换");rt.frame(0.1);
        assert_eq!(embedded_text(&rt.state,&second),"替换");
        assert_eq!(embedded_text(&rt.state,&first),"let value = 123;");
        rt.state.finish_edit();
        let reopened=State::from_root(f.0.clone());
        assert_eq!(embedded_text(&reopened,&first),"let value = 123;");
        assert_eq!(embedded_text(&reopened,&second),"替换");
    }
}

#[test]
fn reopening_a_canvas_card_expands_legacy_folded_headings() {
    use crate::doc_core::DocBlockKind::{Heading, Text};
    let fixture = Fixture::new();
    let mut state = State::from_root(fixture.0.clone());
    state.create_canvas_note("工作", None);
    let owner = state.create_at(100., 150.);
    let doc = &mut state.editor.active.core.doc;
    let mut first = doc.new_block(Heading { level: 1 }, "旧标题");
    let mut second = doc.new_block(Heading { level: 2 }, "旧副标题");
    first.folded = true;
    second.folded = true;
    doc.blocks = vec![first, second, doc.new_block(Text, "以前折叠的正文")];
    state.editor.active.core.dirty = true;
    crate::doc_editor::sync_to_state(&mut state);
    state.finish_edit();
    let rt = runtime(State::from_root(fixture.0.clone()), (1420., 1046.));
    let core = &crate::canvas_document::session(&rt.state, &owner).unwrap().core;
    assert_eq!(core.units().len(), 3);
    for block in &core.doc.blocks {
        assert!(!block.folded);
        assert!(rt.find(&format!("doc-text-{}", block.id)).is_some());
        assert!(rt.find(&format!("fold-{}", block.id)).is_none());
    }
}

#[test]
fn embedded_heading_controls_fit_the_card_at_each_zoom() {
    for zoom in [0.65, 1., 1.4] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        state.create_canvas_note("工作", None);
        state.note_mut().unwrap().zoom = zoom;
        let card_id = state.create_at(100., 150.);
        let mut rt = runtime(state, (1420., 1046.));
        type_text(&mut rt, "/");
        type_text(&mut rt, "二级标题");
        press(&mut rt, Key::Enter);
        type_text(&mut rt, "标题");
        press(&mut rt, Key::Enter);
        type_text(&mut rt, "正文");
        let block_id = rt.state.editor.active.core.doc.blocks[0].id;
        let paragraph_id = rt.state.editor.active.core.doc.blocks[1].id;
        let text_key = format!("doc-text-{block_id}");
        let (x, y) = rt.find(&text_key).unwrap();
        rt.pointer_move(x, y);
        rt.frame(0.2);
        let card = rect(&rt, &format!("block-{card_id}"));
        let grip_key = format!("doc-handle-{block_id}");
        let grip = rect(&rt, &grip_key);
        let text = rect(&rt, &text_key);
        assert!(grip[0] >= card[0] && grip[0] + grip[2] <= text[0]);
        assert!(rt.find(&format!("fold-{block_id}")).is_none());
        assert!(rt.find(&format!("doc-blk-{paragraph_id}")).is_some());
        tap(&mut rt, &grip_key);
        assert!(rt.find("doc-block-menu").is_some());
        press(&mut rt, Key::Escape);
        let [_, top, _, height] = rect(&rt, &format!("doc-blk-{paragraph_id}"));
        rt.pointer_move(x, y);
        rt.frame(0.1);
        let (sx, sy) = rt.find(&grip_key).unwrap();
        let drop_y = top + height * 0.8;
        rt.pointer_down(sx, sy);
        rt.pointer_move(sx, drop_y);
        rt.frame(0.05);
        rt.pointer_up(sx, drop_y);
        rt.frame(0.05);
        assert_eq!(rt.state.editor.active.core.doc.blocks[0].id, block_id);
        assert_eq!(rt.state.editor.active.core.doc.blocks[1].id, paragraph_id);
    }
}

#[test]
fn heading_drag_keeps_sections_in_both_editors() {
    use crate::doc_core::DocBlockKind::{Heading, Text};
    for (canvas, folded) in [(false, false), (false, true), (true, false)] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        if canvas {
            state.create_canvas_note("工作", None);
            state.create_at(100., 150.);
        } else {
            state.choose_mode(WorkspaceMode::Document);
        }
        let doc = &mut state.editor.active.core.doc;
        let mut a = doc.new_block(Heading { level: 1 }, "甲");
        a.folded = folded;
        let mut b = doc.new_block(Heading { level: 1 }, "乙");
        b.folded = folded;
        let block_a = a.id;
        let block_b = b.id;
        doc.blocks = vec![a, doc.new_block(Text, "甲正文"), doc.new_block(Heading { level: 2 }, "甲子节"),
            doc.new_block(Text, "子节正文"), b, doc.new_block(Text, "乙正文")];
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        let order = |rt: &Runtime<State>| rt.state.editor.active.core.doc.blocks.iter()
            .map(|b| b.text.text.clone()).collect::<Vec<_>>();
        let before = order(&rt);
        let end_b = rt.state.editor.active.core.doc.blocks[5].id;
        let grip_key = format!("doc-handle-{block_a}");
        let (x, y) = rt.find(&format!("doc-text-{block_a}")).unwrap();
        rt.pointer_move(x, y);
        rt.frame(0.1);
        let control = if canvas || folded { grip_key } else { format!("fold-{block_a}") };
        let (sx, sy) = rt.find(&control).unwrap();
        let target = rect(&rt, &format!("doc-blk-{block_b}"));
        let (tx, ty) = (target[0] + 8., target[1] + target[3] * 0.85);
        let boundary = rect(&rt, &format!("doc-blk-{}", if folded { block_b } else { end_b }));
        rt.pointer_down(sx, sy);
        rt.pointer_move(tx, ty);
        rt.frame(0.05);
        assert!(rt.find("doc-drop-indicator").is_some(), "canvas={canvas} folded={folded}");
        let line = rect(&rt, "doc-drop-indicator");
        assert!((line[1] + line[3] - boundary[1] - boundary[3]).abs() < 1.5);
        assert_eq!(rect(&rt, &format!("doc-blk-{block_b}")), target, "indicator moved the text");
        press(&mut rt, Key::Escape);
        rt.pointer_up(tx, ty);
        rt.frame(0.05);
        assert_eq!(order(&rt), before);
        assert!(rt.state.editor.active.popover.is_none());
        rt.pointer_move(x, y);
        rt.frame(0.05);
        rt.pointer_down(sx, sy);
        rt.pointer_move(tx, ty);
        rt.frame(0.05);
        rt.pointer_up(0., ty);
        rt.frame(0.05);
        assert_eq!(order(&rt), before);
        rt.pointer_move(x, y);
        rt.frame(0.05);
        rt.pointer_down(sx, sy);
        rt.pointer_move(tx, ty);
        rt.frame(0.05);
        rt.pointer_up(tx, ty);
        rt.frame(0.05);
        assert_eq!(order(&rt), ["乙", "乙正文", "甲", "甲正文", "甲子节", "子节正文"]);
        assert_eq!(rt.state.editor.active.core.block(block_a).unwrap().folded, folded);
        command(&mut rt, 'z', false);
        assert_eq!(order(&rt), before);
    }
}

#[test]
fn editor_audit_folded_heading_enter_keeps_body_and_visible_caret() {
    use crate::doc_core::{DocBlockKind as K, DocDocument, DocPos, DocSel};
    for offset in [0, "甲".len(), "甲乙".len()] {
        let mut doc = DocDocument::empty();
        let mut title = doc.new_block(K::Heading { level: 2 }, "甲乙");
        title.folded = true;
        let id = title.id;
        let body = doc.new_block(K::Todo { done: true }, "原有待办");
        let body_id = body.id;
        let next = doc.new_block(K::Heading { level: 2 }, "下一节");
        doc.blocks = vec![title, body, next];
        let (_fixture, mut rt) = layout_test_runtime(doc.clone(), (1280., 943.));
        rt.update(|s, _| s.editor.active.core.set_head(DocPos::new(id, offset), false));
        press(&mut rt, Key::Enter);
        let core = &rt.state.editor.active.core;
        let head = core.sel.head;
        assert_eq!(core.sel, DocSel::caret(DocPos::new(head.block, 0)));
        assert!(rt.find("doc-caret").is_some(), "offset {offset}: caret must be mounted");
        let row = rect(&rt, &format!("doc-text-{}", head.block));
        let caret = rect(&rt, "doc-caret");
        assert!((row[1] - caret[1]).abs() < 2., "offset {offset}: {row:?} {caret:?}");
        if offset == "甲乙".len() {
            assert_eq!(core.doc.blocks[1].id, body_id, "folded section keeps its original body");
            assert_eq!(core.doc.blocks[2].id, head.block);
            assert_eq!(core.block(head.block).unwrap().kind, K::Heading { level: 2 });
        }
        let expected = format!("新{}", &"甲乙"[offset..]);
        type_text(&mut rt, "新");
        assert_eq!(rt.state.editor.active.core.block(head.block).unwrap().text.text, expected);
        command(&mut rt, 'z', false);
        command(&mut rt, 'z', false);
        assert_eq!(rt.state.editor.active.core.doc, doc);
        assert_eq!(rt.state.editor.active.core.sel.head, DocPos::new(id, offset));
    }
}

#[test]
fn editor_audit_drag_then_enter_edits_moved_block_in_both_modes() {
    use crate::doc_core::{DocBlockKind as K, DocPos};
    for canvas in [false, true] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        if canvas {
            state.create_canvas_note("工作", None);
            state.create_at(100., 150.);
        } else {
            state.choose_mode(WorkspaceMode::Document);
        }
        let doc = &mut state.editor.active.core.doc;
        let table = doc.new_block(K::Table(crate::table_data::TableData::default()), "");
        let source = doc.new_block(K::Todo { done: true }, "拖走的待办");
        let target = doc.new_block(K::Bullet, "目标");
        let (table_id, source_id, target_id) = (table.id, source.id, target.id);
        doc.blocks = vec![table, source.clone(), target.clone()];
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        rt.update(|s, tx| crate::doc_tables::begin(s, table_id, 0, 0, tx));
        let (x, y) = rt.find(&format!("doc-text-{source_id}")).unwrap();
        rt.pointer_move(x, y);
        rt.frame(0.05);
        let (sx, sy) = rt.find(&format!("doc-handle-{source_id}")).unwrap();
        let [x, y, _, h] = rect(&rt, &format!("doc-blk-{target_id}"));
        rt.pointer_down(sx, sy);
        rt.pointer_move(x + 16., y + h * 0.8);
        rt.frame(0.05);
        rt.pointer_up(x + 16., y + h * 0.8);
        rt.frame(0.05);
        let core = &rt.state.editor.active.core;
        assert_eq!(core.doc.blocks[1..], [target, source.clone()]);
        assert_eq!(core.sel.head, DocPos::new(source_id, source.text.len()), "canvas={canvas}");
        press(&mut rt, Key::Enter);
        type_text(&mut rt, "继续");
        let next = &rt.state.editor.active.core.doc.blocks[3];
        assert_eq!(next.kind, K::Todo { done: false });
        assert_eq!(next.text.text, "继续");
    }
}

#[test]
fn editor_audit_every_kind_moves_with_its_payload_and_round_trips() {
    use crate::doc_core::{DocBlockKind as K, DocCore, DocDocument, DocPos};
    for kind in [
        K::Text, K::Heading { level: 1 }, K::Heading { level: 3 }, K::Bullet,
        K::Ordered { start: Some(7) }, K::Todo { done: true }, K::Quote,
        K::Callout { emoji: "💡".into(), palette: "blue".into() },
        K::Code { lang: "Rust".into(), wrap: false }, K::Divider,
        K::Image { src: "img.png".into(), caption: "图".into(), width: Some(42.) },
        K::Table(crate::table_data::TableData::default()),
    ] {
        for after in [false, true] {
            let mut doc = DocDocument::empty();
            let mut moved = doc.new_block(kind.clone(), if kind.has_text() { "待移动📝" } else { "" });
            if kind.can_nest() || kind.is_container() {
                moved.children.push(doc.new_block(K::Todo { done: false }, "子待办"));
            }
            let dest = doc.new_block(K::Heading { level: 1 }, "目标节");
            let moved_id = moved.id;
            let dest_id = dest.id;
            let boundary = doc.new_block(K::Heading { level: 1 }, "边界");
            doc.blocks = if after { vec![moved.clone(), dest, boundary] }
                else { vec![dest, boundary, moved.clone()] };
            let mut core = DocCore::new(doc.clone());
            core.set_head(DocPos::new(dest_id, 0), false);
            assert!(core.move_block(moved_id, dest_id, after), "{kind:?}, after={after}");
            assert_eq!(core.doc.blocks[usize::from(after)], moved);
            let changed = core.doc.clone();
            let serialized = crate::file_format::write_document_body(&changed).unwrap();
            let mut reopened = DocDocument::empty();
            reopened.blocks.clear();
            crate::file_format::read_document_body(&serialized, &mut reopened).unwrap();
            assert_eq!(reopened.blocks, changed.blocks);
            assert!(core.undo());
            assert_eq!(core.doc, doc);
            assert!(core.redo());
            assert_eq!(core.doc, changed);
        }
    }
}

#[test]
fn editor_audit_enter_at_unicode_boundaries_and_rendered_line_breaks() {
    use crate::doc_core::{DocBlockKind as K, DocDocument, DocPos};
    for kind in [K::Text, K::Heading { level: 1 }, K::Bullet, K::Ordered { start: Some(8) },
        K::Todo { done: true }, K::Code { lang: "Rust".into(), wrap: true }, K::Quote,
        K::Callout { emoji: "💡".into(), palette: "blue".into() }] {
        for offset in [0, "甲".len(), "甲📝".len()] {
            let mut doc = DocDocument::empty();
            let block = doc.new_block(if kind.is_container() { K::Text } else { kind.clone() }, "甲📝");
            let id = block.id;
            doc.blocks = vec![if kind.is_container() { doc.new_container(kind.clone(), block) } else { block }];
            let (_fixture, mut rt) = layout_test_runtime(doc.clone(), (1000., 736.));
            rt.update(|s, _| s.editor.active.core.set_head(DocPos::new(id, offset), false));
            press(&mut rt, Key::Enter);
            let core = &rt.state.editor.active.core;
            let head = core.sel.head;
            let row = rect(&rt, &format!("doc-text-{}", head.block));
            let caret = rect(&rt, "doc-caret");
            if matches!(kind, K::Code { .. }) {
                assert_eq!(head, DocPos::new(id, offset + 1));
                assert!(caret[1] >= row[1] + caret[3] - 2., "{kind:?}/{offset}: {caret:?}, {row:?}");
            } else {
                assert_eq!(head.offset, 0);
                assert_eq!(core.block(head.block).unwrap().text.text, &"甲📝"[offset..]);
                let expected_kind = if kind.is_container() { K::Text }
                    else if offset == 0 { kind.clone() } else { kind.resets_on_enter_split() };
                assert_eq!(core.block(head.block).unwrap().kind, expected_kind);
                assert!((caret[1] - row[1]).abs() < 2., "{kind:?}/{offset}");
                if kind.is_container() { assert_eq!(core.parent_of(head.block).unwrap().kind, kind); }
            }
            assert!((caret[0] - row[0]).abs() < 2., "{kind:?}/{offset}");
            type_text(&mut rt, "续");
            let text = &rt.state.editor.active.core.block(head.block).unwrap().text.text;
            assert!(text[head.offset..].starts_with("续"));
            command(&mut rt, 'z', false);
            command(&mut rt, 'z', false);
            assert_eq!(rt.state.editor.active.core.doc, doc, "{kind:?}/{offset}");
        }
    }
}

#[test]
fn editor_audit_repeated_breaks_keep_caret_inside_viewport() {
    use crate::doc_core::{DocBlockKind as K, DocPos};
    for canvas in [false, true] {
        for code in [false, true] {
            let fixture = Fixture::new();
            let mut state = State::from_root(fixture.0.clone());
            if canvas {
                state.create_canvas_note("工作", None);
                state.create_at(100., 150.);
            } else {
                state.choose_mode(WorkspaceMode::Document);
            }
            let doc = &mut state.editor.active.core.doc;
            let kind = if code { K::Code { lang: "Rust".into(), wrap: true } } else { K::Text };
            let b = doc.new_block(kind, "字");
            let id = b.id;
            doc.blocks = vec![b];
            state.editor.active.focused = true;
            state.editor.active.core.set_head(DocPos::new(id, "字".len()), false);
            let mut rt = runtime(state, (1100., 530.));
            for _ in 0..25 {
                rt.key(KeyEvent { key: Key::Enter, mods: Mods { shift: !code, ..Default::default() } });
                rt.frame(0.02);
            }
            for _ in 0..20 { rt.frame(1./60.); }
            let caret = rect(&rt, "doc-caret");
            let vp = rt.state.editor.active.viewport_rect.unwrap();
            assert!(caret[1] >= vp[1] - 1. && caret[1] + caret[3] <= vp[1] + vp[3] + 1.,
                "canvas={canvas}, code={code}, caret={caret:?}, viewport={vp:?}");
        }
    }
}

#[test]
fn editor_audit_heading_drop_respects_foreign_sections_and_indicator() {
    use crate::doc_core::{DocBlockKind as K, DocDocument, DocPos};
    let mut doc = DocDocument::empty();
    let source = doc.new_block(K::Heading { level: 1 }, "源");
    let destination = doc.new_block(K::Heading { level: 2 }, "目标");
    let body = doc.new_block(K::Bullet, "目标正文");
    let (src, dst, text) = (source.id, destination.id, body.id);
    let caret = DocPos::new(text, "目标".len());
    doc.blocks = vec![destination, body, doc.new_block(K::Heading { level: 1 }, "分隔"),
        source, doc.new_block(K::Todo { done: true }, "源待办")];
    let (_fixture, mut rt) = layout_test_runtime(doc.clone(), (1280., 943.));
    rt.update(|s, _| s.editor.active.core.set_head(caret, false));
    let (x, y) = rt.find(&format!("doc-text-{src}")).unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.05);
    let (sx, sy) = rt.find(&format!("doc-handle-{src}")).unwrap();
    let [bx, by, _, bh] = rect(&rt, &format!("doc-blk-{text}"));
    rt.pointer_down(sx, sy);
    rt.pointer_move(bx + 8., by + bh * 0.2);
    rt.frame(0.05);
    assert!(rt.find("doc-drop-indicator").is_none(), "cannot claim the target heading's body");
    rt.pointer_up(bx + 8., by + bh * 0.2);
    assert_eq!(rt.state.editor.active.core.doc, doc);
    assert_eq!(rt.state.editor.active.core.sel.head, caret);
    let [dx, dy, _, dh] = rect(&rt, &format!("doc-blk-{dst}"));
    rt.pointer_move(x, y);
    rt.frame(0.05);
    rt.pointer_down(sx, sy);
    rt.pointer_move(dx + 8., dy + dh * 0.8);
    rt.frame(0.05);
    let indicator = rect(&rt, "doc-drop-indicator");
    assert!((indicator[1] + indicator[3] - by - bh).abs() < 1.5);
    rt.pointer_up(dx + 8., dy + dh * 0.8);
    rt.frame(0.05);
    assert_eq!(rt.state.editor.active.core.doc.blocks.iter().map(|b| b.id).collect::<Vec<_>>(),
        [dst, text, src, doc.blocks[4].id, doc.blocks[2].id]);
    command(&mut rt, 'z', false);
    assert_eq!(rt.state.editor.active.core.doc, doc);
    assert_eq!(rt.state.editor.active.core.sel.head, caret);
}

#[test]
fn editor_audit_nested_caret_remains_visible_after_move_fold_and_enter() {
    use crate::doc_core::{DocBlockKind as K, DocCore, DocDocument, DocPos};
    let mut doc = DocDocument::empty();
    let item = doc.new_block(K::Bullet, "要移动");
    let mut target = doc.new_block(K::Heading { level: 1 }, "折叠目标");
    target.folded = true;
    let last = doc.new_block(K::Text, "");
    let (src, dst, tail) = (item.id, target.id, last.id);
    doc.blocks = vec![item, target, last];
    let mut core = DocCore::new(doc.clone());
    let midway = DocPos::new(src, "要".len());
    core.set_head(midway, false);
    assert!(core.move_block(src, dst, true));
    assert_eq!(core.sel.head, midway);
    assert!(core.units().iter().any(|u| u.id == src));
    assert!(core.undo());
    assert_eq!(core.doc, doc);
    assert_eq!(core.ensure_trailing_empty_paragraph(), DocPos::new(tail, 0));
    assert!(core.units().iter().any(|u| u.id == tail));
    core.set_head(DocPos::new(tail, 0), false);
    core.toggle_fold(dst);
    assert_eq!(core.sel.head, DocPos::new(dst, core.len_of(dst)));
    assert!(core.exit_block_below());
    assert!(core.units().iter().any(|u| u.id == core.sel.head.block));

    let mut parent = core.doc.new_block(K::Bullet, "上级");
    let child = core.doc.new_block(K::Todo { done: false }, "");
    let id = child.id;
    parent.children.push(child);
    core.doc.blocks = vec![parent];
    core.set_head(DocPos::new(id, 0), false);
    let original = core.doc.clone();
    core.enter();
    assert_eq!(core.sel.head, DocPos::new(id, 0));
    assert!(core.parent_of(id).is_none());
    assert!(core.undo());
    assert_eq!(core.doc, original);
    assert!(core.redo());
    assert!(core.parent_of(id).is_none());
}

#[test]
fn editor_audit_folded_rows_release_their_drag_handles() {
    use crate::doc_core::DocBlockKind as K;
    for canvas in [false, true] {
        let fixture = Fixture::new();
        let mut state = State::from_root(fixture.0.clone());
        if canvas {
            state.create_canvas_note("工作", None);
            state.create_at(100., 150.);
        } else {
            state.choose_mode(WorkspaceMode::Document);
        }
        let doc = &mut state.editor.active.core.doc;
        let title = doc.new_block(K::Heading { level: 2 }, "折叠");
        let next = doc.new_block(K::Heading { level: 2 }, "另一节");
        let (fold, next_id) = (title.id, next.id);
        doc.blocks = vec![title, doc.new_block(K::Todo { done: true }, "隐藏"), doc.new_block(K::Text, "隐藏"), next];
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        let arrow = format!("fold-{fold}");
        if canvas {
            let (x, y) = rt.find(&format!("doc-text-{fold}")).unwrap();
            rt.pointer_move(x, y);
            rt.frame(0.1);
            assert!(rt.find(&arrow).is_none());
            assert!(rt.find(&format!("doc-handle-{fold}")).is_some());
            continue;
        }
        let opacity = |rt: &Runtime<State>| rt.elements().into_iter()
            .find(|element| element.key.as_deref() == Some(&arrow)).unwrap().opacity;
        assert!(opacity(&rt) < 0.01, "canvas={canvas}");
        let (x, y) = rt.find(&format!("doc-text-{fold}")).unwrap();
        rt.pointer_move(x, y);
        for _ in 0..12 { rt.frame(1. / 60.); }
        assert!(opacity(&rt) > 0.99, "悬停标题时应显示箭头，canvas={canvas}");
        let [left, top, width, height] = rect(&rt, &arrow);
        let mid_y = top + height / 2.;
        let text_left = rect(&rt, &format!("doc-text-{fold}"))[0];
        for x in [(left + width + text_left) / 2., left + width / 2.] {
            rt.pointer_move(x, mid_y);
            for _ in 0..12 { rt.frame(1. / 60.); }
            assert!(opacity(&rt) > 0.99, "canvas={canvas}, x={x}");
        }
        tap(&mut rt, &arrow);
        assert!(rt.state.editor.active.core.block(fold).unwrap().folded);
        let (x, y) = rt.find(&format!("doc-text-{next_id}")).unwrap();
        rt.pointer_move(x, y);
        for _ in 0..12 { rt.frame(1. / 60.); }
        assert!(opacity(&rt) < 0.01, "canvas={canvas}");
        assert!(rt.find(&format!("doc-handle-{next_id}")).is_some(), "canvas={canvas}");
    }
}

#[test]
fn canvas_block_menu_converts_with_separate_pointer_frames() {
    use crate::doc_core::DocBlockKind;
    let choices = [
        ("正文", DocBlockKind::Text),
        ("一级标题", DocBlockKind::Heading { level: 1 }),
        ("二级标题", DocBlockKind::Heading { level: 2 }),
        ("三级标题", DocBlockKind::Heading { level: 3 }),
        ("无序列表", DocBlockKind::Bullet),
        ("有序列表", DocBlockKind::Ordered { start: None }),
        ("待办", DocBlockKind::Todo { done: false }),
        ("引用", DocBlockKind::Quote),
        ("高亮块", DocBlockKind::Callout { emoji: "💡".into(), palette: "orange".into() }),
        ("代码块", DocBlockKind::Code { lang: "Plain Text".into(), wrap: true }),
    ];
    for (source_name, source) in &choices {
        for (title, kind) in &choices {
            let fixture = Fixture::new();
            let mut state = State::from_root(fixture.0.clone());
            state.create_canvas_note("工作", None);
            let owner = state.create_at(100., 150.);
            let mut rt = runtime(state, (1420., 1046.));
            type_text(&mut rt, "需要转换的正文");
            rt.update(|s, tx| {
                let id = s.editor.active.core.doc.blocks[0].id;
                s.editor.active.core.turn_into(&[id], source.clone());
                crate::doc_editor::sync_to_state(s);
                tx.instant();
            });
            rt.frame(0.2);
            let before = rt.state.editor.active.core.doc.clone();
            let id = before.blocks[0].id;
            rt.update(|s, tx| {
                s.create_at(700., 150.);
                s.editor.active.core.insert_text("另一张卡片");
                crate::doc_editor::sync_to_state(s);
                tx.instant();
            });
            rt.frame(0.2);
            let (x,y) = rt.find(&format!("doc-blk-{id}")).unwrap();
            rt.pointer_move(x,y); rt.frame(0.2);
            tap(&mut rt, &format!("doc-handle-{id}"));
            let key = format!("block-type-{title}");
            let (x,y) = rt.find(&key).unwrap();
            rt.pointer_move(x,y); rt.frame(0.1);
            rt.pointer_down(x,y); rt.frame(0.1);
            rt.pointer_up(x,y); rt.frame(0.1);
            assert_eq!(&rt.state.editor.active.core.doc.blocks[0].kind, kind, "{source_name} -> {title}");
            assert_eq!(rt.state.editor.canvas_owner(), Some(owner.as_str()));
            assert!(rt.state.editor.active.popover.is_none());
            let converted = rt.state.editor.active.core.doc.clone();
            if kind.is_container() {
                assert_eq!(converted.blocks[0].children.len(), 1);
                let expected = if source.is_container() { &DocBlockKind::Text } else { source };
                assert_eq!(&converted.blocks[0].children[0].kind, expected);
            }
            assert!(rt.state.editor.active.core.undo());
            assert_eq!(rt.state.editor.active.core.doc, before);
            assert!(rt.state.editor.active.core.redo());
            assert_eq!(rt.state.editor.active.core.doc, converted);
            crate::doc_editor::sync_to_state(&mut rt.state);
            rt.state.finish_edit();
            let reopened = State::from_root(fixture.0.clone());
            let session = crate::canvas_document::session(&reopened, &owner).unwrap();
            assert_eq!(&session.core.doc.blocks[0].kind, kind);
            assert!(session.core.doc.to_plain_text().contains("需要转换的正文"));
            assert_eq!(reopened.note().unwrap().blocks.len(), 2);
            let other = reopened.note().unwrap().blocks.iter().find(|b| b.id != owner).unwrap();
            assert!(other.body.contains("另一张卡片"));
        }
    }
}

#[test]
fn container_conversion_preserves_rich_text_tables_and_nested_children() {
    use crate::doc_core::{DocBlockKind as K, DocCore, DocDocument, DocPos, DocSel};
    let mut doc = DocDocument::empty();
    let mut first = doc.new_block(K::Text, "保留格式");
    first.text.runs[0].marks.bold = true;
    let nested = doc.new_block(K::Bullet, "嵌套列表");
    first.children.push(nested);
    let second = doc.new_block(K::Text, "第二段");
    let table = doc.new_block(K::Table(crate::table_data::TableData::default()), "");
    let original_children = vec![first.clone(), table, second];
    let mut quote = doc.new_container(K::Quote, first.clone());
    quote.children = original_children.clone();
    let outer = quote.id;
    doc.blocks = vec![quote];
    let mut core = DocCore::new(doc);
    core.include_title = false;
    core.sel = DocSel::caret(DocPos::new(first.id, "保留".len()));
    let before = core.doc.clone();
    let selection = core.sel;
    core.turn_into(&[outer], K::Callout { emoji: "💡".into(), palette: "orange".into() });
    assert_eq!(core.doc.blocks[0].children, original_children);
    core.turn_into(&[outer], K::Text);
    assert_eq!(core.doc.blocks, original_children);
    assert_eq!(core.sel, selection);
    assert!(core.undo());
    assert!(core.undo());
    assert_eq!(core.doc, before);
}

#[test]
fn canvas_structural_blocks_offer_conversions_that_preserve_their_content() {
    use crate::doc_core::DocBlockKind as K;
    for source in [
        K::Table(crate::table_data::TableData::default()),
        K::Image { src: "images/example.png".into(), width: None, caption: "图片说明".into() },
        K::Divider,
    ] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.create_canvas_note("工作", None);
        state.create_at(100., 150.);
        state.editor.active.core.doc.blocks[0].kind = source.clone();
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        let mut rt = runtime(state, (1420., 1046.));
        let id = rt.state.editor.active.core.doc.blocks[0].id;
        let (x,y) = rt.find(&format!("doc-blk-{id}")).unwrap();
        rt.pointer_move(x,y); rt.frame(0.2);
        tap(&mut rt, &format!("doc-handle-{id}"));
        assert!(rt.find("block-type-正文").is_none());
        assert!(rt.find("block-type-代码块").is_none());
        assert!(rt.find("block-type-引用").is_some());
        assert!(rt.find("block-type-高亮块").is_some());
        tap(&mut rt, "block-type-引用");
        assert_eq!(rt.state.editor.active.core.doc.blocks[0].kind, K::Quote);
        assert_eq!(rt.state.editor.active.core.doc.blocks[0].children[0].kind, source);
    }
}

#[test]
fn canvas_inner_grips_reorder_every_kind_without_moving_the_card() {
    use crate::doc_core::DocBlockKind as K;
    for kind in [
        K::Text, K::Heading { level: 1 }, K::Heading { level: 3 },
        K::Bullet, K::Ordered { start: Some(3) }, K::Todo { done: false },
        K::Quote, K::Callout { emoji: "💡".into(), palette: "orange".into() },
        K::Code { lang: "Plain Text".into(), wrap: true },
        K::Table(crate::table_data::TableData::default()), K::Divider,
        K::Image { src: "images/example.png".into(), width: Some(100.), caption: "图片".into() },
    ] {
        for zoom in [0.65, 1., 1.4] {
            let f = Fixture::new();
            let mut state = State::from_root(f.0.clone());
            state.create_canvas_note("工作", None);
            let owner = state.create_at(100., 150.);
            state.note_mut().unwrap().zoom = zoom;
            let doc = &mut state.editor.active.core.doc;
            let mut source = doc.new_block(kind.clone(), if kind.has_text() { "拖动内容" } else { "" });
            if kind.is_container() { source.children.push(doc.new_block(K::Text, "容器内文")); }
            let target = doc.new_block(K::Heading { level: 1 }, "放到这里以后");
            let (id, target_id) = (source.id, target.id);
            doc.blocks = vec![source.clone(), target.clone()];
            state.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(&mut state);
            let mut rt = runtime(state, (1420., 1046.));
            let card = rt.state.note().unwrap().blocks.iter().find(|b| b.id == owner).unwrap();
            let initial_position = [card.x, card.y];
            let (x,y) = rt.find(&format!("doc-blk-{id}")).unwrap();
            rt.pointer_move(x,y); rt.frame(0.2);
            let (sx,sy) = rt.find(&format!("doc-handle-{id}")).unwrap();
            rt.pointer_move(sx,sy); rt.frame(0.1);
            rt.pointer_down(sx,sy); rt.frame(0.05);
            let [_,y,_,h] = rect(&rt, &format!("doc-blk-{target_id}"));
            rt.pointer_move(sx,y+h*0.8); rt.frame(0.05);
            assert!(rt.find("doc-drop-indicator").is_some(), "{kind:?}, {zoom}");
            rt.pointer_up(sx,y+h*0.8); rt.frame(0.05);
            assert_eq!(rt.state.editor.active.core.doc.blocks, vec![target,source], "{kind:?}, {zoom}");
            let card = rt.state.note().unwrap().blocks.iter().find(|b| b.id==owner).unwrap();
            assert_eq!([card.x,card.y], initial_position);
        }
    }
}


#[test]
fn canvas_card_drag_regions_move_each_card_with_separate_pointer_frames() {
    use crate::doc_core::DocBlockKind as K;
    for kind in [K::Text, K::Heading { level: 1 }, K::Bullet, K::Todo { done: false },
        K::Quote, K::Callout { emoji: "💡".into(), palette: "orange".into() },
        K::Code { lang: "Plain Text".into(), wrap: true },
        K::Table(crate::table_data::TableData::default()), K::Divider,
        K::Image { src: "images/example.png".into(), width: Some(100.), caption: "图片".into() }] {
        for zoom in [0.65, 1., 1.4] {
            let f = Fixture::new();
            let mut state = State::from_root(f.0.clone());
            state.create_canvas_note("工作", None);
            let owner = state.create_at(142., 205.);
            let doc = &mut state.editor.active.core.doc;
            let mut source = doc.new_block(kind.clone(), if kind.has_text() { "需要移动的卡片" } else { "" });
            if kind.is_container() { source.children.push(doc.new_block(K::Text, "容器内文")); }
            doc.blocks = vec![source];
            state.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(&mut state);
            state.finish_edit();
            let other = state.create_at(142., 505.);
            state.editor.active.core.doc.blocks[0].text = crate::doc_core::RichText::plain("另一张卡片");
            state.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(&mut state);
            state.finish_edit();
            state.note_mut().unwrap().zoom = zoom;
            state.selected = Some(other.clone());
            let mut rt = runtime(state, (1420., 1046.));
            for (id, region) in [(&owner, "move-edge-left"), (&other, "move-edge-right"),
                (&owner, "move-edge-top"), (&owner, "move-edge-bottom"), (&owner, "move-handle")] {
                let before = rt.state.note().unwrap().blocks.iter().find(|b| &b.id==id).unwrap().clone();
                let [x,y,w,_] = rect(&rt, &format!("block-{id}"));
                rt.pointer_move(x+w*0.5,y+8.*zoom); rt.frame(0.2);
                let key = format!("{region}-{id}");
                let (sx,sy) = rt.find(&key).unwrap_or_else(|| panic!("missing handle: {kind:?} {zoom} {id}"));
                rt.pointer_move(sx,sy); rt.frame(0.1);
                assert!(rt.find(&key).is_some(), "handle lost: {kind:?} {zoom}");
                assert_eq!(rt.cursor(), rune::Cursor::Move, "{kind:?} {zoom} {region}");
                rt.pointer_down(sx,sy); rt.frame(0.05);
                rt.pointer_move(sx+50.*zoom,sy+30.*zoom); rt.frame(0.05);
                rt.pointer_up(sx+50.*zoom,sy+30.*zoom); rt.frame(0.1);
                let after = rt.state.note().unwrap().blocks.iter().find(|b| &b.id==id).unwrap();
                assert!((after.x-before.x-50.).abs()<1. && (after.y-before.y-30.).abs()<1.,
                    "{kind:?} zoom={zoom} {id} {region}: before={},{} after={},{}", before.x,before.y,after.x,after.y);
            }
        }
    }
}


#[test]
fn canvas_scrolled_card_borders_keep_dragging_after_editing() {
    use crate::doc_core::DocBlockKind as K;
    for zoom in [0.65, 1., 1.4] {
        for active in [false, true] {
            for side in ["left", "right"] {
                let f = Fixture::new();
                let mut state = State::from_root(f.0.clone());
                state.create_canvas_note("工作", None);
                let owner = state.create_at(242., 205.);
                let doc = &mut state.editor.active.core.doc;
                let mut table = crate::table_data::TableData::default();
                table.rows = vec![vec!["表格内容".into();3];30];
                doc.blocks = vec![doc.new_block(K::Table(table), "")];
                state.editor.active.core.dirty = true;
                crate::doc_editor::sync_to_state(&mut state);
                state.finish_edit();
                state.note_mut().unwrap().zoom = zoom;
                state.pending_scroll = Some([0., 500.*zoom]);
                let mut rt = runtime(state, (1420., 1046.));
                for _ in 0..10 { rt.frame(0.05); }
                if active {
                    let cell = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k|k.starts_with("doc-cell-")) && e.rect[1]>250. && e.rect[1]<700.).unwrap();
                    tap(&mut rt, cell.key.as_ref().unwrap());
                    type_text(&mut rt, "新内容");
                }
                let before = rt.state.note().unwrap().blocks[0].clone();
                let [x,y,w,h] = rect(&rt, &format!("block-{owner}"));
                let [_,vy,_,vh] = rect(&rt, &rt.state.scroll_key());
                let sx = if side=="left" {x} else {x+w};
                let visible_top = y.max(vy);
                let visible_bottom = (y+h).min(vy+vh);
                assert!(visible_bottom > visible_top);
                let sy = (visible_top + visible_bottom) * 0.5;
                rt.pointer_move(sx,sy); rt.frame(1.);
                assert_eq!(rt.cursor(), rune::Cursor::Move, "{zoom} {active} {side}");
                rt.pointer_down(sx,sy); rt.frame(0.05);
                for step in 1..=10 {
                    rt.pointer_move(sx+step as f32*5.*zoom, sy+step as f32*3.*zoom); rt.frame(0.02);
                }
                rt.pointer_up(sx+50.*zoom,sy+30.*zoom); rt.frame(0.1);
                let after = &rt.state.note().unwrap().blocks[0];
                assert!((after.x-before.x-50.).abs()<1. && (after.y-before.y-30.).abs()<1.,
                    "zoom={zoom} active={active} {side}: before={},{} after={},{}", before.x,before.y,after.x,after.y);
            }
        }
    }
}


#[test]
fn selected_canvas_card_border_wins_over_overlapping_editors() {
    for zoom in [0.65, 1., 1.4] {
      for editing in [false, true] {
       for table in [false, true] {
        let f = Fixture::new();
        let mut state = State::from_root(f.0.clone());
        state.create_canvas_note("工作", None);
        let first = state.create_at(142., 205.);
        state.editor.active.core.doc.blocks[0].text = crate::doc_core::RichText::plain("先创建的卡片");
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        state.finish_edit();
        let second = state.create_at(342., 205.);
        state.editor.active.core.doc.blocks[0].text = crate::doc_core::RichText::plain("后创建的卡片");
        state.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(&mut state);
        if table {
            state.editor.active.core.doc.blocks[0].kind = crate::doc_core::DocBlockKind::Table(crate::table_data::TableData::default());
            state.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(&mut state);
        }
        state.finish_edit();
        for b in &mut state.note_mut().unwrap().blocks { b.min_h = 180.; }
        state.note_mut().unwrap().zoom = zoom;
        let mut rt = runtime(state, (1420.,1046.));
        // Select the exposed left edge, then grab its visible right border where the cards overlap.
        if editing {
            let id = crate::canvas_document::session(&rt.state, &first).unwrap().core.doc.blocks[0].id;
            let [x,y,_,h] = rect(&rt, &format!("doc-text-{id}"));
            rt.pointer_down(x+10.,y+h*0.5); rt.pointer_up(x+10.,y+h*0.5); rt.frame(0.1);
            assert!(crate::containers::is_focused(&rt.state, &rt.state.note().unwrap().blocks[0]));
        } else {
            tap(&mut rt, &format!("move-edge-left-{first}"));
            assert_eq!(rt.state.selected.as_deref(), Some(first.as_str()));
        }
        let original = rt.state.note().unwrap().blocks.clone();
        let (x,y) = rt.find(&format!("move-edge-right-{first}")).unwrap();
        rt.pointer_move(x,y); rt.frame(0.2);
        assert_eq!(rt.cursor(), rune::Cursor::Move);
        rt.pointer_down(x,y); rt.frame(0.05);
        rt.pointer_move(x+50.*zoom,y+30.*zoom); rt.frame(0.05);
        rt.pointer_up(x+50.*zoom,y+30.*zoom); rt.frame(0.05);
        let blocks = &rt.state.note().unwrap().blocks;
        assert!((blocks[0].x-original[0].x-50.).abs()<1. && (blocks[0].y-original[0].y-30.).abs()<1.,
            "selected border lost the drag at zoom {zoom}: selected={:?}", rt.state.selected);
        assert_eq!(blocks[1], original[1], "overlapping card {second} changed");
        assert_eq!(blocks[0].body, original[0].body);
        let moved = blocks.clone();
        rt.state.undo(false); rt.frame(0.1);
        assert_eq!(rt.state.note().unwrap().blocks, original);
        rt.state.undo(true); rt.frame(0.1);
        assert_eq!(rt.state.note().unwrap().blocks, moved);
        rt.state.finish_edit();
        drop(rt);
        let reopened = State::from_root(f.0.clone());
        assert_eq!(reopened.note().unwrap().blocks, moved);
       }
      }
    }
}

#[path = "editor_regression.rs"]
mod editor_regression;

#[test]
fn integrated_windows_chrome_reserves_caption_space_and_keeps_toolbar_actions() {
    for mode in [WorkspaceMode::Document, WorkspaceMode::Canvas] {
        for width in [1000., 1280.] {
            for scale in [1_f32, 1.25, 1.5, 2.] {
                let f = Fixture::new();
                let mut state = State::from_root(f.0.clone());
                state.choose_mode(mode);
                let mut rt = runtime(state, (width, 736.));
                let mut chrome = rune::ChromeInsets::default();
                chrome.height = crate::ui::CHROME_HEIGHT;
                chrome.trailing = (rune::Titlebar::CAPTION_BUTTON_WIDTH * scale).round() / scale * 3.;
                rt.set_chrome_insets(chrome);
                for collapsed in [false, true] {
                    rt.update(|s, tx| { s.sidebar_collapsed = collapsed; tx.instant(); });
                    rt.frame(1.);
                    let page = rect(&rt, "editor-surface");
                    assert!(page[1] >= chrome.height);
                    for key in ["editor-new-page", if mode == WorkspaceMode::Document { "doc-more" } else { "canvas-more" }] {
                        let r = rect(&rt, key);
                        assert!(r[0] + r[2] < width - chrome.trailing, "{key}: {r:?}");
                        assert!(r[1] >= 0. && r[1] + r[3] <= chrome.height);
                    }
                    let bar = rect(&rt, "editor-toolbar");
                    let x = rect(&rt, "editor-new-page")[0] - 40.;
                    rt.pointer_down(x, bar[1] + 17.);
                    assert!(rt.take_titlebar_action().is_some(), "empty toolbar must drag the window");
                    rt.pointer_up(x, bar[1] + 17.);
                    rt.frame(0.5);
                    tap(&mut rt, "editor-new-page");
                    assert!(rt.take_titlebar_action().is_none(), "new page must not drag the window");
                    assert!(rt.state.library_dialogs.new_page.is_some());
                    press(&mut rt, Key::Escape);
                    rt.frame(0.5);
                    if collapsed {
                        tap(&mut rt, "toggle-sidebar");
                        assert!(!rt.state.sidebar_collapsed, "caption drag regions must not cover the restore button");
                        rt.frame(1.);
                    }
                }
                rt.update(|s, tx| { s.sidebar_collapsed = false; tx.instant(); });
                rt.frame(1.);
                tap(&mut rt, "settings");
                rt.frame(1.);
                assert!(rect(&rt, "settings-body")[1] >= chrome.height);
                assert!(rect(&rt, "settings-toolbar")[3] >= chrome.height);
            }
        }
    }
}


#[test]
fn right_pane_cancelled_canvas_gestures_preserve_history_and_redo() {
    for handle in ["move-edge-top", "size-handle"] {
        let (_fixture, mut rt, id) = isolated_block(Kind::Shape, 1.);
        let edge = format!("move-edge-top-{id}");
        drag_key(&mut rt, &edge, 40., 24.);
        let end_x = rt.state.block_mut(&id).unwrap().x;
        command(&mut rt, 'z', false);
        assert_ne!(rt.state.block_mut(&id).unwrap().x, end_x);
        let depth = (rt.state.undo.len(), rt.state.redo.len());
        tap(&mut rt, &edge);
        assert_eq!((rt.state.undo.len(), rt.state.redo.len()), depth, "border click");
        let (x, y) = rt.find(&format!("{handle}-{id}")).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_move(x + 30., y + 20.);
        rt.frame(0.02);
        press(&mut rt, Key::Escape);
        rt.pointer_up(x + 30., y + 20.);
        rt.frame(0.02);
        assert_eq!((rt.state.undo.len(), rt.state.redo.len()), depth, "cancel {handle}");
        command(&mut rt, 'z', true);
        assert_eq!(rt.state.block_mut(&id).unwrap().x, end_x);
    }
}

#[test]
fn right_pane_canvas_rename_reveals_the_input_with_sidebar_collapsed() {
    let (_fixture, mut rt, _) = isolated_block(Kind::Shape, 1.);
    rt.update(|s, _| s.toggle_sidebar());
    for _ in 0..40 { rt.frame(1. / 60.); }
    tap(&mut rt, "canvas-more");
    rt.resolve_popup(Some("rename")).unwrap();
    for _ in 0..40 { rt.frame(1. / 60.); }
    let [x, y, w, h] = rect(&rt, "rename-field");
    assert!(x >= 0. && w > 0. && y >= 0. && h > 0., "input is offscreen at {x},{y}");
    type_text(&mut rt, "画布已更名");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.note().unwrap().title, "画布已更名");
}


#[test]
fn right_pane_shift_drag_merge_undo_restores_original_positions() {
    let f = Fixture::new();
    let mut s = State::from_root(f.0.clone());
    s.create_canvas_note("工作", None);
    let left = s.create_at(60., 160.);
    s.editor.active.core.insert_text("左侧");
    crate::doc_editor::sync_to_state(&mut s);
    let right = s.create_at(580., 160.);
    s.editor.active.core.insert_text("右侧");
    crate::doc_editor::sync_to_state(&mut s);
    s.finish_edit();
    let mut rt = runtime(s, (1420., 1046.));
    let start = rt.state.block_mut(&right).unwrap().x;
    let delta = rt.state.block_mut(&left).unwrap().x - start + 30.;
    let depth = rt.state.undo.len();
    rt.physical_input(rune::PhysicalInput::Modifiers(Mods { shift: true, ..Default::default() }));
    drag_key(&mut rt, &format!("move-edge-top-{right}"), delta, 5.);
    rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
    assert_eq!(rt.state.note().unwrap().blocks.len(), 1);
    assert_eq!(rt.state.undo.len(), depth + 1);
    command(&mut rt, 'z', false);
    assert_eq!(rt.state.block_mut(&right).unwrap().x, start);
    assert_eq!(embedded_text(&rt.state, &right), "右侧");
    assert_eq!(embedded_text(&rt.state, &left), "左侧");
}
