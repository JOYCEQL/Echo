use crate::doc_core::NoteFormat;
use crate::storage::{Store, data_root};
use rune::{Image, ImageCache, TextEdit};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const CANVAS_MAX_ZOOM: f32 = 2.5;

pub use crate::identity::{now, uid};
pub use crate::library_data::{Block, Kind, Note};
#[cfg(test)]
use crate::library_data::BOOKS;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Canvas,
    Document,
}
impl WorkspaceMode {
    pub fn from_format(format: NoteFormat) -> Self {
        match format {
            NoteFormat::Document => Self::Document,
            NoteFormat::Canvas => Self::Canvas,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::Document => "document",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Canvas => "自由画布",
            Self::Document => "结构化文档",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}
impl Theme {
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
    pub fn choose(self, light: u32, dark: u32) -> u32 {
        if self.is_dark() { dark } else { light }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    Appearance,
    Shortcuts,
    Data,
    Trash,
    Updates,
}
#[derive(Clone, PartialEq, Debug)]
pub enum RenameTarget {
    Book(usize),
    Note(String),
}
impl RenameTarget {
    pub fn row_key(&self) -> String {
        match self {
            Self::Book(i) => format!("book-{i}"),
            Self::Note(id) => format!("note-{id}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum NavDrag {
    Book {
        source_idx: usize,
        target_idx: usize,
        pointer: [f32; 2],
    },
    Note {
        note_id: String,
        book: String,
        parent_id: Option<String>,
        source_idx: usize,
        target_idx: usize,
        drop_into_book: Option<String>,
        valid_drop: bool,
        pointer: [f32; 2],
    },
}

enum SettingsReturnFocus {
    Native(String),
    Editor {
        card: Option<String>,
        cell: Option<(u64, usize, usize)>,
    },
}

pub struct State {
    pub updater: crate::update::Updater,
    pub editor: crate::editor_sessions::Sessions,
    pub language: crate::i18n::Language,
    pub notebook_icon_style: crate::notebook_icons::Style,
    pub pending_document_save: Option<(std::time::Instant, std::time::Instant)>,
    pub document_save_failed: bool,
    pub nav_focus: Option<String>,
    pub pane_widths: crate::panes::Widths,
    pub pane_resize: Option<crate::panes::Resize>,
    pub pane_hover: Option<crate::panes::Boundary>,
    pub renaming: Option<RenameTarget>,
    pub name_input: TextEdit,
    pub(crate) book_names: HashMap<String, String>,
    pub store: Store,
    pub notes: Vec<Note>,
    pub current: String,
    pub book: String,
    pub selected: Option<String>,
    // A blank click is an input position, persisted only after meaningful input.
    pub draft: Option<String>,
    pub hovered: Option<String>,
    pub scroll_pos: std::rc::Rc<std::cell::Cell<rune::ScrollPos>>,
    pub pending_scroll: Option<[f32; 2]>,

    pub query: TextEdit,
    pub search_open: bool,
    pub library_dialogs: crate::library_ui::Dialogs,
    pub cache: Option<ImageCache>,
    pub formulas: crate::rich_render::Formulas,
    pub page_icons: crate::page_icons::PageIcons,
    pending_images: std::cell::RefCell<HashMap<[u8; 32], Image>>,
    pub display_scale: std::cell::Cell<f32>,
    pub drag_start: [f32; 4],
    canvas_gesture_history: Option<Vec<std::sync::Arc<Note>>>,
    pub drag_bounds: [f32; 4],
    pub undo: Vec<Vec<std::sync::Arc<Note>>>,
    pub redo: Vec<Vec<std::sync::Arc<Note>>>,
    pub ui_scale: std::cell::Cell<f32>,
    pub window_chrome: std::cell::Cell<rune::ChromeInsets>,
    pub settings: Option<SettingsPage>,
    settings_return_focus: Option<SettingsReturnFocus>,
    pub list_offset: std::rc::Rc<std::cell::Cell<[f32; 2]>>,
    pub native_ready: bool,
    pub sidebar_material: crate::material::Settings,
    pub look: crate::look::Editor,
    pub system_dark: std::cell::Cell<bool>,
    pub message: Option<String>,
    pub clock: f64,
    pub arrange_anim_until: Option<f64>,
    pub mode: WorkspaceMode,
    pub tree_expanded: std::collections::HashSet<String>,
    doc_outlines: std::collections::BTreeSet<String>,
    pub hovered_nav: Option<String>,
    pub sidebar_collapsed: bool,
    pub book_order: Vec<String>,
    #[allow(dead_code)]
    #[allow(dead_code)]
    pub nav_drag: Option<NavDrag>,
    pub nav_row_rects: HashMap<String, [f32; 4]>,
    pub just_dragged_nav: bool,
    pub canvas_viewport_rect: Option<[f32; 4]>,
    pub canvas_visible: std::collections::HashSet<String>,
    pub canvas_scroll_observed: Option<rune::ScrollPos>,
    pub window_waker: Option<rune::Waker>,
}
impl Drop for State {
    fn drop(&mut self) {
        if self.pending_document_save.is_some() || self.store.saving() {
            self.save();
        }
    }
}

impl State {
    pub fn theme(&self) -> Theme {
        if self.sidebar_material.mode == crate::material::LookMode::Dark
            || (self.sidebar_material.mode == crate::material::LookMode::Auto
                && self.system_dark.get())
        {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
    pub fn set_look_mode(&mut self, mode: crate::material::LookMode) {
        self.sidebar_material.mode = mode;
        #[cfg(target_os = "macos")]
        if self.native_ready {
            crate::appearance::apply(mode);
        }
        self.save_sidebar_material();
    }

    pub fn new() -> Self {
        Self::from_root(data_root())
    }
    pub fn from_root(root: PathBuf) -> Self {
        let mut store = Store::open(root).unwrap_or_else(|e| {
            eprintln!("Cannot open Echo data: {e}");
            std::process::exit(1)
        });
        let mut notes = store.load().unwrap_or_else(|e| {
            eprintln!("Cannot read Echo data: {e}");
            std::process::exit(1)
        });
        if notes.is_empty() && store.preference("book_order").is_none() {
            notes = crate::welcome::initialize(&mut store).expect("initialize welcome examples");
        }
        let current = store
            .selected()
            .filter(|id| notes.iter().any(|n| &n.id == id))
            .unwrap_or_else(|| notes.first().map(|n| n.id.clone()).unwrap_or_default());
        let n = notes.iter().find(|n| n.id == current);
        let mode = n.map_or(WorkspaceMode::Document, |n| {
            WorkspaceMode::from_format(n.format)
        });
        let book = n.map(|n| n.notebook.clone()).unwrap_or_default();
        let selected = None;
        let pan = if mode == WorkspaceMode::Document {
            [0., 0.]
        } else {
            n.map(|n| n.pan).unwrap_or([0., 0.])
        };
        let sidebar_material = match store.preference("sidebar_material_light") {
            None => crate::material::Settings::default(),
            Some(raw) => {
                let value: crate::material::Settings = serde_json::from_str(&raw)
                    .expect("sidebar_material must contain valid saved appearance settings");
                assert!(
                    value.transparency <= 100
                        && value.grain <= 100
                        && (1..=3).contains(&value.colors.len())
                        && value
                            .colors
                            .iter()
                            .all(|c| c.hue <= 360 && c.saturation <= 100 && c.lightness <= 100),
                    "saved sidebar percentages must be in 0..=100"
                );
                value
            }
        };
        let sidebar_collapsed = matches!(
            store.preference("sidebar_collapsed").as_deref(),
            Some("1" | "true")
        );
        let mut book_order: Vec<String> = match store.preference("book_order") {
            Some(raw) => raw
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty() && *key != "回收站")
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        };
        let mut seen = std::collections::HashSet::new();
        book_order.retain(|key| seen.insert(key.clone()));
        let mut tree_expanded = std::collections::HashSet::new();
        tree_expanded.insert("book:工作".into());
        tree_expanded.insert(format!("book:{book}"));
        for note in &notes {
            if let Some(pid) = &note.parent_id {
                tree_expanded.insert(pid.clone());
            }
        }
        let book_names = book_order
            .iter()
            .map(String::as_str)
            .chain(notes.iter().map(|n| n.original_book.as_str()))
            .chain(std::iter::once("回收站"))
            .filter_map(|key| {
                store
                    .preference(&format!("book-name/{key}"))
                    .map(|value| (key.to_string(), value))
            })
            .collect();
        let pane_widths = match store.preference("pane_widths_light") {
            Some(raw) => serde_json::from_str(&raw).expect("stored pane widths must be valid JSON"),
            None => crate::panes::Widths::default(),
        };
        let language =
            crate::i18n::Language::from_preference(store.preference("ui_language").as_deref());
        let empty_book = if store.selected().as_deref() == Some("") {
            store.preference("selected_book")
        } else {
            None
        };
        let doc_outlines = match store.preference("document_outlines") {
            Some(raw) => {
                serde_json::from_str(&raw).expect("saved document outline IDs must be valid JSON")
            }
            None => std::collections::BTreeSet::new(),
        };
        let notebook_icon_style = crate::notebook_icons::Style::from_preference(
            store.preference("notebook_icon_style").as_deref(),
        );
        let mut state = Self {
            editor: Default::default(),
            language,
            notebook_icon_style,
            pane_widths,
            pane_resize: None,
            pane_hover: None,
            nav_focus: None,
            renaming: None,
            name_input: TextEdit::new(""),
            book_names,
            updater: crate::update::Updater::new(store.preference("automatic_updates").as_deref().unwrap_or("true") == "true"),
            store,
            notes,
            current,
            book,
            selected,
            draft: None,
            hovered: None,
            scroll_pos: std::rc::Rc::new(std::cell::Cell::new(rune::ScrollPos::default())),
            pending_scroll: Some(pan),

            query: TextEdit::new(""),
            search_open: false,
            library_dialogs: crate::library_ui::Dialogs::default(),
            cache: None,
            formulas: Default::default(),
            page_icons: Default::default(),
            pending_images: Default::default(),
            display_scale: std::cell::Cell::new(1.),
            drag_start: [0.; 4],
            canvas_gesture_history: None,
            drag_bounds: [0.; 4],
            undo: vec![],
            redo: vec![],
            ui_scale: std::cell::Cell::new(1.),
            window_chrome: std::cell::Cell::new(rune::ChromeInsets::default()),
            settings: None,
            settings_return_focus: None,
            list_offset: Default::default(),
            native_ready: false,
            sidebar_material,
            look: crate::look::Editor::default(),
            system_dark: std::cell::Cell::new(false),
            message: None,
            clock: 0.,
            pending_document_save: None,
            document_save_failed: false,
            arrange_anim_until: None,
            mode,
            tree_expanded,
            doc_outlines,
            hovered_nav: None,
            sidebar_collapsed,
            book_order,
            nav_drag: None,
            nav_row_rects: HashMap::new(),
            just_dragged_nav: false,
            canvas_viewport_rect: None,
            canvas_visible: Default::default(),
            canvas_scroll_observed: None,
            window_waker: None,
        };
        state.ensure_selected_note();
        state.rebuild_editors();
        state.load_active_doc_session(false);
        if let Some(book) = empty_book {
            if state.book_order.contains(&book) || (book.is_empty() && state.book_order.is_empty())
            {
                state.select_book(&book);
            }
        }
        if state.book_order.is_empty() {
            state.book.clear();
            state.current.clear();
            state.rebuild_editors();
            state.load_active_doc_session(false);
        }
        state
    }
    pub fn active_format(&self) -> crate::doc_core::NoteFormat {
        match self.mode {
            WorkspaceMode::Document => crate::doc_core::NoteFormat::Document,
            WorkspaceMode::Canvas => crate::doc_core::NoteFormat::Canvas,
        }
    }
    #[cfg(test)]
    pub fn seed_doc_notes_if_needed(&mut self) {
        use crate::doc_core::NoteFormat;
        if self.store.preference("document_seeded").as_deref() == Some("1")
            || self.notes.iter().any(|n| n.format == NoteFormat::Document)
        {
            return;
        }
        let seeded = crate::doc_core::seed_documents();
        let mut created_ids: Vec<String> = Vec::with_capacity(seeded.len());
        let base_ts = now();
        for (idx, (book, parent_idx, doc)) in seeded.into_iter().enumerate() {
            if !self.book_order.contains(&book) {
                created_ids.push(String::new());
                continue;
            }
            let id = uid();
            let parent_id =
                parent_idx.and_then(|pi| created_ids.get(pi).filter(|id| !id.is_empty()).cloned());
            let title_text = doc.title.text.trim();
            let title = if title_text.is_empty() {
                self.tr("无标题文档").to_owned()
            } else {
                title_text.to_owned()
            };
            let preview_text = doc.preview_text();
            let preview = if preview_text.is_empty() {
                self.tr("开始输入正文…").to_owned()
            } else {
                preview_text
            };
            self.tree_expanded.insert(format!("book:{book}"));
            if let Some(pid) = &parent_id {
                self.tree_expanded.insert(pid.clone());
            }
            self.notes.push(Note {
                id: id.clone(),
                notebook: book.clone(),
                original_book: book,
                parent_id,
                title,
                preview,
                stamp: "刚刚".into(),
                updated: base_ts - idx as i64,
                blocks: Vec::new(),
                zoom: 1.,
                pan: [0., 0.],
                editor_version: 7,
                format: NoteFormat::Document,
                doc: Some(doc),
            });
            created_ids.push(id);
        }
        if let Err(error) = self.store.set_preference("document_seeded", "1") {
            self.message = Some(format!("{}: {error}", self.tr("保存失败")));
        }
    }
    pub fn ensure_selected_note(&mut self) {
        if self.book_order.is_empty() {
            self.book.clear();
            self.current.clear();
            self.mode = WorkspaceMode::Document;
            return;
        }
        if !self
            .notes
            .iter()
            .any(|n| n.id == self.current && n.notebook == self.book)
        {
            self.current = self
                .notes
                .iter()
                .find(|n| n.notebook == self.book)
                .map(|n| n.id.clone())
                .unwrap_or_default();
        }
        if let Some(note) = self.notes.iter().find(|n| n.id == self.current) {
            self.mode = WorkspaceMode::from_format(note.format);
            self.tree_expanded.insert(format!("book:{}", note.notebook));
            if let Some(pid) = &note.parent_id {
                self.tree_expanded.insert(pid.clone());
            }
        } else {
            self.mode = WorkspaceMode::Document;
        }
    }
    pub fn load_active_doc_session(&mut self, focus: bool) {
        if self.mode != WorkspaceMode::Document {
            self.editor.active.focused = false;
            return;
        }
        let fmt = crate::doc_core::NoteFormat::Document;
        if let Some(n) = self
            .notes
            .iter()
            .find(|n| n.id == self.current && n.format == fmt)
        {
            let doc = n.doc.clone().unwrap_or_default();
            let id = n.id.clone();
            self.editor.load_document(&id, doc, focus);
        } else {
            self.editor.clear();
        }
    }
    pub fn save_doc_image_bytes(&mut self, bytes: &[u8], ext: &str) -> Result<String, String> {
        let rel = format!("images/{}.{}", uid(), ext.trim_start_matches('.'));
        if let Some(parent) = self.store.root.join(&rel).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(self.store.root.join(&rel), bytes).map_err(|e| e.to_string())?;
        Ok(rel)
    }
    pub fn book_at(&self, slot: usize) -> &str {
        &self.book_order[slot]
    }
    pub fn delete_notebook(&mut self, book: &str) -> Result<(), String> {
        if !self.book_order.iter().any(|key| key == book) {
            return Err(self.tr("空间不存在").into());
        }
        self.finish_edit();
        if self.document_save_failed {
            return Err(self.tr("请先保存当前内容，再删除空间").into());
        }
        let mut notes = self.notes.clone();
        for note in notes.iter_mut().filter(|note| note.notebook == book) {
            note.original_book = book.into();
            note.notebook = "回收站".into();
            note.updated = now();
            note.stamp = "刚刚".into();
        }
        let order: Vec<_> = self
            .book_order
            .iter()
            .filter(|key| *key != book)
            .cloned()
            .collect();
        let next_book = if self.book == book {
            order.first().cloned().unwrap_or_default()
        } else {
            self.book.clone()
        };
        let current = if self.book == book {
            notes
                .iter()
                .find(|note| note.notebook == next_book)
                .map(|note| note.id.clone())
                .unwrap_or_default()
        } else {
            self.current.clone()
        };
        // Move Markdown, update the index and hide the notebook in one durable transaction.
        self.store.save_with_preferences(
            &notes,
            &current,
            &[
                ("book_order", &order.join(",")),
                ("selected_book", &next_book),
                ("document_seeded", "1"),
            ],
        )?;
        self.notes = notes;
        self.book_order = order;
        self.book = next_book;
        self.current = current;
        self.undo.clear();
        self.redo.clear();
        self.nav_drag = None;
        self.nav_focus = None;
        self.selected = None;
        self.hovered = None;
        self.tree_expanded.remove(&format!("book:{book}"));
        self.ensure_selected_note();
        self.pending_scroll = Some(self.note().map(|n| n.pan).unwrap_or([0., 0.]));
        self.rebuild_editors();
        self.load_active_doc_session(false);
        Ok(())
    }
    pub fn move_book(&mut self, from_slot: usize, to_slot: usize) -> bool {
        if from_slot >= self.book_order.len() {
            return false;
        }
        let insert_at = if to_slot > from_slot {
            (to_slot - 1).min(self.book_order.len() - 1)
        } else {
            to_slot.min(self.book_order.len() - 1)
        };
        if insert_at == from_slot {
            return false;
        }
        let item = self.book_order.remove(from_slot);
        self.book_order.insert(insert_at, item);
        let joined = self.book_order.join(",");
        if let Err(e) = self.store.set_preference("book_order", &joined) {
            self.message = Some(format!("{}: {e}", self.tr("空间排序保存失败")));
        }
        true
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    #[allow(dead_code)]
    #[allow(dead_code)]

    fn is_descendant_note(&self, candidate_id: &str, ancestor_id: &str) -> bool {
        let mut cursor = Some(candidate_id);
        let mut guard = 0;
        while let Some(cid) = cursor {
            if cid == ancestor_id {
                return true;
            }
            if guard > 24 {
                break;
            }
            guard += 1;
            cursor = self
                .notes
                .iter()
                .find(|n| n.id == cid)
                .and_then(|n| n.parent_id.as_deref());
        }
        false
    }
    pub fn move_note_to_book(&mut self, note_id: &str, target_book: &str) -> bool {
        let Some(idx) = self.notes.iter().position(|n| n.id == note_id) else {
            return false;
        };
        if self.notes[idx].notebook == target_book && self.notes[idx].parent_id.is_none() {
            return false;
        }
        self.finish_edit();
        if self.document_save_failed {
            return false;
        }
        self.remember();
        let mut tree_ids = vec![note_id.to_owned()];
        let mut changed = true;
        while changed {
            changed = false;
            for n in &self.notes {
                if let Some(pid) = &n.parent_id {
                    if tree_ids.contains(pid) && !tree_ids.contains(&n.id) {
                        tree_ids.push(n.id.clone());
                        changed = true;
                    }
                }
            }
        }
        for n in &mut self.notes {
            if n.id == note_id {
                n.parent_id = None;
                n.notebook = target_book.to_owned();
                n.original_book = target_book.to_owned();
            } else if tree_ids.contains(&n.id) {
                n.notebook = target_book.to_owned();
                n.original_book = target_book.to_owned();
            }
        }
        self.tree_expanded.insert(format!("book:{target_book}"));
        if self.note().is_some_and(|n| n.notebook == target_book) {
            self.book = target_book.to_owned();
        }
        self.save();
        true
    }
    pub fn reorder_note(
        &mut self,
        note_id: &str,
        target_book: &str,
        target_parent: Option<String>,
        target_sibling_slot: usize,
    ) -> bool {
        if !self.notes.iter().any(|n| n.id == note_id) {
            return false;
        }
        if target_parent
            .as_deref()
            .is_some_and(|pid| self.is_descendant_note(pid, note_id))
        {
            return false;
        }
        let siblings: Vec<String> = self
            .notes
            .iter()
            .filter(|n| n.notebook == target_book && n.parent_id == target_parent)
            .map(|n| n.id.clone())
            .collect();
        let current_slot = siblings.iter().position(|id| id == note_id);
        let insert_at = match current_slot {
            Some(src) if target_sibling_slot > src => {
                (target_sibling_slot - 1).min(siblings.len().saturating_sub(1))
            }
            Some(_) => target_sibling_slot.min(siblings.len().saturating_sub(1)),
            None => target_sibling_slot.min(siblings.len()),
        };
        if current_slot == Some(insert_at) {
            return false;
        }
        self.finish_edit();
        if self.document_save_failed {
            return false;
        }
        self.remember();
        let Some(note_pos) = self.notes.iter().position(|n| n.id == note_id) else {
            return false;
        };
        let mut moved = self.notes.remove(note_pos);
        let book_changed = moved.notebook != target_book;
        moved.notebook = target_book.to_owned();
        moved.original_book = target_book.to_owned();
        moved.parent_id = target_parent.clone();
        if book_changed {
            let mut tree_ids = vec![note_id.to_owned()];
            let mut grew = true;
            while grew {
                grew = false;
                for n in &self.notes {
                    if let Some(pid) = &n.parent_id {
                        if tree_ids.contains(pid) && !tree_ids.contains(&n.id) {
                            tree_ids.push(n.id.clone());
                            grew = true;
                        }
                    }
                }
            }
            for n in &mut self.notes {
                if tree_ids.contains(&n.id) {
                    n.notebook = target_book.to_owned();
                    n.original_book = target_book.to_owned();
                }
            }
        }
        let remaining_siblings: Vec<String> = self
            .notes
            .iter()
            .filter(|n| n.notebook == target_book && n.parent_id == target_parent)
            .map(|n| n.id.clone())
            .collect();
        if insert_at < remaining_siblings.len() {
            let anchor_id = &remaining_siblings[insert_at];
            let anchor_pos = self
                .notes
                .iter()
                .position(|n| &n.id == anchor_id)
                .unwrap_or(self.notes.len());
            self.notes.insert(anchor_pos, moved);
        } else if let Some(last_id) = remaining_siblings.last() {
            let last_pos = self
                .notes
                .iter()
                .position(|n| &n.id == last_id)
                .map_or(self.notes.len(), |p| p + 1);
            self.notes.insert(last_pos, moved);
        } else {
            self.notes.insert(0, moved);
        }
        self.tree_expanded.insert(format!("book:{target_book}"));
        if let Some(pid) = &target_parent {
            self.tree_expanded.insert(pid.clone());
        }
        if self.note().is_some_and(|n| n.notebook == target_book) {
            self.book = target_book.to_owned();
        }
        self.save();
        true
    }
    pub fn reorder_note_in_book(&mut self, note_id: &str, book: &str, target_slot: usize) -> bool {
        let book_ids: Vec<String> = self
            .notes
            .iter()
            .filter(|n| n.notebook == book)
            .map(|n| n.id.clone())
            .collect();
        let Some(src_idx) = book_ids.iter().position(|id| id == note_id) else {
            return false;
        };
        let insert_at = if target_slot > src_idx {
            (target_slot - 1).min(book_ids.len().saturating_sub(1))
        } else {
            target_slot.min(book_ids.len().saturating_sub(1))
        };
        if insert_at == src_idx {
            return false;
        }
        self.finish_edit();
        self.remember();
        let Some(note_pos) = self.notes.iter().position(|n| n.id == note_id) else {
            return false;
        };
        let moved = self.notes.remove(note_pos);
        let remaining: Vec<String> = self
            .notes
            .iter()
            .filter(|n| n.notebook == book)
            .map(|n| n.id.clone())
            .collect();
        if insert_at < remaining.len() {
            let anchor_id = &remaining[insert_at];
            let anchor_pos = self
                .notes
                .iter()
                .position(|n| &n.id == anchor_id)
                .unwrap_or(self.notes.len());
            self.notes.insert(anchor_pos, moved);
        } else if let Some(last_id) = remaining.last() {
            let last_pos = self
                .notes
                .iter()
                .position(|n| &n.id == last_id)
                .map_or(self.notes.len(), |p| p + 1);
            self.notes.insert(last_pos, moved);
        } else {
            self.notes.insert(0, moved);
        }
        self.save();
        true
    }
    pub fn empty_trash(&mut self) -> usize {
        self.clear_trash(Some(self.active_format()))
    }
    pub fn empty_all_trash(&mut self) -> usize {
        self.clear_trash(None)
    }
    fn clear_trash(&mut self, format: Option<NoteFormat>) -> usize {
        let trashed_ids: Vec<String> = self
            .notes
            .iter()
            .filter(|n| n.notebook == "回收站" && format.is_none_or(|f| n.format == f))
            .map(|n| n.id.clone())
            .collect();
        if trashed_ids.is_empty() {
            return 0;
        }
        let count = trashed_ids.len();
        self.finish_edit();
        self.remember();
        self.notes
            .retain(|n| !(n.notebook == "回收站" && format.is_none_or(|f| n.format == f)));
        self.selected = None;
        self.hovered = None;
        self.ensure_selected_note();
        self.rebuild_editors();
        self.load_active_doc_session(false);
        self.save();
        self.message = Some(if self.language.is_english() {
            format!("Trash emptied ({count} notes). Press ⌘Z to undo.")
        } else {
            format!("已清空回收站（共清理 {count} 篇内容，可按 ⌘Z 撤销）")
        });
        count
    }
    pub fn delete_note_permanently(&mut self, id: &str) {
        if !self.notes.iter().any(|n| n.id == id) {
            return;
        }
        self.finish_edit();
        self.remember();
        self.notes.retain(|n| n.id != id);

        self.selected = None;
        self.hovered = None;
        self.ensure_selected_note();
        self.rebuild_editors();
        self.load_active_doc_session(false);
        self.save();
    }
    pub fn restore_note_by_id(&mut self, id: &str) {
        let Some(idx) = self
            .notes
            .iter()
            .position(|n| n.id == id && n.notebook == "回收站")
        else {
            return;
        };
        self.finish_edit();
        if self.document_save_failed {
            return;
        }
        let restored_book = self.notes[idx].original_book.clone();
        if restored_book.is_empty() || restored_book == "回收站" {
            self.message = Some(self.tr("无法恢复：笔记缺少原空间信息").into());
            return;
        }
        let mut notes = self.notes.clone();
        notes[idx].notebook = restored_book.clone();
        notes[idx].updated = now();
        notes[idx].stamp = "刚刚".into();
        // A restored child needs its ancestors to be reachable in the page tree.
        let mut parent = notes[idx].parent_id.clone();
        let mut visited = std::collections::HashSet::from([id.to_owned()]);
        while let Some(parent_id) = parent {
            if !visited.insert(parent_id.clone()) {
                break;
            }
            let Some(ancestor) = notes
                .iter_mut()
                .find(|n| n.id == parent_id && n.original_book == restored_book)
            else {
                break;
            };
            if ancestor.notebook == "回收站" {
                ancestor.notebook = restored_book.clone();
                ancestor.updated = now();
                ancestor.stamp = "刚刚".into();
            }
            parent = ancestor.parent_id.clone();
        }
        let mut order = self.book_order.clone();
        if !order.contains(&restored_book) {
            order.push(restored_book.clone());
        }
        if let Err(error) = self.store.save_with_preferences(
            &notes,
            &self.current,
            &[("book_order", &order.join(","))],
        ) {
            self.message = Some(format!("{}: {error}", self.tr("保存失败")));
            return;
        }
        self.remember();
        self.notes = notes;
        self.book_order = order;
        self.settings_return_focus = None;
        if self.book.is_empty() {
            self.book = restored_book.clone();
        }
        self.tree_expanded.insert(format!("book:{restored_book}"));
        if self.current == id || self.current.is_empty() {
            self.current = id.to_string();
            self.book = restored_book;
        }
        self.ensure_selected_note();
        self.rebuild_editors();
        self.load_active_doc_session(false);
        self.save();
    }
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        let val = if self.sidebar_collapsed { "1" } else { "0" };
        if let Err(e) = self.store.set_preference("sidebar_collapsed", val) {
            self.message = Some(format!("{}: {e}", self.tr("侧边栏状态保存失败")));
        }
    }
    pub fn open_settings(&mut self, tx: &mut rune::Tx) {
        self.arrange_anim_until = None;
        self.settings_return_focus = if self.editor.active.focused {
            Some(SettingsReturnFocus::Editor {
                card: self.editor.canvas_owner().map(str::to_owned),
                cell: self
                    .editor
                    .active
                    .editing_cell
                    .as_ref()
                    .map(|(id, r, c, _)| (*id, *r, *c)),
            })
        } else {
            tx.focused()
                .map(|key| SettingsReturnFocus::Native(key.to_owned()))
        };
        self.finish_edit();
        self.look.editing_color = None;
        self.look.color_error = false;
        self.look.opened_transparency = self.sidebar_material.transparency;
        self.settings = Some(SettingsPage::Appearance);
        tx.focus("settings-back", false);
    }
    pub fn close_settings(&mut self, tx: &mut rune::Tx) {
        self.settings = None;
        self.look.open = false;
        if let Some(n) = self.note() {
            self.pending_scroll = Some(n.pan);
        }
        let [x, y] = self.list_offset.get();
        tx.scroll_to_instant("notes-scroll", x, y);
        match self.settings_return_focus.take() {
            Some(SettingsReturnFocus::Native(key)) => tx.focus(key, false),
            Some(SettingsReturnFocus::Editor { card, cell }) => {
                if let Some(card) = card {
                    crate::canvas_document::activate(self, &card);
                }
                self.editor.active.focused = true;
                if let Some((id, r, c)) = cell {
                    crate::doc_tables::begin(self, id, r, c, tx);
                } else {
                    tx.blur();
                }
            }
            None => tx.blur(),
        }
    }
    pub fn save_sidebar_material(&mut self) {
        let value =
            serde_json::to_string(&self.sidebar_material).expect("sidebar settings serialize");
        if let Err(err) = self.store.set_preference("sidebar_material_light", &value) {
            self.message = Some(format!("{}: {err}", self.tr("侧栏外观保存失败")));
        }
    }
    // Test fixtures select an editor by opening a real file of the requested type.
    #[cfg(test)]
    pub fn choose_mode(&mut self, mode: WorkspaceMode) {
        self.finish_edit();
        if mode == WorkspaceMode::Document {
            self.seed_doc_notes_if_needed();
            self.save();
        }
        let id = self
            .notes
            .iter()
            .find(|n| WorkspaceMode::from_format(n.format) == mode && n.notebook == self.book)
            .expect("fixture contains the requested file type")
            .id
            .clone();
        self.select_note(&id);
    }
    pub fn note(&self) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == self.current)
    }
    pub fn note_mut(&mut self) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == self.current)
    }
    pub fn block_mut(&mut self, id: &str) -> Option<&mut Block> {
        self.note_mut()?.blocks.iter_mut().find(|b| b.id == id)
    }
    pub fn remember(&mut self) {
        let snapshot = self.history_snapshot(self.undo.last().map(Vec::as_slice));
        self.record_history(snapshot);
    }
    pub fn begin_canvas_gesture(&mut self) {
        self.canvas_gesture_history = Some(self.history_snapshot(self.undo.last().map(Vec::as_slice)));
    }
    pub fn end_canvas_gesture(&mut self, changed: bool) {
        if let Some(snapshot) = self.canvas_gesture_history.take() && changed {
            self.record_history(snapshot);
        }
    }
    fn record_history(&mut self, snapshot: Vec<std::sync::Arc<Note>>) {
        self.undo.push(snapshot);
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn history_snapshot(
        &self,
        previous: Option<&[std::sync::Arc<Note>]>,
    ) -> Vec<std::sync::Arc<Note>> {
        let old: HashMap<&str, &std::sync::Arc<Note>> = previous
            .into_iter()
            .flatten()
            .map(|n| (n.id.as_str(), n))
            .collect();
        self.notes
            .iter()
            .map(|note| {
                let clean;
                let value = if note
                    .blocks
                    .iter()
                    .any(|b| self.draft.as_deref() == Some(&b.id))
                {
                    clean = {
                        let mut n = note.clone();
                        n.blocks.retain(|b| self.draft.as_deref() != Some(&b.id));
                        n
                    };
                    &clean
                } else {
                    note
                };
                if let Some(old) = old
                    .get(value.id.as_str())
                    .filter(|old| old.as_ref() == value)
                {
                    std::sync::Arc::clone(old)
                } else {
                    std::sync::Arc::new(value.clone())
                }
            })
            .collect()
    }
    fn committed_notes(&self) -> Vec<Note> {
        let mut notes = self.notes.clone();
        if let Some(id) = &self.draft {
            for note in &mut notes {
                note.blocks.retain(|b| &b.id != id);
            }
        }
        notes
    }
    pub fn save(&mut self) {
        if self.settings.is_none() && self.pending_scroll.is_none() {
            let offset = self.scroll_pos.get().offset;
            if let Some(n) = self.note_mut() {
                n.pan = offset;
            }
        }
        let result = if self.draft.is_some() {
            self.store.save(&self.committed_notes(), &self.current)
        } else {
            self.store.save(&self.notes, &self.current)
        };
        if let Err(e) = result {
            self.message = Some(format!("{}: {e}", self.tr("保存失败")));
            self.document_save_failed = true;
        } else {
            self.pending_document_save = None;
            self.document_save_failed = false;
        }
    }
    pub fn queue_document_save(&mut self) {
        let now = std::time::Instant::now();
        let first = self.pending_document_save.map_or(now, |(first, _)| first);
        self.pending_document_save = Some((first, now));
        self.document_save_failed = false;
    }
    pub fn document_save_deadline(&self) -> Option<std::time::Instant> {
        if self.document_save_failed {
            return None;
        }
        self.pending_document_save.map(|(first, last)| {
            (last + std::time::Duration::from_millis(400))
                .min(first + std::time::Duration::from_secs(2))
        })
    }
    pub fn save_current_document(&mut self) -> bool {
        let Some(note) = self.notes.iter().find(|n| n.id == self.current) else {
            self.message = Some(self.tr("保存失败：当前文档不存在").into());
            self.document_save_failed = true;
            return false;
        };
        match self.store.save_note(note) {
            Ok(()) => {
                self.pending_document_save = None;
                self.document_save_failed = false;
                true
            }
            Err(e) => {
                self.message = Some(format!("{}: {e}", self.tr("保存失败")));
                self.document_save_failed = true;
                false
            }
        }
    }
    pub fn start_document_save(&mut self) -> bool {
        let Some(mut note) = self.note().cloned() else {
            return false;
        };
        if let Some(id) = &self.draft {
            note.blocks.retain(|b| &b.id != id);
        }
        if let Err(error) = self.store.save_note_background(note) {
            self.document_save_failed = true;
            self.message = Some(format!("{}: {error}", self.tr("保存失败")));
            return false;
        }
        self.pending_document_save = None;
        true
    }
    pub fn save_canvas_geometry(&mut self) {
        let pan = self.scroll_pos.get().offset;
        if let Some(n) = self.note_mut() {
            n.pan = pan;
            n.updated = now();
            n.stamp = "刚刚".into();
        }
        self.queue_document_save();
        if !self.store.saving() {
            self.start_document_save();
        }
    }
    pub fn changed(&mut self) {
        if let Some(n) = self.note_mut() {
            n.updated = now();
            n.stamp = "刚刚".into();
        }
        self.save();
    }
    pub fn rebuild_editors(&mut self) {
        self.editor.clear();
        if self.mode == WorkspaceMode::Canvas {
            self.editor.active = Default::default();
            self.canvas_visible.clear();
            self.canvas_scroll_observed = None;
            let size = self
                .canvas_viewport_rect
                .map(|r| [r[2], r[3]])
                .unwrap_or([1280., 943.]);
            let offset = self.pending_scroll.unwrap_or(self.scroll_pos.get().offset);
            crate::canvas_document::refresh_visible(self, size, offset);
        }
    }

    #[allow(dead_code)]

    pub fn toggle_tree(&mut self, key: &str) {
        if !self.tree_expanded.remove(key) {
            self.tree_expanded.insert(key.to_owned());
        }
    }
    pub fn create_doc_note(&mut self, book: &str, parent_id: Option<String>) -> String {
        self.arrange_anim_until = None;
        self.finish_edit();
        if self.document_save_failed {
            return String::new();
        }
        self.remember();
        self.mode = WorkspaceMode::Document;
        self.selected = None;
        self.hovered = None;
        let book = if book == "回收站" {
            "工作".to_string()
        } else {
            book.to_string()
        };
        self.tree_expanded.insert(format!("book:{book}"));
        if let Some(pid) = &parent_id {
            self.tree_expanded.insert(pid.clone());
        }
        let doc = crate::doc_core::DocDocument::empty();
        let id = uid();
        let note = Note {
            id: id.clone(),
            notebook: book.clone(),
            original_book: book.clone(),
            parent_id,
            title: self.tr("无标题文档").into(),
            preview: self.tr("开始输入正文…").into(),
            stamp: "刚刚".into(),
            updated: now(),
            blocks: Vec::new(),
            zoom: 1.,
            pan: [0., 0.],
            editor_version: 7,
            format: crate::doc_core::NoteFormat::Document,
            doc: Some(doc),
        };
        self.current = id.clone();
        self.nav_focus = Some(format!("note:{id}"));
        self.book = book;
        self.notes.insert(0, note);
        self.pending_scroll = Some([0., 0.]);
        self.load_active_doc_session(true);
        self.editor.active.focus_title_end();
        self.query.clear();
        self.save();
        id
    }
    pub fn promote_note(&mut self, id: &str) {
        let next_parent = self
            .notes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent_id.as_deref())
            .and_then(|pid| self.notes.iter().find(|p| p.id == pid))
            .and_then(|p| p.parent_id.clone());
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
            if n.parent_id != next_parent {
                self.remember();
                if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
                    n.parent_id = next_parent;
                    n.updated = now();
                    n.stamp = "刚刚".into();
                }
                self.save();
            }
        }
    }
    pub fn note_breadcrumbs(&self, id: &str) -> Vec<(String, String)> {
        let mut chain = Vec::new();
        let mut cursor = Some(id);
        let mut guard = 0;
        while let Some(cid) = cursor {
            if guard > 16 || chain.iter().any(|(seen, _)| seen == cid) {
                break;
            }
            guard += 1;
            if let Some(n) = self.notes.iter().find(|n| n.id == cid) {
                chain.push((n.id.clone(), n.title.clone()));
                cursor = n.parent_id.as_deref();
            } else {
                break;
            }
        }
        chain.reverse();
        chain
    }
    #[allow(dead_code)]
    pub fn delete_block(&mut self, id: &str) {
        self.finish_edit();
        self.remember();
        if let Some(n) = self.note_mut() {
            n.blocks.retain(|b| b.id != id);
        }

        self.changed();
    }
    pub fn mount_editor(&mut self, b: &Block) {
        if self.mode == WorkspaceMode::Canvas && b.is_textual() {
            self.canvas_visible.insert(b.id.clone());
            crate::canvas_document::mount(self, b);
        }
    }

    pub fn discard_draft(&mut self) {
        let Some(id) = self.draft.take() else {
            return;
        };
        if let Some(n) = self.note_mut() {
            n.blocks.retain(|b| b.id != id);
        }
        self.editor.remove_card(&id);
        self.canvas_visible.remove(&id);
        if self.selected.as_deref() == Some(&id) {
            self.selected = None;
        }
        if self.hovered.as_deref() == Some(&id) {
            self.hovered = None;
        }
    }
    pub fn tr(&self, key: &str) -> &'static str {
        self.language.text(key)
    }
    pub fn document_outline_visible(&self) -> bool {
        self.doc_outlines.contains(&self.current)
    }
    pub fn toggle_document_outline(&mut self) {
        if self.mode != WorkspaceMode::Document || self.note().is_none() {
            return;
        }
        let mut outlines = self.doc_outlines.clone();
        if !outlines.remove(&self.current) {
            outlines.insert(self.current.clone());
        }
        let value = serde_json::to_string(&outlines).expect("document IDs serialize");
        match self.store.set_preference("document_outlines", &value) {
            Ok(()) => self.doc_outlines = outlines,
            Err(error) => self.message = Some(format!("{}: {error}", self.tr("保存失败"))),
        }
    }
    pub fn set_notebook_icon_style(&mut self, style: crate::notebook_icons::Style) {
        if self.notebook_icon_style == style {
            return;
        }
        match self
            .store
            .set_preference("notebook_icon_style", style.key())
        {
            Ok(()) => self.notebook_icon_style = style,
            Err(error) => self.message = Some(format!("{}: {error}", self.tr("保存失败"))),
        }
    }
    pub fn set_language(&mut self, language: crate::i18n::Language) {
        match self.store.set_preference("ui_language", language.key()) {
            Ok(()) => self.language = language,
            Err(error) => self.message = Some(format!("{}: {error}", self.tr("保存失败"))),
        }
    }
    pub fn book_name<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(name) = self.book_names.get(key) {
            return name;
        }
        if !self.language.is_english() {
            return key;
        }
        match key {
            "工作" => "Work",
            "产品" => "Product",
            "设计" => "Design",
            "学习" => "Study",
            "生活" => "Life",
            "灵感" => "Ideas",
            "回收站" => "Trash",
            _ => panic!("unknown notebook key: {key}"),
        }
    }
    pub fn focus_page_rename(&mut self, id: &str, tx: &mut rune::Tx) {
        if self.sidebar_collapsed {
            self.toggle_sidebar();
        }
        for (ancestor, _) in self.note_breadcrumbs(id).into_iter().rev().skip(1) {
            self.tree_expanded.insert(ancestor);
        }
        tx.scroll_into_view(format!("page-row-{id}"));
        tx.focus("rename-field", false);
        tx.animated();
    }
    pub fn begin_rename(&mut self, target: RenameTarget) {
        self.finish_edit();
        let name = match &target {
            RenameTarget::Book(i) => self.book_name(self.book_at(*i)),
            RenameTarget::Note(id) => match self.notes.iter().find(|n| &n.id == id) {
                Some(n) => &n.title,
                None => return,
            },
        };
        self.name_input = TextEdit::new(name);
        self.name_input.set_selection(0, self.name_input.text.len());
        self.name_input.focused = true;
        self.renaming = Some(target);
    }
    pub fn end_rename(&mut self, commit: bool) {
        let Some(target) = self.renaming.take() else {
            return;
        };
        self.name_input.focused = false;
        let name = self.name_input.text.trim().to_owned();
        if !commit || name.is_empty() {
            return;
        }
        match target {
            RenameTarget::Note(id) => {
                if let Some(i) = self
                    .notes
                    .iter()
                    .position(|n| n.id == id && n.title != name)
                {
                    self.remember();
                    self.notes[i].title = name.clone();
                    if let Some(doc) = &mut self.notes[i].doc {
                        doc.title = crate::doc_core::RichText::plain(&name);
                        if self.current == id {
                            self.editor.active.core.doc.title = doc.title.clone();
                        }
                    }
                    self.notes[i].updated = now();
                    self.notes[i].stamp = "刚刚".into();
                    if self.current == id {}
                    self.save();
                }
            }
            RenameTarget::Book(i) => {
                let key = self.book_at(i).to_owned();
                if self.book_name(&key) == name {
                    return;
                }
                if self
                    .book_order
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once("回收站"))
                    .any(|other| self.book_name(other).to_lowercase() == name.to_lowercase())
                {
                    self.message = Some(self.tr("已有同名空间").into());
                    return;
                }
                match self
                    .store
                    .set_preference(&format!("book-name/{key}"), &name)
                {
                    Ok(()) => {
                        self.book_names.insert(key, name);
                    }
                    Err(e) => self.message = Some(format!("{}: {e}", self.tr("重命名失败"))),
                }
            }
        }
    }
    pub fn finish_edit(&mut self) {
        self.end_rename(true);
        if crate::canvas_document::enabled(self) {
            self.editor.active.commit_cell_edit();
            crate::doc_editor::sync_to_state(self);
            self.editor.active.slash = None;
            self.editor.active.popover = None;
            self.editor.active.core.preedit = None;
            if self.mode == WorkspaceMode::Canvas {
                self.editor.active.focused = false;
            }
        }
        self.discard_draft();
        if self.pending_scroll.is_none() && self.mode == WorkspaceMode::Canvas {
            let offset = self.scroll_pos.get().offset;
            if let Some(note) = self.note_mut() {
                note.pan = offset;
            }
        }
        if self.note().is_some() {
            self.save_current_document();
        } else if let Err(error) = self.store.flush_background() {
            self.document_save_failed = true;
            self.message = Some(format!("{}: {error}", self.tr("保存失败")));
        }
    }
    pub fn edit(&mut self, id: &str) {
        if self
            .note()
            .is_some_and(|n| n.blocks.iter().any(|b| b.id == id && b.is_textual()))
        {
            crate::canvas_document::activate(self, id);
        }
    }
    pub fn resize_text_height(&mut self, id: &str, height: f32) -> bool {
        let Some(old) = self
            .note()
            .and_then(|n| n.blocks.iter().find(|b| b.id == id))
            .cloned()
        else {
            return false;
        };
        let delta = height - old.h;
        if delta.abs() < 0.5 {
            return false;
        }
        // Measuring an existing card after opening or zooming must preserve its neighbours' coordinates.
        let reflow = self.draft.as_deref() != Some(id)
            && ((self.editor.canvas_owner() == Some(id) && self.editor.active.focused)
                || (self.canvas_gesture_history.is_some() && self.selected.as_deref() == Some(id)));
        if let Some(n) = self.note_mut() {
            for b in &mut n.blocks {
                if b.id == id {
                    b.h = height;
                } else if reflow
                    && b.y >= old.y + old.h - 1.
                    && b.x < old.x + old.w
                    && b.x + b.w > old.x
                {
                    b.y = (b.y + delta).max(86.);
                }
            }
        }
        true
    }
    pub fn scroll_key(&self) -> String {
        format!("page-viewport-{}", self.current)
    }
    pub fn zoom_at(&mut self, factor: f32, point: [f32; 2], tx: &mut rune::Tx) {
        let pos = self.scroll_pos.get().offset;
        if let Some(n) = self.note_mut() {
            let old = n.zoom;
            n.zoom = (old * factor).clamp(0.4, CANVAS_MAX_ZOOM);
            let k = n.zoom / old;
            let offset = [
                (pos[0] + point[0]) * k - point[0],
                (pos[1] + point[1]) * k - point[1],
            ];
            n.pan = [offset[0].max(0.), offset[1].max(0.)];
            self.pending_scroll = Some([offset[0].max(0.), offset[1].max(0.)]);
            tx.scroll_to_instant(self.scroll_key(), offset[0], offset[1]);
        }
        self.save();
        tx.instant();
    }
    pub fn create_at(&mut self, x: f32, y: f32) -> String {
        self.arrange_anim_until = None;
        let mut b = Block::new(
            Kind::Text,
            (x - crate::doc_editor::CARD_TEXT_INSET).max(12.),
            (y - 5.).max(86.),
            360.,
            40.,
            "",
            "",
        );
        b.min_h = 40.;
        b.structured = true;
        b.body = crate::file_format::write_document_body(&crate::doc_core::DocDocument::empty())
            .expect("empty document serializes");
        let id = b.id.clone();
        self.mount_editor(&b);
        crate::canvas_document::activate(self, &id);
        if let Some(n) = self.note_mut() {
            n.blocks.push(b);
        }
        self.draft = Some(id.clone());
        id
    }
    pub fn visible_canvas_bounds(&self, bounds: [f32; 4]) -> [f32; 4] {
        let zoom = self.note().map_or(1., |n| n.zoom);
        let [x, y] = self.scroll_pos.get().offset.map(|v| v / zoom);
        [bounds[0] + x, bounds[1] + y, bounds[2] + x, bounds[3] + y]
    }
    pub fn create_in_canvas(&mut self, x: f32, y: f32, bounds: [f32; 4]) -> String {
        let bounds = self.visible_canvas_bounds(bounds);
        let id = self.create_at(x, y);
        let note = self.note_mut().unwrap();
        let index = note.blocks.iter().position(|b| b.id == id).unwrap();
        let left = (x - crate::doc_editor::CARD_TEXT_INSET).max(0.);
        let width = 360_f32.min((bounds[2] - left).max(150.));
        let top = (y - 16.).max(crate::canvas::ORIGIN_Y + 8.);
        let b = &mut note.blocks[index];
        b.x = left;
        b.y = top;
        b.w = width;
        id
    }
    pub fn constrain_resize(&mut self, id: &str) {
        let [_, _, right, bottom] = self.drag_bounds;
        let Some(note) = self.note() else { return };
        let Some(b) = note.blocks.iter().find(|b| b.id == id) else {
            return;
        };
        let max_w = (right - b.x).max(72.);
        let max_h = (bottom - b.y).max(40.);
        if let Some(b) = self.block_mut(id) {
            let scale = (max_w / b.w).min(max_h / b.h).min(1.);
            if matches!(b.kind, Kind::Image | Kind::Handwritten) {
                b.w *= scale;
                b.h *= scale;
            } else {
                b.w = b.w.min(max_w);
                b.min_h = b.min_h.min(max_h);
                if b.kind == Kind::Shape {
                    b.h = b.h.min(max_h);
                }
            }
        }
    }
    pub fn arrange_blocks(&mut self, viewport_width: f32, tx: &mut rune::Tx) {
        self.finish_edit();
        let Some(note) = self.note() else { return };
        let mut order: Vec<usize> = (0..note.blocks.len()).collect();
        order.sort_by(|&a, &b| {
            note.blocks[a]
                .y
                .total_cmp(&note.blocks[b].y)
                .then_with(|| note.blocks[a].x.total_cmp(&note.blocks[b].x))
        });
        let right = (viewport_width / note.zoom - 32.).max(32.);
        let mut positions = Vec::with_capacity(order.len());
        let (mut x, mut y, mut row_height) = (32., 112., 0_f32);
        for index in order {
            let block = &note.blocks[index];
            if x > 32. && x + block.w > right {
                x = 32.;
                y += row_height + 48.;
                row_height = 0.;
            }
            positions.push((index, x, y));
            x += block.w + 40.;
            row_height = row_height.max(block.h);
        }
        let changed = positions.iter().any(|&(index, x, y)| {
            let b = &note.blocks[index];
            (b.x - x).abs() > 0.5 || (b.y - y).abs() > 0.5
        });
        if changed {
            self.remember();
            let note = self.note_mut().unwrap();
            for (index, x, y) in positions {
                note.blocks[index].x = x;
                note.blocks[index].y = y;
            }
            self.arrange_anim_until = Some(self.clock + 0.65);
            tx.wake_after(std::time::Duration::from_millis(660));
        }
        self.selected = None;
        self.hovered = None;
        self.pending_scroll = None;
        tx.blur();
        tx.scroll_to_with(self.scroll_key(), 0., 0., rune::Spring::new(0.42, 0.));
        if changed {
            self.changed();
        } else {
            self.save();
        }
        tx.animated();
    }
    pub fn merge_containers(&mut self, source: &str, target: &str) {
        if source == target {
            return;
        }
        self.finish_edit();
        if self.document_save_failed {
            return;
        }
        let Some(note) = self.note() else {
            return;
        };
        let (Some(src), Some(dst)) = (
            note.blocks.iter().find(|b| b.id == source),
            note.blocks.iter().find(|b| b.id == target),
        ) else {
            return;
        };
        if !src.is_textual() || !dst.is_textual() {
            return;
        }
        let merged = (|| -> Result<String, String> {
            let mut doc = crate::file_format::card_document(dst)?;
            let mut extra = crate::file_format::card_document(src)?.blocks;
            fn append_ids(blocks: &mut [crate::doc_core::DocBlock], next: &mut u64) {
                for b in blocks {
                    b.id = *next;
                    *next += 1;
                    append_ids(&mut b.children, next);
                }
            }
            append_ids(&mut extra, &mut doc.next_id);
            doc.blocks.extend(extra);
            crate::file_format::write_document_body(&doc)
        })();
        let body = match merged {
            Ok(body) => body,
            Err(error) => {
                self.message = Some(format!("无法合并卡片：{error}"));
                return;
            }
        };
        if self.canvas_gesture_history.is_some() {
            self.end_canvas_gesture(true);
        } else {
            self.remember();
        }
        let card = self.block_mut(target).expect("merge target exists");
        card.body = body;
        card.structured = true;
        card.kind = Kind::Text;
        card.title.clear();
        card.body_html.clear();
        card.table = None;
        self.note_mut()
            .expect("active canvas")
            .blocks
            .retain(|b| b.id != source);
        self.rebuild_editors();
        self.edit(target);
        self.changed();
    }

    pub fn select_note(&mut self, id: &str) {
        self.finish_edit();
        if self.document_save_failed {
            return;
        }
        let note = match self.store.load_note(id) {
            Ok(note) => note,
            Err(error) => {
                self.message = Some(format!("{}: {error}", self.tr("无法读取笔记")));
                return;
            }
        };
        let Some(existing) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        *existing = note;
        self.activate_saved_note(id);
        self.queue_document_save();
    }

    pub(crate) fn insert_imported_notes(&mut self, mut imported: Vec<Note>, book: &str) -> Result<(), String> {
        let selected = imported.first().expect("import batch has a page").id.clone();
        self.finish_edit();
        if self.document_save_failed {
            return Err(self.tr("保存失败").into());
        }
        imported.extend(self.committed_notes());
        self.store.save_with_preferences(&imported, &selected, &[("selected_book", book)])?;
        self.remember();
        self.notes = imported;
        self.query.clear();
        self.activate_saved_note(&selected);
        Ok(())
    }

    fn activate_saved_note(&mut self, id: &str) {
        let existing = self.notes.iter().find(|n| n.id == id).expect("saved note exists");
        self.current = id.into();
        self.book = existing.notebook.clone();
        self.mode = WorkspaceMode::from_format(existing.format);
        self.nav_focus = Some(format!("note:{id}"));
        self.arrange_anim_until = None;
        self.tree_expanded.insert(format!("book:{}", self.book));
        if let Some(pid) = existing.parent_id.clone() {
            self.tree_expanded.insert(pid);
        }
        self.pending_scroll = Some(if self.mode == WorkspaceMode::Canvas {
            existing.pan
        } else {
            [0., 0.]
        });
        self.hovered = None;
        self.selected = None;
        self.rebuild_editors();
        self.load_active_doc_session(true);
    }
    pub fn select_book(&mut self, book: &str) {
        self.finish_edit();
        if self.document_save_failed {
            return;
        }
        let id = self
            .notes
            .iter()
            .find(|n| n.notebook == book)
            .map(|n| n.id.clone());
        self.query.clear();
        if let Some(id) = id {
            self.select_note(&id);
            if self.document_save_failed {
                return;
            }
        } else {
            self.book = book.into();
            self.current.clear();
            self.mode = WorkspaceMode::Document;
            self.selected = None;
            self.hovered = None;
            self.pending_scroll = Some([0., 0.]);
            self.rebuild_editors();
            self.load_active_doc_session(false);
            self.save();
        }
        self.nav_focus = Some(format!("book:{book}"));
        if let Err(error) = self.store.set_preference("selected_book", &self.book) {
            self.message = Some(format!("{}: {error}", self.tr("保存失败")));
        }
    }
    pub fn new_note(&mut self) {
        if self.book_order.is_empty() {
            return;
        }
        let book = self.book.clone();
        if self.mode == WorkspaceMode::Document {
            self.create_doc_note(&book, None);
        } else {
            self.create_canvas_note(&book, None);
        }
    }
    pub fn create_canvas_note(&mut self, book: &str, parent_id: Option<String>) -> String {
        self.arrange_anim_until = None;
        self.finish_edit();
        if self.document_save_failed {
            return String::new();
        }
        self.remember();
        self.mode = WorkspaceMode::Canvas;
        self.selected = None;
        self.hovered = None;
        self.editor.active.focused = false;
        let book = book.to_owned();
        let note = Note {
            id: uid(),
            notebook: book.clone(),
            original_book: book.clone(),
            parent_id,
            title: self.tr("未命名笔记").into(),
            preview: self.tr("开始记录你的想法").into(),
            stamp: "刚刚".into(),
            updated: now(),
            blocks: vec![],
            zoom: 1.,
            pan: [0., 0.],
            editor_version: 6,
            format: crate::doc_core::NoteFormat::Canvas,
            doc: None,
        };
        self.current = note.id.clone();
        self.book = book;
        self.notes.insert(0, note);
        self.pending_scroll = Some([0., 0.]);
        self.rebuild_editors();
        self.query.clear();
        self.save();
        self.begin_rename(RenameTarget::Note(self.current.clone()));
        self.current.clone()
    }
    pub fn add(&mut self, kind: Kind) {
        if self.note().is_none() {
            self.new_note();
        }
        let anchor = self
            .editor
            .canvas_owner()
            .and_then(|owner| self.note()?.blocks.iter().find(|b| b.id == owner))
            .map(|b| [b.x, b.y + b.h + 24.]);
        self.finish_edit();
        let zoom = self.note().map_or(1., |n| n.zoom);
        let pos = self.scroll_pos.get().offset;
        let [insert_x, insert_y] = anchor.unwrap_or([48. + pos[0] / zoom, 130. + pos[1] / zoom]);
        if kind == Kind::Text {
            self.create_at(insert_x + 8., insert_y + 5.);
            return;
        }
        self.remember();
        let (w, h, title, body) = match kind {
            Kind::Text => (360., 40., "", ""),
            Kind::Tasks => (300., 220., self.tr("待办事项"), ""),
            Kind::Code => (
                388.,
                225.,
                "TypeScript",
                if self.language.is_english() {
                    "// Write your code here\n"
                } else {
                    "// 写下你的代码\n"
                },
            ),
            Kind::Callout => (
                388.,
                160.,
                self.tr("设计思路"),
                if self.language.is_english() {
                    "Capture, reflect, and organize your ideas."
                } else {
                    "记录、思考与整理，自然流动。"
                },
            ),
            Kind::Shape => (280., 180., "", ""),
            _ => (342., 242., "", ""),
        };
        let mut b = Block::new(kind.clone(), insert_x, insert_y, w, h, title, body);
        if kind == Kind::Tasks {
            b.body = if self.language.is_english() {
                "- [ ] New task"
            } else {
                "- [ ] 新的待办事项"
            }
            .into();
        }
        if kind == Kind::Table {
            b.table = Some(crate::table_data::TableData::default());
        }
        if b.is_textual() {
            let body = crate::file_format::card_document(&b)
                .and_then(|doc| crate::file_format::write_document_body(&doc));
            match body {
                Ok(body) => {
                    b.body = body;
                    b.structured = true;
                    b.kind = Kind::Text;
                    b.title.clear();
                    b.body_html.clear();
                    b.table = None;
                }
                Err(error) => {
                    self.message = Some(error);
                    return;
                }
            }
        }
        let id = b.id.clone();
        self.mount_editor(&b);
        if let Some(n) = self.note_mut() {
            n.blocks.push(b);
        }
        self.selected = Some(id.clone());
        self.changed();
        if matches!(kind, Kind::Text | Kind::Code | Kind::Callout) {
            self.edit(&id);
        }
    }
    pub fn import_image(&mut self, path: &Path) {
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        if !["png", "jpg", "jpeg"].contains(&ext.as_str()) {
            self.message = Some(self.tr("请选择 PNG 或 JPEG 图片").into());
            return;
        }
        let rel = format!("images/{}.{}", uid(), ext);
        if let Err(e) = std::fs::copy(path, self.store.root.join(&rel)) {
            self.message = Some(format!("{}: {e}", self.tr("无法导入图片")));
            return;
        }
        self.add(Kind::Image);
        let id = self.selected.clone().unwrap();
        if let Some(b) = self.block_mut(&id) {
            b.image = rel;
        }
        self.changed();
    }
    pub fn delete_selected(&mut self) {
        self.finish_edit();
        if let Some(id) = self.selected.take() {
            self.remember();
            if let Some(n) = self.note_mut() {
                n.blocks.retain(|b| b.id != id);
            }
            self.changed();
        }
    }
    pub fn trash_note(&mut self) {
        if self.note().is_some_and(|n| n.notebook == "回收站") {
            let id = self.current.clone();
            self.restore_note_by_id(&id);
            return;
        }
        self.finish_edit();
        self.remember();
        if let Some(n) = self.note_mut() {
            n.original_book = n.notebook.clone();
            n.notebook = "回收站".into();
        }
        self.changed();
        let book = self.book.clone();
        self.select_book(&book);
    }
    pub fn duplicate(&mut self) {
        self.finish_edit();
        if let Some(id) = self.selected.clone() {
            if let Some(mut b) = self
                .note()
                .and_then(|n| n.blocks.iter().find(|b| b.id == id))
                .cloned()
            {
                self.remember();
                b.id = uid();
                b.x += 24.;
                b.y += 24.;
                self.selected = Some(b.id.clone());
                self.mount_editor(&b);
                self.note_mut().unwrap().blocks.push(b);
                self.changed();
            }
        }
    }
    pub fn undo(&mut self, redo: bool) {
        self.arrange_anim_until = None;
        self.finish_edit();
        let next = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(next) = next {
            let current = self.history_snapshot(Some(&next));
            self.notes = next.iter().map(|n| n.as_ref().clone()).collect();
            if redo {
                self.undo.push(current)
            } else {
                self.redo.push(current)
            };
            self.ensure_selected_note();
            self.selected = None;
            self.rebuild_editors();
            self.load_active_doc_session(false);
            self.save();
        }
    }
    /// Mounted rows and cards own the pixels. Rune shares handles for repeated
    /// draws and releases decoded pixels once the last element using them goes away.
    pub(crate) fn note_image(&self, src: &str, column_width: Option<f32>) -> Option<Image> {
        let cache = self.cache.as_ref()?;
        let rel = match src {
            "builtin:architecture" => "images/echo-architecture.png",
            "builtin:handwritten" => "images/echo-handwritten.png",
            rel if rel.starts_with("images/") && Path::new(rel).components().all(|c| matches!(c, std::path::Component::Normal(_))) => rel,
            _ => return None,
        };
        let path = self.store.root.join(rel);
        let image = if let Some(width) = column_width {
            // Size for the full column so dragging any of the four corners can
            // reuse the same pixels. A canvas can zoom to 250% without reloading.
            let zoom = if self.mode == WorkspaceMode::Canvas { CANVAS_MAX_ZOOM } else { 1. };
            let pixels = (width * zoom * self.display_scale.get()).ceil() as u32;
            cache.file_preview(path, pixels.clamp(1, 8192).next_multiple_of(128))
        } else {
            cache.file(path, 400., 2.)
        };
        if !image.is_ready() {
            self.pending_images.borrow_mut().entry(image.id()).or_insert_with(|| image.clone());
        }
        Some(image)
    }
    pub fn poll_images(&mut self) -> bool {
        let pending = self.pending_images.get_mut();
        let before = pending.len();
        let mut error = None;
        pending.retain(|_, image| {
            if !image.is_ready() { return true; }
            if let Some(reason) = image.error() { error = Some(reason.to_string()); }
            false
        });
        let changed = before != pending.len();
        if let Some(error) = error {
            self.message = Some(format!("{}: {error}", self.tr("图片无法读取")));
        }
        changed
    }
}

#[cfg(test)]
pub fn seed() -> Vec<Note> {
    let mut title = Block::new(
        Kind::Text,
        32.,
        118.,
        360.,
        204.,
        "产品设计方案",
        "围绕提升核心体验与使用效率，优化产品的整体结构、关键流程与交互细节，打磨在真实场景中的稳定性与易用性。",
    );
    title.id = "intro".into();
    let mut tasks = Block::new(Kind::Tasks, 458., 122., 296., 228., "关键待办", "");
    tasks.id = "tasks".into();
    tasks.body = [
        ("梳理核心流程", true),
        ("完成交互原型", true),
        ("设计评审与反馈", false),
        ("补充未覆盖的场景", false),
        ("确定开发排期", false),
    ]
    .iter()
    .map(|(text, done)| format!("- [{}] {text}", if *done { "x" } else { " " }))
    .collect::<Vec<_>>()
    .join("\n");
    let mut photo = Block::new(Kind::Image, 42., 392., 342., 242., "", "");
    photo.id = "photo".into();
    photo.image = "builtin:architecture".into();
    let mut code = Block::new(
        Kind::Code,
        420.,
        402.,
        346.,
        225.,
        "TypeScript",
        "export function createNote() {\n  return {\n    id: nanoid(),\n    createdAt: new Date(),\n    content: ''\n  }\n}",
    );
    code.id = "code".into();
    let mut hand = Block::new(Kind::Handwritten, 24., 676., 380., 214., "", "");
    hand.id = "handwritten".into();
    hand.image = "builtin:handwritten".into();
    let mut callout = Block::new(
        Kind::Callout,
        420.,
        696.,
        346.,
        156.,
        "设计思路",
        "以更轻量、直观的方式组织信息，让记录、思考与整理在同一空间中自然流动。",
    );
    callout.id = "callout".into();
    let content = [
        (
            "产品设计方案",
            "整体目标、核心流程、关键功能…",
            "今天 10:24",
        ),
        ("版本迭代计划", "v1.3 的功能范围与时间安排", "昨天 18:07"),
        ("用户访谈记录", "4 位用户的关键反馈与洞察", "9月15日"),
        ("设计评审要点", "界面、交互、可用性、性能", "9月12日"),
        ("需求收集", "下一阶段的功能候选清单", "9月10日"),
    ];
    let mut notes = vec![];
    for (i, (name, preview, stamp)) in content.into_iter().enumerate() {
        let blocks = if i == 0 {
            vec![
                title.clone(),
                tasks.clone(),
                photo.clone(),
                code.clone(),
                hand.clone(),
                callout.clone(),
            ]
        } else {
            vec![Block::new(Kind::Text, 48., 130., 560., 260., name, preview)]
        };
        notes.push(Note {
            id: format!("sample-{i}"),
            notebook: "工作".into(),
            original_book: "工作".into(),
            parent_id: None,
            title: name.into(),
            preview: preview.into(),
            stamp: stamp.into(),
            updated: 100 - i as i64,
            blocks,
            zoom: 1.,
            pan: [0., 0.],
            editor_version: 5,
            format: crate::doc_core::NoteFormat::Canvas,
            doc: None,
        });
    }
    notes
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
