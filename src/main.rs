#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
mod canvas;
mod canvas_document;
mod containers;
#[path = "domain/document.rs"]
mod doc_core;
#[path = "editor/view.rs"]
mod doc_editor;
mod doc_mode;
#[path = "editor/tables.rs"]
mod doc_tables;
#[path = "domain/color.rs"]
mod document_color;
#[path = "editor/native.rs"]
mod editor_native;
#[path = "editor/palette.rs"]
mod editor_palette;
#[path = "editor/sessions.rs"]
mod editor_sessions;
mod export;
mod import;
#[path = "persistence/format.rs"]
mod file_format;
mod i18n;
mod icons;
#[path = "domain/identity.rs"]
mod identity;
#[path = "editor/format.rs"]
mod inline_format;
#[path = "domain/input_rules.rs"]
mod input_rules;
mod insert_menu;
#[path = "domain/library.rs"]
mod library_data;
mod library_ui;
mod look;
mod material;
mod model;
mod navigation;
mod notebook_icons;
mod page_icons;
#[path = "editor/rich_render.rs"]
mod rich_render;
mod overlays;
#[path = "library/page_tree.rs"]
mod page_tree;
mod panes;
mod platform;
#[path = "persistence/writer.rs"]
mod save_writer;
mod settings;
#[path = "persistence/store.rs"]
mod storage;
#[path = "domain/table.rs"]
mod table_data;
#[path = "persistence/table_markdown.rs"]
mod table_markdown;
mod ui;
mod update;
mod welcome;
use model::State;
use rune::{App, AppSpec, ImageCache, Key, KeyEvent, Tx, Waker};
fn setup(s: &mut State, waker: Waker) {
    s.window_waker = Some(waker.clone());
    let save_waker = waker.clone();
    s.store.set_waker(move || save_waker());
    s.formulas.set_waker(waker.clone());
    let cache = ImageCache::new(waker);
    s.page_icons.load(&cache);
    s.cache = Some(cache);
    update::show_release_note(s);
}
pub(crate) fn key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if s.updater.required_ready() {
        if e.key == Key::Enter { update::act(s, tx); }
        return true;
    }
    if e.key == Key::Escape && s.nav_drag.is_some() {
        s.nav_drag = None;
        s.just_dragged_nav = true;
        tx.instant();
        return true;
    }

    if s.search_open
        || s.library_dialogs.new_page.is_some()
        || s.library_dialogs.new_book_open
        || s.library_dialogs.delete_book.is_some()
    {
        return false; // Rune's modal owns keyboard input until it closes.
    }
    if e.is_command(',') {
        if s.settings.is_some() {
            s.close_settings(tx);
        } else {
            s.open_settings(tx);
        }
        return true;
    }
    if s.settings.is_some() {
        if s.look.open && e.key == Key::Escape {
            crate::look::close(s, tx);
            return true;
        }
        if s.settings == Some(model::SettingsPage::Trash) && e.is_command('z') {
            s.undo(e.mods.shift);
            tx.instant();
            return true;
        }
        if e.key == Key::Escape {
            s.close_settings(tx);
            return true;
        }
        return e.mods.cmd;
    }
    if e.is_command('\\') {
        s.toggle_sidebar();
        tx.animated();
        return true;
    }
    if e.is_command('n') {
        if s.book_order.is_empty() {
            crate::library_ui::open_new_notebook(s, tx);
            return true;
        }
        let book = s.book.clone();
        crate::navigation::new_page(s, &book, None, tx);
        true
    } else if e.is_command('k')
        && crate::canvas_document::enabled(s)
        && s.editor.active.focused
        && !s.editor.active.core.sel.is_collapsed()
    {
        doc_editor::open_link(s, tx);
        true
    } else if e.is_command('k') {
        crate::library_ui::open_search(s, tx);
        true
    } else if e.is_command('s') {
        s.end_rename(true);
        s.editor.active.flush_cell_edit();
        doc_editor::sync_to_state(s);
        s.save();
        true
    } else if doc_editor::key(s, e, tx) {
        true
    } else if e.key == Key::Escape {
        s.finish_edit();
        s.selected = None;
        s.message = None;
        tx.blur();
        true
    } else if tx.focused().is_some() {
        false
    } else if e.is_command('z') {
        s.undo(e.mods.shift);
        true
    } else if e.is_command('d') {
        s.duplicate();
        true
    } else if e.is_command('0') {
        if let Some(n) = s.note_mut() {
            n.zoom = 1.;
            n.pan = [0., 0.];
        }
        s.pending_scroll = Some([0., 0.]);
        s.save();
        tx.instant();
        true
    } else if matches!(e.key, Key::Backspace | Key::Delete) {
        if e.mods.cmd {
            s.trash_note();
        } else {
            s.delete_selected();
        }
        true
    } else if e.key == Key::Enter {
        if let Some(id) = s.selected.clone() {
            s.edit(&id);
        }
        true
    } else {
        false
    }
}
pub(crate) fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.clock = t;
    let update_changed = update::poll(s, tx);
    let images_ready = s.poll_images();
    let formulas_ready = s.formulas.poll(s.cache.as_ref());
    // Drawn-layout callbacks wait for springs to settle. Follow Rune's live scroll
    // observation as well, so fast scrolling/glides mount cards while moving.
    let pos = s.scroll_pos.get();
    let canvas_changed = if s.mode == model::WorkspaceMode::Canvas
        && s.pending_scroll.is_none()
        && pos.viewport[0] > 0.
        && s.canvas_scroll_observed != Some(pos)
    {
        s.canvas_scroll_observed = Some(pos);
        crate::canvas_document::refresh_visible(s, pos.viewport, pos.offset)
    } else {
        false
    };
    s.editor.active.poll_caret(
        t,
        crate::canvas_document::enabled(s) && tx.focused().is_none(),
        tx,
    );
    let mut document_saved = false;
    if let Some(result) = s.store.poll_save() {
        if let Err(error) = result {
            s.message = Some(format!("{}: {error}", s.tr("保存失败")));
            s.document_save_failed = true;
            let now = std::time::Instant::now();
            s.pending_document_save.get_or_insert((now, now));
            return true;
        }
    }
    if let Some(deadline) = s.document_save_deadline() {
        let now = std::time::Instant::now();
        if now >= deadline && !s.store.saving() {
            document_saved = s.start_document_save();
            // Report a persistence error immediately; keep the unsaved state for explicit
            // save/close or the next edit, without a busy retry loop.
            if !document_saved {
                return true;
            }
        } else if now < deadline {
            tx.wake_after(deadline - now);
        }
    }
    let arrange_done = s.arrange_anim_until.is_some_and(|until| t >= until);
    if arrange_done {
        s.arrange_anim_until = None;
    }
    let rename_blurred = s.renaming.is_some() && !s.name_input.focused;
    if rename_blurred {
        s.end_rename(true);
    }
    let discard_draft = s
        .draft
        .as_ref()
        .is_some_and(|id| s.editor.canvas_owner() != Some(id) || !s.editor.active.focused);
    if discard_draft {
        s.discard_draft();
    }
    if let Some(offset) = s.pending_scroll.take() {
        tx.scroll_to_instant(s.scroll_key(), offset[0], offset[1]);
    }
    let expired_doc_copy = s
        .editor
        .active
        .copied_block
        .is_some_and(|(_, at)| at.elapsed() >= std::time::Duration::from_millis(1600));
    if expired_doc_copy {
        s.editor.active.copied_block = None;
    }
    update_changed
        || formulas_ready
        || canvas_changed
        || images_ready
        || document_saved
        || discard_draft
        || rename_blurred
        || expired_doc_copy
        || arrange_done
}
#[cfg(target_os = "macos")]
mod appearance;
fn main() {
    let os_args: Vec<_> = std::env::args_os().collect();
    if os_args.get(1).is_some_and(|arg| arg == "--apply-update") {
        if let Err(error) = update::helper(&os_args) {
            eprintln!("Update installation failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--import-library") {
        if args.len() != 4 {
            eprintln!("Usage: echo --import-library transfer.json target-folder");
            std::process::exit(2);
        }
        if let Err(e) = storage::import_library(
            std::path::Path::new(&args[2]),
            std::path::Path::new(&args[3]),
        ) {
            eprintln!("Migration failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    rune::set_app_icon(&include_bytes!("../packaging/Echo-runtime.png")[..])
        .expect("bundled system-rendered Echo icon");
    let spec = AppSpec {
        min_size: Some((1000., 736.)),
        families: (platform::UI_FONT, platform::CODE_FONT),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(key),
        ..AppSpec::new("Echo", (1280., 943.), State::new, ui::view)
    };
    let mut app = App::new(spec)
        .compositor(material::MaterialCompositor::default())
        // Use platform-native glyph rasterization (DirectWrite on Windows).
        .font_smoothing(platform::FONT_SMOOTHING)
        .on_ime(doc_editor::on_ime)
        .on_focus(|s, focused, tx| {
            if focused { s.updater.foreground(); }
            s.editor.active.window_focused = focused;
            if focused
                && canvas_document::enabled(s)
                && s.editor.active.focused
                && tx.focused().is_none()
            {
                s.editor.active.reset_caret_blink(s.clock);
            } else {
                s.editor.active.caret_opacity.snap(0.);
            }
            tx.live_only();
        })
        .chrome(platform::chrome())
        .window_options(platform::window_options())
        .on_quit(|s, _| {
            s.finish_edit();
            s.save();
        })
        .on_close(|s, tx| {
            s.finish_edit();
            s.save();
            if !s.document_save_failed {
                tx.quit();
            }
        });
    #[cfg(target_os = "windows")]
    { app = app.titlebar(rune::Titlebar::new(ui::CHROME_HEIGHT)); }
    let appearance_handle = app.handle();
    app = app.on_ready(move |_| {
        appearance_handle.update(|s, tx| {
            s.native_ready = true;
            #[cfg(target_os = "macos")]
            appearance::apply(s.sidebar_material.mode);
            tx.instant();
        });
    });
    if let Some(path) =
        std::env::var_os("ECHO_FRAME_LOG").or_else(|| std::env::var_os("NOTEFLOW_FRAME_LOG"))
    {
        use std::{
            cell::{Cell, RefCell},
            io::{BufWriter, Write},
            rc::Rc,
        };
        if let Ok(file) = std::fs::File::create(path) {
            let out = Rc::new(RefCell::new(BufWriter::new(file)));
            let frames = out.clone();
            let poses = out.clone();
            let seq = Rc::new(Cell::new(0));
            let draw_seq = seq.clone();
            let mut origin = None;
            app = app
                .on_frame_encoded(move |frame| {
                    let base = *origin.get_or_insert(frame.at());
                    seq.set(frame.seq());
                    let row = serde_json::json!({
                        "kind": "encode",
                        "seq": frame.seq(),
                        "t_ms": frame.at().saturating_duration_since(base).as_secs_f64() * 1000.,
                        "started_ms": frame.started().map(|t| t.saturating_duration_since(base).as_secs_f64() * 1000.),
                        "refresh_hz": frame.refresh_hz(),
                        "update_ms": frame.update_ms(),
                        "encode_ms": frame.encode_ms(),
                        "outcome": format!("{:?}", frame.outcome()),
                    });
                    let mut out = frames.borrow_mut();
                    let _ = writeln!(out, "{row}");
                    if frame.seq() % 32 == 0 {
                        let _ = out.flush();
                    }
                })
                .on_frame_drawn(move |rt| {
                    let row = serde_json::json!({
                        "kind": "draw",
                        "seq": draw_seq.get(),
                        "animating": rt.animating,
                    });
                    let _ = writeln!(poses.borrow_mut(), "{row}");
                });
        }
    }
    app.run();
}
