//! Opt-in, isolated release measurements. Never opens the user's library.
use super::*;
use crate::doc_core::{DocBlockKind, DocDocument, DocPos, DocSel, NoteFormat};
use std::time::Instant;

fn report(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!("AUDIT {label} n={} median_ms={:.3} p95_ms={:.3} max_ms={:.3}",
        samples.len(), samples[samples.len()/2], samples[(samples.len()-1)*95/100], samples[samples.len()-1]);
}

fn isolated_library(count: usize) -> (Fixture, State) {
    let f = Fixture(std::env::temp_dir().join(format!("echo-perf-audit-{}", uid())));
    let mut store = crate::storage::Store::open(f.0.clone()).unwrap();
    let mut notes = crate::welcome::notes().unwrap();
    notes[0].doc.as_mut().unwrap().blocks.retain(|b| matches!(b.kind, DocBlockKind::Text));
    notes[1].blocks.truncate(1);
    for i in 2..count {
        let mut n = notes[0].clone();
        n.id = format!("audit-{i}");
        n.title = format!("笔记 {i}");
        let doc = n.doc.as_mut().unwrap();
        doc.title = crate::doc_core::RichText::plain(&n.title);
        doc.blocks = vec![doc.new_block(DocBlockKind::Text, "用于测量实际笔记库的保存与导航开销。".repeat(40))];
        notes.push(n);
    }
    store.save_with_preferences(&notes, &notes[0].id, &[("book_order", "工作")]).unwrap();
    (f, State::from_root(store.root.clone()))
}

#[test]
fn background_save_orders_edits_flushes_on_close_and_preserves_external_conflicts() {
    let (f, mut s) = isolated_library(2);
    s.select_note("welcome-document");
    let id = s.editor.active.core.doc.blocks[0].id;
    s.editor.active.core.sel = DocSel::caret(DocPos::new(id, 0));
    s.editor.active.core.insert_text("甲");
    crate::doc_editor::sync_to_state(&mut s);
    s.start_document_save();
    assert!(s.store.saving());
    s.editor.active.core.insert_text("乙");
    crate::doc_editor::sync_to_state(&mut s);
    let expected = s.note().unwrap().clone();
    drop(s); // Must join the older save and persist the newer edit before returning.
    let mut s = State::from_root(f.0.clone());
    assert_eq!(s.store.load_note("welcome-document").unwrap().doc, expected.doc);
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(f.0.join(".echo/index.json")).unwrap()).unwrap();
    let path = index["entries"].as_array().unwrap().iter().find(|e| e["id"] == "welcome-document").unwrap()["path"].as_str().unwrap();
    let path = f.0.join(path);
    std::fs::write(&path, "external content must survive").unwrap();
    s.editor.active.core.insert_text("丙");
    crate::doc_editor::sync_to_state(&mut s);
    s.start_document_save();
    assert!(s.store.flush_background().is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external content must survive");
}

#[test]
fn canvas_mount_window_scrolls_and_keeps_the_active_editor() {
    let (_f, mut s) = isolated_library(2);
    s.select_note("welcome-canvas");
    let mut doc = DocDocument::empty();
    doc.blocks = vec![doc.new_block(DocBlockKind::Text, "可编辑的内容")];
    let body = crate::file_format::write_document_body(&doc).unwrap();
    s.note_mut().unwrap().blocks = (0..300).map(|i| {
        let mut b = Block::new(Kind::Text, 32., 104. + i as f32*120., 430., 80., "", &body);
        b.id = format!("window-{i}"); b.structured = true; b.min_h = 40.; b
    }).collect();
    s.save();
    s.rebuild_editors();
    let mut rt = runtime(s, (1280.,943.));
    for _ in 0..20 { rt.frame(1./60.); }
    for (index, card) in rt.state.note().unwrap().blocks.iter().enumerate() {
        assert_eq!([card.x, card.y], [32., 104. + index as f32 * 120.],
            "measuring a card on open must preserve all saved positions");
    }
    assert!(rt.state.editor.mounted_cards() < 30);
    rt.update(|s,tx| tx.scroll_to_instant(s.scroll_key(),0.,12000.));
    for _ in 0..20 { rt.frame(1./60.); }
    assert!(rt.state.canvas_visible.contains("window-105"), "offset {:?}, page {:?}, viewport {:?}, visible {:?}", rt.state.scroll_pos.get().offset, rect(&rt,"page"),rt.state.canvas_viewport_rect,rt.state.canvas_visible);
    assert!(rt.state.editor.card("window-105").is_some());
    assert!(rt.state.editor.mounted_cards() < 50);
    assert!(rt.state.editor.card("window-0").unwrap().text_layouts.borrow().is_empty());
    rt.update(|s,_| crate::canvas_document::activate(s,"window-105"));
    rt.frame(0.02);
    type_text(&mut rt,"保持输入");
    rt.update(|s,tx| tx.scroll_to_instant(s.scroll_key(),0.,0.));
    for _ in 0..20 { rt.frame(1./60.); }
    assert!(rt.state.canvas_visible.contains("window-105"),"focused editor stays mounted");
    rt.state.finish_edit();
    let mut reopened = State::from_root(rt.state.store.root.clone());
    crate::canvas_document::activate(&mut reopened,"window-105");
    assert!(reopened.editor.active.core.doc.to_plain_text().contains("保持输入"));
}

#[test]
fn sidebar_mounts_visible_rows_and_hover_does_not_rebuild_the_view() {
    let (_f, s) = isolated_library(500);
    let mut rt = runtime(s,(1280.,943.));
    for _ in 0..30 { rt.frame(1./60.); }
    let mounted = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("page-row-"))).count();
    let viewport_height = rect(&rt,"notes-scroll")[3];
    let pitch = crate::navigation::PAGE_ROW_PITCH;
    assert!(mounted <= (viewport_height / pitch).ceil() as usize * 3 + 2,"mounted {mounted} rows");
    let [x,y,_,_] = rect(&rt,"notes-scroll");
    rt.pointer_move(x+100.,y+20.); rt.frame(0.3);
    let commits = rt.stats.commits;
    for i in 0..12 { rt.pointer_move(x+100.,y+20.+i as f32*pitch); rt.frame(0.03); }
    assert!(rt.stats.commits - commits <= 2,"hover committed {} times",rt.stats.commits-commits);
    rt.update(|_,tx| tx.scroll_to_instant("notes-scroll",0.,414. * pitch));
    for _ in 0..20 { rt.frame(1./60.); }
    assert!(rt.find("note-audit-420").is_some());
}

#[test]
fn library_undo_shares_unchanged_notes_without_losing_history() {
    let (_f, mut s) = isolated_library(100);
    s.remember();
    s.note_mut().unwrap().preview = "changed".into();
    s.remember();
    assert!(std::sync::Arc::ptr_eq(&s.undo[0][50],&s.undo[1][50]));
    assert!(!std::sync::Arc::ptr_eq(&s.undo[0][0],&s.undo[1][0]));
    s.undo(false);
    s.undo(false);
    assert_ne!(s.note().unwrap().preview,"changed");
    s.undo(true);
    assert_eq!(s.note().unwrap().preview,"changed");
}

#[test]
#[ignore = "manual release audit; isolated generated fixtures only"]
fn profile_current_editor_performance() {
    for count in [2, 100, 500] {
        let (_f, mut s) = isolated_library(count);
        for (name, id) in [("document", "welcome-document"), ("canvas", "welcome-canvas")] {
            s.select_note(id);
            if name == "canvas" { crate::canvas_document::activate(&mut s, "welcome-card-0"); }
            let block = s.editor.active.core.doc.blocks[0].id;
            s.editor.active.core.sel = DocSel::caret(DocPos::new(block, 0));
            let mut samples = vec![];
            for _ in 0..20 {
                let start = Instant::now();
                s.editor.active.core.insert_text("字");
                crate::doc_editor::sync_to_state(&mut s);
                samples.push(start.elapsed().as_secs_f64()*1000.);
                assert!(!s.document_save_failed);
            }
            report(&format!("edit_sync mode={name} library_notes={count}"), samples);
            s.finish_edit();
        }
        let mut nav = vec![];
        for i in 0..20 {
            let start = Instant::now();
            s.select_note(if i%2 == 0 { "welcome-document" } else { "welcome-canvas" });
            nav.push(start.elapsed().as_secs_f64()*1000.);
        }
        report(&format!("select_note library_notes={count}"), nav);
    }
    for count in [20, 200, 1000] {
        for format in [NoteFormat::Document, NoteFormat::Canvas] {
            let (_f, mut s) = isolated_library(2);
            let mut doc = DocDocument::empty();
            doc.blocks.clear();
            for _ in 0..count { let b = doc.new_block(DocBlockKind::Text, "这是用于性能检查的段落，检查视口外内容是否仍然参与构建。"); doc.blocks.push(b); }
            let id = if format == NoteFormat::Document { "welcome-document" } else { "welcome-canvas" };
            s.select_note(id);
            if format == NoteFormat::Document {
                s.note_mut().unwrap().doc = Some(doc.clone());
                s.editor.active.load(id, doc, false);
            } else {
                let mut unit = DocDocument::empty();
                unit.blocks = vec![unit.new_block(DocBlockKind::Text, "这是用于性能检查的一张画布卡片。")];
                let body = crate::file_format::write_document_body(&unit).unwrap();
                s.note_mut().unwrap().blocks = (0..count).map(|i| {
                    let mut b = Block::new(Kind::Text, 32., 104.+i as f32*120., 430., 80., "", &body);
                    b.id = format!("audit-card-{i}"); b.structured = true; b.min_h = 40.; b
                }).collect();
                s.rebuild_editors();
            }
            let start = Instant::now();
            let mut rt = runtime(s, (1280.,943.));
            for _ in 0..5 { rt.frame(1./60.); }
            println!("AUDIT mount mode={format:?} blocks={count} elapsed_ms={:.3} elements={}", start.elapsed().as_secs_f64()*1000., rt.elements().len());
            let mut commits = vec![];
            for _ in 0..12 {
                let start = Instant::now();
                rt.update(|s, tx| { s.hovered_nav = None; tx.instant(); });
                rt.frame(1./60.);
                commits.push(start.elapsed().as_secs_f64()*1000.);
            }
            report(&format!("view_commit mode={format:?} blocks={count}"), commits);
        }
    }
}

#[test]
#[ignore = "manual release timing using isolated canvas notes"]
fn profile_canvas_drop_save() {
    for count in [20, 200, 1000] {
        let mut releases = Vec::new();
        for _ in 0..5 {
            let (fixture, mut state) = isolated_library(2);
            state.select_note("welcome-canvas");
            let mut doc = DocDocument::empty();
            doc.blocks = vec![doc.new_block(DocBlockKind::Text, "用来测量边缘滚动的可编辑内容")];
            let body = crate::file_format::write_document_body(&doc).unwrap();
            state.note_mut().unwrap().blocks = (0..count).map(|i| {
                let mut b = Block::new(Kind::Text, 40., 180. + i as f32 * 120., 430., 80., "", &body);
                b.id = format!("perf-card-{i}");
                b.structured = true;
                b.min_h = 60.;
                b
            }).collect();
            state.save();
            state.rebuild_editors();
            let mut rt = runtime(state, (1280.,943.));
            for _ in 0..30 { rt.frame(1./60.); }
            let (x,y) = rt.find("move-edge-top-perf-card-0").unwrap();
            let [_, top, _, h] = rect(&rt,&rt.state.scroll_key());
            rt.pointer_down(x,y);
            rt.frame(1./60.);
            rt.pointer_move(x,top+h-4.);
            for _ in 0..190 { rt.frame(1./60.); }
            assert!(rt.state.scroll_pos.get().offset[1]>900.);
            let release = Instant::now();
            rt.pointer_up(x,top+h-4.);
            rt.frame(1./60.);
            releases.push(release.elapsed().as_secs_f64()*1000.);
            let moved = rt.state.note().unwrap().blocks[0].y;
            rt.state.store.flush_background().unwrap();
            let reopened = State::from_root(fixture.0.clone());
            assert_eq!(reopened.note().unwrap().blocks[0].y, moved);
        }
        report(&format!("canvas_drop cards={count}"), releases);
    }
}


#[test]
fn canvas_cold_start_paints_content_before_the_first_click() {
    let mut notes = crate::welcome::notes().unwrap();
    for zoom in [1., 0.6, 1.6] {
        for offset in [0., 600., 1200., 2400.] {
            let f = Fixture(std::env::temp_dir().join(format!("echo-cold-paint-{}", uid())));
            let mut store = crate::storage::Store::open(f.0.clone()).unwrap();
            notes[1].zoom = zoom;
            notes[1].pan = [0., offset];
            store.save_with_preferences(&notes, "welcome-canvas", &[("book_order", "工作")]).unwrap();
            let s = State::from_root(f.0.clone());
            assert_eq!(s.current, "welcome-canvas");
            let mut rt = Runtime::new(s, crate::ui::view, (1280., 943.), Env::new(false, 2.));
            rt.poll = Some(crate::poll);
            rt.on_key = Some(crate::key);
            for _ in 0..120 { if !rt.frame(1./60.) { break; } }
            let before = rt.instances.iter().filter(|i| rune::compose::is_glyph(i)).count();
            rt.invalidate();
            for _ in 0..120 { if !rt.frame(1./60.) { break; } }
            let after = rt.instances.iter().filter(|i| rune::compose::is_glyph(i)).count();
            assert_eq!(before, after, "zoom={zoom} scroll={offset}: content appeared only after rebuilding");
        }
    }
}

#[test]
#[ignore = "release timing for interactive typing on isolated documents"]
fn profile_continuous_document_input() {
    for (count, repetitions) in [(20, 1), (1000, 1), (20, 80)] {
        let (_fixture, mut state) = isolated_library(2);
        let mut doc = DocDocument::empty();
        doc.blocks.clear();
        for i in 0..count {
            let kind = if i % 8 < 4 { DocBlockKind::Text } else { DocBlockKind::Ordered { start: None } };
            let block = doc.new_block(kind, format!("第 {i} 段，测量连续输入和中文候选词更新。"));
            doc.blocks.push(block);
        }
        doc.blocks[0].text = crate::doc_core::RichText::plain(doc.blocks[0].text.text.repeat(repetitions));
        let block = doc.blocks[0].id;
        state.note_mut().unwrap().doc = Some(doc.clone());
        let note = state.current.clone();
        state.editor.active.load(&note, doc, true);
        state.editor.active.core.set_head(DocPos::new(block, 0), false);
        let mut rt = runtime(state, (1280.,943.));
        for _ in 0..8 { rt.frame(1./60.); }
        let mut typing = Vec::new();
        let mut preedit = Vec::new();
        let mut commit = Vec::new();
        for _ in 0..40 {
            let now = Instant::now();
            type_text(&mut rt, "a");
            typing.push(now.elapsed().as_secs_f64() * 1000.);
            for pinyin in ["z", "zh", "zho", "zhon", "zhong"] {
                let event = rune::Ime::Preedit(pinyin.into(), Some((pinyin.len(), pinyin.len())));
                let now = Instant::now();
                rt.update(|s, tx| { assert!(crate::doc_editor::on_ime(s, &event, tx)); });
                rt.frame(1./60.);
                preedit.push(now.elapsed().as_secs_f64() * 1000.);
            }
            let now = Instant::now();
            rt.update(|s, tx| { assert!(crate::doc_editor::on_ime(s, &rune::Ime::Commit("中".into()), tx)); });
            rt.frame(1./60.);
            commit.push(now.elapsed().as_secs_f64() * 1000.);
        }
        assert!(rt.state.editor.active.core.text_of(block).unwrap().text.starts_with(&"a中".repeat(40)));
        report(&format!("typing blocks={count} paragraph_repetitions={repetitions}"), typing);
        report(&format!("preedit blocks={count} paragraph_repetitions={repetitions}"), preedit);
        report(&format!("commit blocks={count} paragraph_repetitions={repetitions}"), commit);
    }
}
