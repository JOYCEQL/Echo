//! Behaviour coverage migrated from the removed native-card editor to both current hosts.
use super::*;
use crate::doc_core::{
    DocAlign, DocBlockKind as K, DocDocument, DocMark, DocPos, DocSel, RichText,
};

fn host(canvas: bool) -> (Fixture, Runtime<State>, Option<String>) {
    let fixture = Fixture::new();
    let mut state = State::from_root(fixture.0.clone());
    let card = if canvas {
        state.create_canvas_note("工作", None);
        Some(state.create_at(80., 140.))
    } else {
        state.create_doc_note("工作", None);
        let id = state.editor.active.core.doc.blocks[0].id;
        state.editor.active.core.set_head(DocPos::new(id, 0), false);
        None
    };
    let mut rt = runtime(state, (1420., 1046.));
    rt.frame(0.05);
    (fixture, rt, card)
}

fn chars(rt: &mut Runtime<State>, text: &str) {
    for ch in text.chars() {
        type_text(rt, &ch.to_string());
    }
}

fn select_body(rt: &mut Runtime<State>) -> u64 {
    let id = rt.state.editor.active.core.doc.blocks[0].id;
    rt.update(|s, _| {
        let len = s.editor.active.core.len_of(id);
        s.editor.active.core.sel = DocSel {
            anchor: DocPos::new(id, 0),
            head: DocPos::new(id, len),
        };
        s.editor.active.typing = false;
    });
    rt.frame(0.02);
    id
}

fn persisted_doc(state: &State, card: Option<&str>) -> DocDocument {
    match card {
        Some(id) => crate::file_format::card_document(
            state
                .note()
                .unwrap()
                .blocks
                .iter()
                .find(|b| b.id == id)
                .unwrap(),
        )
        .unwrap(),
        None => state.note().unwrap().doc.clone().unwrap(),
    }
}

fn table_host(canvas: bool) -> (Fixture, Runtime<State>, Option<String>, u64) {
    let (fixture, mut rt, card) = host(canvas);
    rt.update(|s, tx| crate::doc_editor::execute_slash_cmd(s, "table", tx));
    rt.frame(0.1);
    let id = rt
        .state
        .editor
        .active
        .core
        .doc
        .blocks
        .iter()
        .find(|b| matches!(b.kind, K::Table(_)))
        .unwrap()
        .id;
    (fixture, rt, card, id)
}

fn table(rt: &Runtime<State>, id: u64) -> &table_data::TableData {
    let K::Table(table) = &rt.state.editor.active.core.block(id).unwrap().kind else {
        panic!("table")
    };
    table
}

#[test]
fn unicode_editing_and_formats_survive_restart_in_both_hosts() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        type_text(&mut rt, "你好，世界 🚀");
        select_body(&mut rt);
        command(&mut rt, 'b', false);
        assert!(
            rt.state
                .editor
                .active
                .core
                .selection_has_mark(DocMark::Bold)
        );
        rt.state.finish_edit();
        let expected = persisted_doc(&rt.state, card.as_deref());
        let reopened = State::from_root(fixture.0.clone());
        let actual = persisted_doc(&reopened, card.as_deref());
        assert_eq!(actual.blocks, expected.blocks);
        assert!(actual.to_plain_text().contains("你好，世界 🚀"));
    }
}

#[test]
fn heading_shortcuts_enter_and_undo_use_one_model_in_both_hosts() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        chars(&mut rt, "## ");
        let id = rt.state.editor.active.core.sel.head.block;
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Heading { level: 2 }
        ));
        command(&mut rt, 'z', false);
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Text
        ));
        command(&mut rt, 'z', true);
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Heading { level: 2 }
        ));
        type_text(&mut rt, "中文标题");
        press(&mut rt, Key::Enter);
        let next = rt.state.editor.active.core.sel.head;
        assert_ne!(next.block, id);
        assert_eq!(next.offset, 0);
        assert!(matches!(
            rt.state.editor.active.core.block(next.block).unwrap().kind,
            K::Text
        ));
        type_text(&mut rt, "下一段");
        assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 2);
    }
}

#[test]
fn inline_markdown_shortcuts_keep_unicode_and_marks() {
    for canvas in [false, true] {
        for (source, expected, mark) in [
            ("**中文**", "中文", DocMark::Bold),
            ("*中文*", "中文", DocMark::Italic),
            ("~~中文~~", "中文", DocMark::Strike),
            ("`code`", "code", DocMark::Code),
        ] {
            let (_fixture, mut rt, _) = host(canvas);
            chars(&mut rt, source);
            select_body(&mut rt);
            assert_eq!(
                rt.state.editor.active.core.selected_text(),
                expected,
                "{canvas}/{source}"
            );
            assert!(
                rt.state.editor.active.core.selection_has_mark(mark),
                "{canvas}/{source}"
            );
        }
    }
}

#[test]
fn list_continuation_indent_empty_exit_and_tasks_work_in_both_hosts() {
    for canvas in [false, true] {
        for source in ["- ", "1. ", "- [ ] "] {
            let (_fixture, mut rt, _) = host(canvas);
            chars(&mut rt, source);
            type_text(&mut rt, "第一项");
            press(&mut rt, Key::Enter);
            type_text(&mut rt, "第二项");
            let id = rt.state.editor.active.core.sel.head.block;
            press(&mut rt, Key::Tab);
            assert!(rt.state.editor.active.core.parent_of(id).is_some());
            rt.key(KeyEvent {
                key: Key::Tab,
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
            });
            rt.frame(0.02);
            assert!(rt.state.editor.active.core.parent_of(id).is_none());
            press(&mut rt, Key::Enter);
            press(&mut rt, Key::Enter);
            let id = rt.state.editor.active.core.sel.head.block;
            assert!(matches!(
                rt.state.editor.active.core.block(id).unwrap().kind,
                K::Text
            ));
        }
    }
}

#[test]
fn heading_ime_preedit_never_reaches_disk_before_commit() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        chars(&mut rt, "## ");
        rt.update(|s, tx| {
            assert!(crate::doc_editor::on_ime(
                s,
                &rune::Ime::Preedit("zhongwen".into(), Some((8, 8))),
                tx
            ));
        });
        rt.frame(0.02);
        assert!(rt.state.editor.active.core.preedit.is_some());
        assert!(
            !persisted_doc(&rt.state, card.as_deref())
                .to_plain_text()
                .contains("zhongwen")
        );
        rt.update(|s, tx| {
            assert!(crate::doc_editor::on_ime(
                s,
                &rune::Ime::Commit("中文".into()),
                tx
            ));
        });
        rt.frame(0.02);
        let id = rt.state.editor.active.core.sel.head.block;
        assert_eq!(
            rt.state.editor.active.core.block(id).unwrap().text.text,
            "中文"
        );
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Heading { level: 2 }
        ));
        rt.state.finish_edit();
        let reopened = State::from_root(fixture.0.clone());
        assert!(
            persisted_doc(&reopened, card.as_deref())
                .to_plain_text()
                .contains("中文")
        );
    }
}

#[test]
fn slash_keyboard_wrap_filter_escape_tab_and_undo_work_in_both_hosts() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        chars(&mut rt, "/");
        press(&mut rt, Key::Up);
        assert_eq!(
            rt.state.editor.active.slash.as_ref().unwrap().selected,
            crate::insert_menu::SLASH_CMDS.len() - 1
        );
        press(&mut rt, Key::Down);
        assert_eq!(rt.state.editor.active.slash.as_ref().unwrap().selected, 0);
        type_text(&mut rt, "不存在");
        assert!(rt.state.editor.active.filtered_slash_cmds().is_empty());
        press(&mut rt, Key::Escape);
        assert!(rt.state.editor.active.slash.is_none());
        assert!(
            rt.state
                .editor
                .active
                .core
                .doc
                .to_plain_text()
                .contains("/不存在")
        );
        select_body(&mut rt);
        type_text(&mut rt, "");
        press(&mut rt, Key::Backspace);
        chars(&mut rt, "、h2");
        press(&mut rt, Key::Tab);
        let id = rt.state.editor.active.core.sel.head.block;
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Heading { level: 2 }
        ));
        command(&mut rt, 'z', false);
        assert!(
            rt.state
                .editor
                .active
                .core
                .doc
                .to_plain_text()
                .contains("、h2")
        );
        command(&mut rt, 'z', true);
        assert!(matches!(
            rt.state.editor.active.core.block(id).unwrap().kind,
            K::Heading { level: 2 }
        ));
    }
}

#[test]
fn slash_ignores_paths_urls_inline_code_and_code_blocks() {
    for canvas in [false, true] {
        for input in ["https://example.com/a", "folder/path"] {
            let (_fixture, mut rt, _) = host(canvas);
            chars(&mut rt, input);
            assert!(rt.state.editor.active.slash.is_none());
        }
        let (_fixture, mut rt, _) = host(canvas);
        rt.update(|s, tx| crate::doc_editor::execute_slash_cmd(s, "code", tx));
        chars(&mut rt, "/todo");
        assert!(rt.state.editor.active.slash.is_none());
    }
}

#[test]
fn shared_toolbar_keeps_selection_colours_links_and_alignment_on_disk() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        type_text(&mut rt, "前中文后");
        let id = select_body(&mut rt);
        let selection = rt.state.editor.active.core.sel.clone();
        tap(&mut rt, "tb-bold");
        assert_eq!(rt.state.editor.active.core.sel, selection);
        assert!(
            rt.state
                .editor
                .active
                .core
                .selection_has_mark(DocMark::Bold)
        );
        tap(&mut rt, "tb-color");
        tap(&mut rt, "text-color-red");
        tap(&mut rt, "tb-color");
        tap(&mut rt, "highlight-color-yellow");
        tap(&mut rt, "tb-align");
        tap(&mut rt, "居中对齐");
        assert_eq!(
            rt.state.editor.active.core.block(id).unwrap().align,
            DocAlign::Center
        );
        tap(&mut rt, "tb-link");
        type_text(&mut rt, "https://example.com");
        press(&mut rt, Key::Enter);
        let marks = rt
            .state
            .editor
            .active
            .core
            .block(id)
            .unwrap()
            .text
            .marks_at(0);
        assert!(marks.bold);
        assert_eq!(marks.color.as_deref(), Some("red"));
        assert_eq!(marks.bg.as_deref(), Some("yellow"));
        assert_eq!(marks.link.as_deref(), Some("https://example.com"));
        rt.state.finish_edit();
        let reopened = State::from_root(fixture.0.clone());
        let doc = persisted_doc(&reopened, card.as_deref());
        assert_eq!(doc.blocks[0].text.marks_at(0), marks);
        assert_eq!(doc.blocks[0].align, DocAlign::Center);
    }
}

#[test]
fn settings_restores_text_selection_and_cell_focus_in_both_hosts() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        type_text(&mut rt, "前中后");
        let id = rt.state.editor.active.core.sel.head.block;
        rt.update(|s, _| {
            s.editor
                .active
                .core
                .set_head(DocPos::new(id, "前中".len()), false)
        });
        command(&mut rt, ',', false);
        assert!(rt.state.settings.is_some());
        press(&mut rt, Key::Escape);
        type_text(&mut rt, "文");
        assert_eq!(
            rt.state.editor.active.core.block(id).unwrap().text.text,
            "前中文后"
        );

        let (_fixture, mut rt, _, id) = table_host(canvas);
        tap(&mut rt, &format!("doc-cell-box-{id}-0-0"));
        type_text(&mut rt, "第一格");
        command(&mut rt, ',', false);
        press(&mut rt, Key::Escape);
        assert!(rt.state.editor.active.editing_cell.is_some());
        type_text(&mut rt, "继续");
        assert!(table(&rt, id).rows[0][0].contains("继续"));
        assert_eq!(table(&rt, id).rows[0][1], "");
    }
}

#[test]
fn table_tsv_multiline_expansion_tab_and_history_match_both_hosts() {
    for canvas in [false, true] {
        let (fixture, mut rt, card, id) = table_host(canvas);
        rt.update(|s, tx| crate::doc_tables::begin(s, id, 2, 2, tx));
        rt.frame(0.02);
        press(&mut rt, Key::Tab);
        assert_eq!(table(&rt, id).rows.len(), 4);
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods {
                shift: true,
                ..Default::default()
            },
        });
        rt.frame(0.02);
        assert!(
            rt.state
                .editor
                .active
                .editing_cell
                .as_ref()
                .is_some_and(|(_, r, c, _)| (*r, *c) == (2, 2))
        );
        rt.update(|s, _| {
            crate::doc_tables::paste(s, id, 0, 0, "\"甲\n乙\"\t丙\t丁\t戊\n一\t二\t三\t四")
        });
        assert_eq!(table(&rt, id).rows[0][0], "甲\n乙");
        assert_eq!(table(&rt, id).cols(), 4);
        assert_eq!(table(&rt, id).rows[1][3], "四");
        let expected = table(&rt, id).clone();
        rt.update(|s, _| {
            s.editor.active.commit_cell_edit();
            assert!(s.editor.active.core.undo());
            crate::doc_editor::sync_to_state(s);
        });
        assert_ne!(table(&rt, id), &expected);
        rt.update(|s, _| {
            assert!(s.editor.active.core.redo());
            crate::doc_editor::sync_to_state(s);
        });
        assert_eq!(table(&rt, id), &expected);
        rt.state.finish_edit();
        let reopened = State::from_root(fixture.0.clone());
        let doc = persisted_doc(&reopened, card.as_deref());
        assert!(
            doc.blocks
                .iter()
                .any(|b| matches!(&b.kind, K::Table(t) if t == &expected))
        );
    }
}

#[test]
fn table_ime_undo_and_literal_slash_do_not_invoke_block_commands() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _, id) = table_host(canvas);
        tap(&mut rt, &format!("doc-cell-box-{id}-0-0"));
        rt.ime(rune::Ime::Preedit("zhongwen".into(), Some((8, 8))));
        rt.frame(0.02);
        assert_eq!(table(&rt, id).rows[0][0], "");
        rt.ime(rune::Ime::Commit("中文".into()));
        rt.frame(0.02);
        assert_eq!(table(&rt, id).rows[0][0], "中文");
        command(&mut rt, 'z', false);
        assert_eq!(table(&rt, id).rows[0][0], "");
        command(&mut rt, 'z', true);
        assert_eq!(table(&rt, id).rows[0][0], "中文");
        chars(&mut rt, "/todo");
        assert!(rt.state.editor.active.slash.is_none());
        assert_eq!(table(&rt, id).rows[0][0], "中文/todo");
    }
}

#[test]
fn code_clipboard_and_native_rich_paste_preserve_source() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        rt.update(|s, tx| crate::doc_editor::execute_slash_cmd(s, "code", tx));
        let source =
            "type Note = { title: string; done: boolean };\nconst x = \"中文\";\nconsole.log(x);";
        rt.paste(source);
        rt.frame(0.02);
        select_body(&mut rt);
        assert_eq!(
            rt.state.editor.active.core.clipboard_content().text(),
            source
        );
        let content =
            rune::ClipboardContent::new(source).with_html("<pre><code>wrong HTML</code></pre>");
        rt.update(|s, tx| crate::doc_editor::on_paste(s, &content, tx));
        assert_eq!(rt.state.editor.active.core.doc.blocks[0].text.text, source);
    }
}

#[test]
fn rich_colour_overlaps_roundtrip_without_crossed_markup() {
    let mut rich = RichText::plain("甲乙丙丁");
    rich.update_marks(0.."甲乙丙".len(), |m| {
        m.bold = true;
        m.color = Some("red".into());
    });
    rich.update_marks("甲".len()..rich.len(), |m| {
        m.bg = Some("yellow".into());
        m.link = Some("https://example.com?a=1&b=2".into());
    });
    let html = crate::file_format::rich_html(rich.clone());
    let edit = rune::TextEdit::from_html(&html);
    let mut core = crate::doc_core::DocCore::new(DocDocument::empty());
    core.include_title = false;
    let id = core.doc.blocks[0].id;
    core.set_head(DocPos::new(id, 0), false);
    core.paste_native(&edit);
    assert_eq!(core.doc.to_plain_text(), rich.text);
    let actual = &core.doc.blocks[0].text;
    assert!(actual.marks_at(0).bold);
    assert_eq!(actual.marks_at("甲乙".len()).bg.as_deref(), Some("#fff5c2"));
    assert!(!actual.marks_at("甲乙丙丁".len()).bold);
    assert!(core.undo());
    assert!(core.doc.to_plain_text().is_empty());
    assert!(core.redo());
    assert_eq!(core.doc.to_plain_text(), rich.text);
}

#[test]
fn suspended_card_history_and_selection_remain_owned_by_their_card() {
    let (_fixture, mut rt, first) = host(true);
    let first = first.unwrap();
    type_text(&mut rt, "甲卡片");
    let first_pos = rt.state.editor.active.core.sel;
    let second = rt.state.create_at(520., 240.);
    rt.frame(0.02);
    type_text(&mut rt, "乙卡片");
    let second_id = rt.state.editor.active.core.sel.head.block;
    assert_ne!(first_pos.head.block, second_id);
    rt.update(|s, _| crate::canvas_document::activate(s, &first));
    rt.frame(0.02);
    assert_eq!(rt.state.editor.active.core.sel, first_pos);
    command(&mut rt, 'z', false);
    assert!(
        !rt.state
            .editor
            .active
            .core
            .doc
            .to_plain_text()
            .contains("甲卡片")
    );
    assert_eq!(
        rt.state
            .editor
            .card(&second)
            .unwrap()
            .core
            .doc
            .to_plain_text(),
        "乙卡片"
    );
    command(&mut rt, 'z', true);
    assert_eq!(rt.state.editor.active.core.doc.to_plain_text(), "甲卡片");
}

#[test]
fn markdown_paste_tables_code_and_escaped_cells_roundtrip_in_both_hosts() {
    let source = "前文\n\n| 名称 | 说明 |\n| --- | --- |\n| a\\|b | 第一行<br>第二行 |\n\n```md\n| x | y |\n| --- | --- |\n```\n\n正文 | 不应成为表格\n\n| C | D |\n| --- | --- |\n| ![图](https://example.com/a.png) | 尾部 |\n";
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        rt.paste(source);
        rt.frame(0.03);
        let blocks = &rt.state.editor.active.core.doc.blocks;
        let tables: Vec<_> = blocks
            .iter()
            .filter_map(|b| {
                if let K::Table(t) = &b.kind {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].rows[1], ["a|b", "第一行\n第二行"]);
        assert!(tables[1].rows[1][0].contains("https://example.com/a.png"));
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, K::Code { .. }) && b.text.text.contains("| --- | --- |"))
        );
        assert!(
            blocks
                .iter()
                .any(|b| b.text.text.contains("正文 | 不应成为表格"))
        );
        command(&mut rt, 'z', false);
        assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 1);
        assert!(
            rt.state.editor.active.core.doc.blocks[0]
                .text
                .text
                .is_empty()
        );
        command(&mut rt, 'z', true);
        assert_eq!(
            rt.state
                .editor
                .active
                .core
                .doc
                .blocks
                .iter()
                .filter(|b| matches!(b.kind, K::Table(_)))
                .count(),
            2
        );
        rt.state.finish_edit();
        let expected = persisted_doc(&rt.state, card.as_deref());
        let reopened = State::from_root(fixture.0.clone());
        assert_eq!(
            persisted_doc(&reopened, card.as_deref()).blocks,
            expected.blocks
        );
    }
}

#[test]
fn export_reads_offscreen_card_images_without_render_cache_and_reports_missing_files() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        let bytes = include_bytes!("../assets/architecture.png");
        std::fs::create_dir_all(fixture.0.join("images")).unwrap();
        std::fs::write(fixture.0.join("images/export-test.png"), bytes).unwrap();
        rt.paste("第一段\n\n第二段  \n软换行\n\n![图](images/export-test.png)\n\n```ts\nconst x = { title: \"中文\" };\n```\n");
        rt.frame(0.02);
        rt.state.finish_edit();
        assert!(rt.state.cache.is_none());
        if let Some(card) = &card {
            rt.state.editor.remove_card(card);
        }
        let output = fixture.0.join("export.md");
        rt.state.export_markdown(&output);
        assert!(output.exists(), "{:?}", rt.state.message);
        assert_eq!(
            std::fs::read(fixture.0.join("export.assets/export-test.png")).unwrap(),
            bytes
        );
        let text = std::fs::read_to_string(&output).unwrap();
        assert!(text.contains("const x = { title: \"中文\" };"));
        assert!(text.contains("export.assets/export-test.png"));
        let mut next = 1;
        let parsed = crate::file_format::parse_standard(&text, &mut next).unwrap();
        assert!(parsed.iter().any(|b| b.text.text == "第二段\n软换行"));
        std::fs::remove_file(fixture.0.join("images/export-test.png")).unwrap();
        let failed = fixture.0.join("missing.md");
        rt.state.export_markdown(&failed);
        assert!(!failed.exists());
        assert!(rt.state.message.as_deref().unwrap().contains("无法导出"));
    }
}

#[test]
fn every_new_textual_card_uses_current_format_and_unmounted_cards_merge() {
    let fixture = Fixture::new();
    let mut s = State::from_root(fixture.0.clone());
    s.create_canvas_note("工作", None);
    for kind in [Kind::Tasks, Kind::Code, Kind::Callout, Kind::Table] {
        s.add(kind);
        let card = s.note().unwrap().blocks.last().unwrap();
        assert!(card.structured);
        assert_eq!(card.kind, Kind::Text);
        assert!(card.title.is_empty() && card.body_html.is_empty() && card.table.is_none());
        assert!(
            !crate::file_format::card_document(card)
                .unwrap()
                .blocks
                .is_empty()
        );
    }
    s.finish_edit();
    let ids: Vec<_> = s
        .note()
        .unwrap()
        .blocks
        .iter()
        .map(|b| b.id.clone())
        .collect();
    s.editor.clear();
    s.merge_containers(&ids[1], &ids[0]);
    assert_eq!(s.note().unwrap().blocks.len(), 3);
    let merged = crate::file_format::card_document(&s.note().unwrap().blocks[0]).unwrap();
    assert!(
        merged
            .blocks
            .iter()
            .any(|b| matches!(b.kind, K::Todo { .. }))
    );
    assert!(
        merged
            .blocks
            .iter()
            .any(|b| matches!(b.kind, K::Code { .. }))
    );
    s.undo(false);
    assert_eq!(s.note().unwrap().blocks.len(), 4);
}

#[test]
fn file_payloads_decode_to_one_editor_and_invalid_payloads_are_explicit_errors() {
    let mut cards = vec![
        Block::new(Kind::Text, 0., 0., 360., 100., "标题", "**加粗**"),
        Block::new(Kind::Tasks, 0., 0., 360., 100., "", "- [x] 已完成"),
        Block::new(Kind::Code, 0., 0., 360., 100., "ts", "const x = 1;"),
        Block::new(Kind::Callout, 0., 0., 360., 100., "", "高亮内容"),
        Block::new(Kind::Table, 0., 0., 360., 100., "", ""),
    ];
    cards[4].table = Some(table_data::TableData::default());
    let mut sessions = crate::editor_sessions::Sessions::default();
    let mut all_ids = std::collections::HashSet::new();
    for card in &cards {
        let doc = crate::file_format::card_document(card).unwrap();
        sessions.mount_card("test", &card.id, doc, None);
        assert!(sessions.activate_card(&card.id));
        assert!(!sessions.active.core.include_title);
        for unit in sessions.active.core.units() {
            assert!(all_ids.insert(unit.id));
        }
    }
    let card = &cards[0];
    sessions.clear();
    sessions.mount_card(
        "next-note",
        &card.id,
        crate::file_format::card_document(card).unwrap(),
        None,
    );
    assert!(sessions.activate_card(&card.id));
    for unit in sessions.active.core.units() {
        assert!(all_ids.insert(unit.id));
    }
    let mut invalid = card.clone();
    invalid.body_html = "<p>unimported</p>".into();
    assert!(crate::file_format::card_document(&invalid).is_err());
    invalid = cards[4].clone();
    invalid.table = None;
    assert!(crate::file_format::card_document(&invalid).is_err());
}

#[test]
fn table_padding_context_structure_and_undo_work_in_both_hosts() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _, id) = table_host(canvas);
        let [x, y, _, _] = rect(&rt, &format!("doc-cell-box-{id}-1-1"));
        rt.pointer_down(x + 3., y + 3.);
        rt.frame(0.02);
        rt.pointer_up(x + 3., y + 3.);
        rt.frame(0.02);
        assert!(
            rt.state
                .editor
                .active
                .editing_cell
                .as_ref()
                .is_some_and(|(bid, r, c, _)| (*bid, *r, *c) == (id, 1, 1))
        );
        type_text(&mut rt, "保留数据");
        press(&mut rt, Key::Escape);
        for (action, rows, cols) in [
            ("row-before", 4, 3),
            ("col-after", 4, 4),
            ("del-row", 3, 4),
            ("del-col", 3, 3),
        ] {
            let (x, y) = rt.find(&format!("doc-cell-box-{id}-0-0")).unwrap();
            rt.secondary_down(x, y);
            rt.frame(0.02);
            rt.secondary_up(x, y);
            rt.frame(0.02);
            rt.resolve_popup(Some(action)).unwrap();
            rt.frame(0.02);
            assert_eq!(
                (table(&rt, id).rows.len(), table(&rt, id).cols()),
                (rows, cols)
            );
        }
        for _ in 0..4 {
            command(&mut rt, 'z', false);
        }
        assert_eq!(table(&rt, id).rows[1][1], "保留数据");
        assert_eq!((table(&rt, id).rows.len(), table(&rt, id).cols()), (3, 3));
    }
}


#[test]
fn slash_menu_position_is_stable_from_first_frame_at_all_canvas_zooms() {
    for canvas in [false, true] {
        for zoom in [0.75, 1., 1.5] {
            for trigger in ["/", "、"] {
                let (_fixture, mut rt, _) = host(canvas);
                rt.update(|s, tx| { s.note_mut().unwrap().zoom = zoom; tx.instant(); });
                for _ in 0..40 { rt.frame(1. / 60.); }
                rt.key(KeyEvent { key: Key::Text(trigger.into()), mods: Mods::default() });
                let mut first: Option<[f32; 4]> = None;
                for frame in 0..72 {
                    if frame > 0 { rt.update(|_, tx| tx.instant()); }
                    rt.frame(1. / 120.);
                    if let Some(menu) = rt.elements().iter().find(|e| e.key.as_deref() == Some("doc-slash-menu")) {
                        let anchor = first.get_or_insert(menu.rect);
                        for axis in 0..2 {
                            assert!((anchor[axis] - menu.rect[axis]).abs() < 0.1,
                                "canvas={canvas} zoom={zoom} trigger={trigger} frame={frame}: {anchor:?} -> {:?}", menu.rect);
                        }
                    }
                }
                assert!(first.is_some(), "menu must appear");
            }
        }
    }
}

#[test]
fn slash_menu_uses_same_frame_trigger_geometry_for_wrap_filter_zoom_and_scroll() {
    for canvas in [false, true] {
        for zoom in [0.75, 1.5] {
            let (_fixture, mut rt, _) = host(canvas);
            rt.set_viewport((1100., 736.), 1.);
            rt.update(|s, tx| { s.note_mut().unwrap().zoom = zoom; tx.instant(); });
            type_text(&mut rt, &format!("{} ", "换行定位与菜单 ".repeat(45)));
            for _ in 0..30 { rt.frame(1./60.); }
            rt.key(KeyEvent { key: Key::Text("/".into()), mods: Mods::default() });
            for frame in 0..36 {
                if frame == 12 {
                    rt.key(KeyEvent { key: Key::Text("text".into()), mods: Mods::default() });
                }
                if frame == 24 {
                    rt.update(|s, tx| { let p = s.scroll_pos.get(); tx.scroll_to_instant(s.scroll_key(), p.offset[0], (p.offset[1]-35.).max(0.)); });
                }
                rt.update(|_, tx| tx.instant());
                rt.frame(1./120.);
                let anchor = rect(&rt, "doc-slash-anchor");
                let menu = rect(&rt, "doc-slash-menu");
                let expected = rune::anchor_rect([anchor[0], anchor[1], anchor[0]+anchor[2], anchor[1]+anchor[3]],
                    (menu[2], menu[3]), rune::Side::Bottom, rune::Align::Start, 6., (1100., 736.));
                assert!((menu[0]-expected[0]).abs()<0.2 && (menu[1]-expected[1]).abs()<0.2,
                    "canvas={canvas} zoom={zoom} frame={frame}: anchor={anchor:?} menu={menu:?} expected={expected:?}");
                assert!((menu[2]-247.).abs()<0.1, "canvas zoom must not scale menu width");
                assert!(menu[0]>=7.9 && menu[0]+menu[2]<=1092.1 && menu[1]>=7.9 && menu[1]+menu[3]<=728.1,
                    "menu must stay inside the window: {menu:?}");
            }
        }
    }
}

#[test]
fn slash_menu_does_not_reanchor_after_pointer_and_layout_updates() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        for _ in 0..40 { rt.frame(1. / 60.); }
        rt.key(KeyEvent { key: Key::Text("/".into()), mods: Mods::default() });
        rt.frame(1. / 120.);
        let first = rect(&rt, "doc-slash-menu");
        for frame in 0..60 {
            rt.pointer_move(900. + frame as f32, 90.);
            rt.update(|_, tx| tx.instant());
            rt.frame(1. / 120.);
            let next = rect(&rt, "doc-slash-menu");
            assert!((first[0]-next[0]).abs() < 0.1 && (first[1]-next[1]).abs() < 0.1,
                "canvas={canvas} frame={frame}: {first:?} -> {next:?}; caret={:?}", rt.state.editor.active.caret_window_rect());
        }
    }
}


fn finish_formulas(rt: &mut Runtime<State>) {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f32() < 2. {
        rt.frame(0.02);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn rich_previews_edit_copy_undo_and_reopen_in_both_hosts() {
    for canvas in [false, true] {
        for (cmd, source) in [("math", r"\frac{a_1+b^2}{\sqrt{x}}"), ("mermaid", "flowchart LR\nA[开始] --> B[完成]")] {
            let (fixture, mut rt, card) = host(canvas);
            crate::setup(&mut rt.state, std::sync::Arc::new(|| {}));
            rt.update(|s, tx| crate::doc_editor::execute_slash_cmd(s, cmd, tx));
            let id = rt.state.editor.active.core.sel.head.block;
            rt.paste(source);
            rt.frame(0.02);
            tap(&mut rt, &format!("code-preview-{id}"));
            finish_formulas(&mut rt);
            assert!(rt.find(&format!("rich-preview-{id}")).is_some(), "{canvas} {cmd}");
            tap(&mut rt, &format!("preview-copy-{id}"));
            assert_eq!(rt.copy_request.as_ref().map(|c| c.content().text()), Some(source), "copy must preserve source");
            tap(&mut rt, &format!("preview-align-center-{id}"));
            assert_eq!(rt.state.editor.active.core.block(id).unwrap().align, DocAlign::Center);
            tap(&mut rt, &format!("preview-align-right-{id}"));
            assert_eq!(rt.state.editor.active.core.block(id).unwrap().align, DocAlign::Right);
            tap(&mut rt, &format!("preview-align-left-{id}"));
            assert_eq!(rt.state.editor.active.core.block(id).unwrap().align, DocAlign::Left);
            tap(&mut rt, &format!("preview-edit-{id}"));
            assert!(rt.find(&format!("doc-text-{id}")).is_some());
            assert_eq!(rt.state.editor.active.core.sel.head, DocPos::new(id, 0));
            type_text(&mut rt, "X");
            command(&mut rt, 'z', false);
            assert_eq!(rt.state.editor.active.core.text_of(id).unwrap().text, source);
            rt.update(|s, _| { s.finish_edit(); s.save(); });
            let expected = persisted_doc(&rt.state, card.as_deref());
            let note_id = rt.state.current.clone();
            drop(rt);
            let mut reopened = State::from_root(fixture.0.clone());
            reopened.select_note(&note_id);
            assert_eq!(persisted_doc(&reopened, card.as_deref()).blocks, expected.blocks);
        }
    }
}
#[test]
fn inline_formula_preview_maps_clicks_back_to_utf8_source() {
    let (_fixture, mut rt, _) = host(false);
    crate::setup(&mut rt.state, std::sync::Arc::new(|| {}));
    let source = r"中文 $\frac{a}{b}$ 之后 $x^2$ 结束";
    type_text(&mut rt, source);
    let id = rt.state.editor.active.core.sel.head.block;
    rt.update(|s, _| s.editor.active.focused = false);
    finish_formulas(&mut rt);
    let meta = rt.state.editor.active.text_layout(id).unwrap();
    let preview = meta.inline_preview.as_ref().expect("inline preview mounted");
    assert_eq!(preview.images.len(), 2);
    assert_eq!(preview.display_to_raw(preview.text.len()), source.len());
    for (src, dst) in &preview.replacements {
        assert_eq!(preview.raw_to_display(src.start), dst.start);
        assert_eq!(preview.display_to_raw(dst.start), src.start);
        assert_eq!(preview.display_to_raw(dst.end), src.end);
    }
    let source_end = rt.state.editor.active.hit_test_window(meta.rect[0] + meta.rect[2] - 1., meta.rect[1] + meta.rect[3] * 0.5);
    assert_eq!(source_end, DocPos::new(id, source.len()));
    rt.pointer_down(meta.rect[0] + 4., meta.rect[1] + meta.rect[3] * 0.5);
    rt.pointer_up(meta.rect[0] + 4., meta.rect[1] + meta.rect[3] * 0.5);
    rt.frame(0.02);
    assert!(rt.state.editor.active.text_layout(id).unwrap().inline_preview.is_none());
    assert_eq!(rt.state.editor.active.core.text_of(id).unwrap().text, source);
}

#[test]
#[ignore = "writes an isolated visual QA library at ECHO_RICH_FIXTURE_DIR"]
fn rich_preview_visual_fixture() {
    let root = std::path::PathBuf::from(std::env::var("ECHO_RICH_FIXTURE_DIR").expect("explicit fixture path"));
    assert!(!root.exists(), "fixture directory must be new");
    let mut state = State::from_root(root);
    state.create_doc_note("工作", None);
    let source = "## 原生图表与公式\n\n行内公式 $E=mc^2$，分式 $\\frac{a+b}{c}$ 与正文对齐。\n\n```mermaid\nflowchart LR\nA[记录想法] --> B{整理}\nB --> C[文档]\nB --> D[自由画布]\n```\n\n$$\n\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}\n$$\n\n```math\n\\begin{pmatrix}a & b \\\\ c & d\\end{pmatrix}\\begin{pmatrix}x\\\\y\\end{pmatrix}\n```\n\n结尾正文，可继续编辑。";
    let mut doc = DocDocument::empty();
    doc.title = RichText::plain("Mermaid 与数学公式");
    doc.blocks = crate::file_format::parse_standard(source, &mut doc.next_id).unwrap();
    let doc_id = state.current.clone();
    state.editor.active.load(&doc_id, doc.clone(), false);
    state.editor.active.core.dirty = true;
    crate::doc_editor::sync_to_state(&mut state);
    state.save();
    state.create_canvas_note("工作", None);
    state.create_at(60., 100.);
    let note = state.current.clone();
    state.editor.active.load(&note, doc, false);
    state.editor.active.core.include_title = false;
    state.editor.active.core.dirty = true;
    crate::doc_editor::sync_to_state(&mut state);
    state.save();
    state.select_note(&doc_id);
    state.save();
    eprintln!("visual fixture: {}", state.store.root.display());
}


#[test]
fn right_pane_save_keeps_typing_and_cell_focus_in_both_editors() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        type_text(&mut rt, "甲");
        command(&mut rt, 's', false);
        type_text(&mut rt, "乙");
        command(&mut rt, 's', false);
        let saved = State::from_root(fixture.0.clone());
        assert_eq!(persisted_doc(&saved, card.as_deref()).blocks[0].text.text, "甲乙");

        let (_fixture, mut rt, _, id) = table_host(canvas);
        tap(&mut rt, &format!("doc-cell-{id}-0-0"));
        type_text(&mut rt, "甲");
        command(&mut rt, 's', false);
        assert!(rt.state.editor.active.editing_cell.is_some(), "saving ended cell editing");
        type_text(&mut rt, "乙");
        command(&mut rt, 's', false);
        assert!(table(&rt, id).rows[0][0].contains("甲乙"));
    }
}

#[test]
fn right_pane_outline_opens_hidden_heading_and_accepts_typing() {
    let (fixture, mut rt, _) = host(false);
    let mut doc = DocDocument::empty();
    doc.blocks.clear();
    for _ in 0..120 {
        let b = doc.new_block(K::Text, "正文");
        doc.blocks.push(b);
    }
    let mut parent = doc.new_block(K::Heading { level: 1 }, "章节");
    parent.folded = true;
    let parent_id = parent.id;
    doc.blocks.push(parent);
    let mut quote = doc.new_block(K::Quote, "");
    let heading = doc.new_block(K::Heading { level: 2 }, "目标");
    let target = heading.id;
    quote.children = vec![heading];
    doc.blocks.push(quote);
    rt.update(|s, _| {
        s.editor.active.load(&s.current.clone(), doc, false);
        s.editor.active.core.dirty = true;
        crate::doc_editor::sync_to_state(s);
        s.toggle_document_outline();
    });
    rt.frame(0.05);
    tap(&mut rt, "outline-item-1");
    for _ in 0..60 { rt.frame(1. / 60.); }
    assert!(!rt.state.editor.active.core.block(parent_id).unwrap().folded);
    let [_, y, _, h] = rect(&rt, &format!("doc-text-{target}"));
    let [_, top, _, height] = rt.state.editor.active.viewport_rect.unwrap();
    assert!(y < top + height && y + h > top, "target remained offscreen");
    type_text(&mut rt, "到达");
    command(&mut rt, 's', false);
    let saved = State::from_root(fixture.0.clone());
    assert_eq!(saved.editor.active.core.block(target).unwrap().text.text, "到达目标");
}


#[test]
fn right_pane_ime_replacement_cancel_save_commit_and_undo() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        type_text(&mut rt, "甲🙂乙");
        let row = select_body(&mut rt);
        let sel = DocSel { anchor: DocPos::new(row, 7), head: DocPos::new(row, 3) };
        rt.update(|s, _| s.editor.active.core.sel = sel);
        rt.frame(0.02);
        assert!(rt.ime_area.is_some(), "selecting text must keep the OS input method enabled");
        let ime = |rt: &mut Runtime<State>, event| {
            rt.update(|s, tx| { assert!(crate::doc_editor::on_ime(s, &event, tx)); });
            rt.frame(0.02);
        };
        ime(&mut rt, rune::Ime::Preedit("ni".into(), Some((2, 2))));
        assert_eq!(rt.state.editor.active.core.block(row).unwrap().text.text, "甲🙂乙");
        assert_eq!(rt.state.editor.active.text_layout(row).unwrap().display_text, "甲ni乙");
        command(&mut rt, 's', false);
        let saved = State::from_root(fixture.0.clone());
        assert_eq!(persisted_doc(&saved, card.as_deref()).blocks[0].text.text, "甲🙂乙");
        ime(&mut rt, rune::Ime::Disabled);
        assert!(rt.state.editor.active.core.preedit.is_none());
        assert_eq!(rt.state.editor.active.core.sel, sel);
        ime(&mut rt, rune::Ime::Preedit("ni".into(), None));
        ime(&mut rt, rune::Ime::Commit("你".into()));
        assert_eq!(rt.state.editor.active.core.block(row).unwrap().text.text, "甲你乙");
        command(&mut rt, 'z', false);
        assert_eq!(rt.state.editor.active.core.block(row).unwrap().text.text, "甲🙂乙");
    }
}

#[test]
fn right_pane_image_write_failure_is_visible_and_preserves_content() {
    let (fixture, mut rt, _) = host(false);
    type_text(&mut rt, "已有正文");
    let before = rt.state.editor.active.core.doc.clone();
    let images = fixture.0.join("images");
    if images.exists() { std::fs::rename(&images, fixture.0.join("previous-images")).unwrap(); }
    std::fs::write(&images, "占用目标目录").unwrap();
    let bytes = include_bytes!("../assets/architecture.png").as_slice();
    let content = rune::ClipboardContent::default()
        .with_image(rune::ClipboardImage::from_png(bytes).unwrap());
    rt.update(|s, tx| crate::doc_editor::on_paste(s, &content, tx));
    assert!(rt.state.message.as_deref().is_some_and(|s| s.contains("图片")));
    assert_eq!(rt.state.editor.active.core.doc, before);
    rt.update(|s, tx| { s.message = None; crate::doc_editor::execute_slash_cmd(s, "image", tx); });
    let source = fixture.0.join("source.png");
    std::fs::write(&source, bytes).unwrap();
    rt.resolve_file_dialog(Some(source)).unwrap();
    rt.frame(0.02);
    assert!(rt.state.message.as_deref().is_some_and(|s| s.contains("图片")));
    assert_eq!(rt.state.editor.active.core.doc, before);
}


#[test]
fn right_pane_multiline_ime_replacement_remains_reversible() {
    for canvas in [false, true] {
        let (_fixture, mut rt, _) = host(canvas);
        type_text(&mut rt, "甲乙");
        press(&mut rt, Key::Enter);
        type_text(&mut rt, "丙丁");
        let first = rt.state.editor.active.core.doc.blocks[0].id;
        let last = rt.state.editor.active.core.doc.blocks[1].id;
        let selection = DocSel { anchor: DocPos::new(last, 3), head: DocPos::new(first, 3) };
        rt.update(|s, _| s.editor.active.core.sel = selection);
        rt.update(|s, tx| { crate::doc_editor::on_ime(s, &rune::Ime::Preedit("xin".into(), None), tx); });
        rt.frame(0.02);
        assert_eq!(rt.state.editor.active.text_layout(first).unwrap().display_text, "甲xin");
        assert_eq!(rt.state.editor.active.text_layout(last).unwrap().display_text, "丁");
        press(&mut rt, Key::Escape);
        assert_eq!(rt.state.editor.active.core.sel, selection);
        assert_eq!(rt.state.editor.active.text_layout(last).unwrap().display_text, "丙丁");
        rt.update(|s, tx| { crate::doc_editor::on_ime(s, &rune::Ime::Commit("新".into()), tx); });
        rt.frame(0.02);
        assert_eq!(rt.state.editor.active.core.doc.blocks.len(), 1);
        assert_eq!(rt.state.editor.active.core.block(first).unwrap().text.text, "甲新丁");
        command(&mut rt, 'z', false);
        assert_eq!(rt.state.editor.active.core.block(last).unwrap().text.text, "丙丁");
        assert_eq!(rt.state.editor.active.core.sel, selection);
    }
}

fn wait_for_rendered_image(rt: &mut Runtime<State>, id: u64) {
    let key = format!("doc-image-{id}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while rt.find(&key).is_none() {
        assert!(std::time::Instant::now() < deadline, "image did not appear: {:?}", rt.state.message);
        rt.frame(0.016);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn document_requests_images_when_their_rows_mount() {
    use std::sync::{Arc, mpsc};
    let mut doc = DocDocument::empty();
    for _ in 0..200 {
        let block = doc.new_block(K::Text, "正文");
        doc.blocks.push(block);
    }
    let src = "images/near-end.png";
    let image = doc.new_block(K::Image { src: src.into(), caption: String::new(), width: Some(120.) }, "");
    let id = image.id;
    doc.blocks.push(image);
    let (fixture, mut rt) = layout_test_runtime(doc, (1200., 800.));
    rt.state.save();
    std::fs::write(fixture.0.join(src), include_bytes!("../assets/architecture.png")).unwrap();
    let (wake, wakes) = mpsc::channel();
    rt.update(|s, _| {
        s.cache = Some(rune::ImageCache::new(Arc::new(move || { wake.send(()).unwrap(); })));
        s.select_note(&s.current.clone());
    });
    for _ in 0..20 { rt.frame(0.016); }
    assert_eq!(wakes.try_recv(), Err(mpsc::TryRecvError::Empty));
    rt.wheel(900., 600., -100000.);
    wait_for_rendered_image(&mut rt, id);
    wakes.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    let [_, _, w, h] = rect(&rt, &format!("doc-image-{id}"));
    assert!((w - 120.).abs() < 1. && (h - 120. * 242. / 341.).abs() < 1.);

    rt.wheel(900., 600., 100000.);
    for _ in 0..30 { rt.frame(0.02); }
    assert!(rt.find(&format!("doc-image-{id}")).is_none());
    // A different aspect ratio on returning proves the old decoded handle was released.
    std::fs::write(fixture.0.join(src), include_bytes!("../docs/images/echo-icon.png")).unwrap();
    rt.wheel(900., 600., -100000.);
    wait_for_rendered_image(&mut rt, id);
    let [_, _, w, h] = rect(&rt, &format!("doc-image-{id}"));
    assert!((w - h).abs() < 1.);
}

#[test]
fn previews_share_a_resolution_and_preserve_intrinsic_image_dimensions() {
    let fixture = Fixture::new();
    let path = fixture.0.join("images/preview.png");
    std::fs::write(&path, include_bytes!("../assets/architecture.png")).unwrap();
    let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
    let image = cache.file_preview(&path, 128);
    assert_eq!(image.wait(), Ok([128, 91]));
    assert_eq!(image.source_size(), Some([341, 242]));
    assert_eq!(image.id(), cache.file_preview(&path, 128).id());
    let larger = cache.file_preview(&path, 1024);
    assert_eq!(larger.wait(), Ok([341, 242]));
    assert_ne!(image.id(), larger.id());
    assert_eq!(std::fs::read(path).unwrap(), include_bytes!("../assets/architecture.png"));
}

#[test]
fn document_image_corners_keep_the_ratio_and_commit_one_undo_step() {
    let (fixture, mut rt, _) = host(false);
    let id = rt.state.editor.active.core.doc.blocks[0].id;
    let src = "images/architecture.png";
    let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
    std::fs::write(fixture.0.join(src), include_bytes!("../assets/architecture.png")).unwrap();
    let [iw, ih] = cache.file(fixture.0.join(src), 400., 2.).wait().unwrap();
    let ratio = ih as f32 / iw as f32;
    rt.update(|s, _| {
        s.cache = Some(cache);
        s.editor.active.core.block_mut(id).unwrap().kind = K::Image {
            src: src.into(), caption: "图注".into(), width: Some(320.),
        };
    });
    rt.frame(0.02);
    wait_for_rendered_image(&mut rt, id);
    let key = format!("doc-image-{id}");
    for (corner, sx, sy) in [("nw", -1., -1.), ("ne", 1., -1.), ("sw", -1., 1.), ("se", 1., 1.)] {
        tap(&mut rt, &key);
        let (x, y) = rt.find(&format!("{key}-{corner}")).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_move(x + 20. * sx, y + 20. * sy * ratio);
        rt.frame(0.02);
        rt.pointer_move(x + 50. * sx, y + 50. * sy * ratio);
        rt.frame(0.02);
        let [_, _, w, h] = rect(&rt, &key);
        assert!((w - 370.).abs() < 1. && (h - w * ratio).abs() < 1.);
        rt.pointer_up(x + 50. * sx, y + 50. * sy * ratio);
        rt.frame(0.02);
        command(&mut rt, 'z', false);
        assert!((rect(&rt, &key)[2] - 320.).abs() < 1.);
    }
    tap(&mut rt, &key);
    let (x, y) = rt.find(&format!("{key}-se")).unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x + 70., y + 70. * ratio);
    rt.frame(0.02);
    press(&mut rt, Key::Escape);
    rt.pointer_up(x + 70., y + 70. * ratio);
    rt.frame(0.02);
    assert!((rect(&rt, &key)[2] - 320.).abs() < 1.);
}

#[test]
fn rapid_image_resize_across_toolbar_layouts_survives_reopen() {
    for canvas in [false, true] {
        let (fixture, mut rt, card) = host(canvas);
        let id = rt.state.editor.active.core.doc.blocks[0].id;
        let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
        std::fs::write(fixture.0.join("images/resize.png"), include_bytes!("../docs/images/echo-icon.png")).unwrap();
        rt.update(|s, _| {
            let src = "images/resize.png";
            s.cache = Some(cache);
            s.editor.active.core.block_mut(id).unwrap().kind = K::Image {
                src: src.into(), caption: String::new(), width: Some(176.),
            };
            s.editor.active.core.dirty = true;
            crate::doc_editor::sync_to_state(s);
        });
        rt.frame(0.02);
        wait_for_rendered_image(&mut rt, id);
        let key = format!("doc-image-{id}");
        tap(&mut rt, &key);
        for (start, middle, end) in [(176., 80., 96.), (96., 140., 160.)] {
            let (x, y) = rt.find(&format!("{key}-se")).unwrap();
            rt.pointer_down(x, y);
            rt.pointer_move(x + middle - start, y + middle - start);
            rt.frame(0.016);
            assert!((rect(&rt, &key)[2] - middle).abs() < 1.);
            rt.pointer_move(x + end - start, y + end - start);
            rt.pointer_up(x + end - start, y + end - start);
            rt.frame(0.016);
            assert!((rect(&rt, &key)[2] - end).abs() < 1.);
            rt.state.save();
            let reopened = State::from_root(fixture.0.clone());
            let doc = persisted_doc(&reopened, card.as_deref());
            assert!(matches!(doc.blocks[0].kind, K::Image { width: Some(w), .. } if (w - end).abs() < 1.));
        }
    }
}

#[test]
fn repeating_ime_events_only_rebuild_when_composition_changes() {
    let fixture = Fixture::new();
    let root = fixture.0.clone();
    let size = (1280., 943.);
    let spec = rune::AppSpec::new("IME", size, State::new, crate::ui::view);
    let mut rt = rune::App::new(spec)
        .init(move || State::from_root(root))
        .on_ime(crate::doc_editor::on_ime)
        .headless_runtime(size, rune::Env::new(false, 1.));
    rt.update(|s, _| {
        s.create_doc_note("工作", None);
        let id = s.editor.active.core.doc.blocks[0].id;
        s.editor.active.core.set_head(DocPos::new(id, 0), false);
    });
    for _ in 0..5 { rt.frame(0.01); }
    let compose = |rt: &mut Runtime<State>, cursor| {
        rt.ime(rune::Ime::Preedit("ni".into(), Some((cursor, cursor))));
        rt.frame(0.01);
    };
    compose(&mut rt, 2);
    let commits = rt.stats.commits;
    compose(&mut rt, 2);
    assert_eq!(rt.stats.commits, commits);
    compose(&mut rt, 1);
    assert!(rt.stats.commits > commits);
    assert_eq!(rt.state.editor.active.core.preedit.as_ref().unwrap().cursor, Some((1, 1)));
}

#[test]
fn inserting_unicode_at_format_boundaries_preserves_the_surrounding_runs() {
    use crate::doc_core::TextMarks;
    let bold = TextMarks { bold: true, ..Default::default() };
    let italic = TextMarks { italic: true, ..Default::default() };
    let mut original = RichText::plain("你a");
    original.push("🙂好", bold.clone());
    original.push("bc", TextMarks::default());
    for offset in original.text.char_indices().map(|(i, _)| i).chain([original.len()]) {
        for marks in [TextMarks::default(), bold.clone(), italic.clone()] {
            let mut expected = original.clone();
            let mut inserted = RichText::default();
            inserted.push("甲🌱", marks.clone());
            expected.replace(offset..offset, &inserted);
            let mut actual = original.clone();
            actual.insert_plain(offset, "甲🌱", marks);
            assert_eq!(actual, expected);
        }
    }
}
