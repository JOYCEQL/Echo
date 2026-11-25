//! File library. Markdown contains the text; JSON contains settings, order and
//! canvas geometry. A durable write-ahead file makes multi-file edits resumable.
use crate::{file_format, library_data::Note};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    id: String,
    path: String,
    layout: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Change {
    path: String,
    before: Option<u64>,
    content: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Transaction {
    version: u32,
    changes: Vec<Change>,
}
pub struct Store {
    pub root: PathBuf,
    index: RefCell<Index>,
    baseline: RefCell<HashMap<String, Option<u64>>>,
    logical: RefCell<HashMap<String, u64>>,
    locations: RefCell<HashMap<String, (String, String)>>,
    worker: crate::save_writer::Writer,
    pending: RefCell<Option<std::sync::mpsc::Receiver<(Snapshot, Result<(), String>)>>>,
}
struct Snapshot {
    index: Index,
    baseline: HashMap<String, Option<u64>>,
    logical: HashMap<String, u64>,
    locations: HashMap<String, (String, String)>,
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Err(error) = self.flush_background() {
            eprintln!("Echo pending save: {error}");
        }
    }
}
fn hash(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
fn fingerprint(path: &Path) -> Result<Option<u64>, String> {
    Ok(read_optional(path)?.map(|b| hash(&b)))
}
pub(crate) fn safe_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty()
        || Path::new(rel)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("Invalid library path: {rel}"));
    }
    let path = root.join(rel);
    // A symlink must never turn a library edit into a write outside the library.
    let mut at = root.to_path_buf();
    for c in Path::new(rel).components() {
        at.push(c);
        if let Ok(meta) = fs::symlink_metadata(&at) {
            if meta.file_type().is_symlink() {
                return Err(format!("Symlinks are not writable library files: {rel}"));
            }
        }
    }
    Ok(path)
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".echo-write-{}", crate::identity::uid()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        crate::platform::replace_file(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}
fn file_name(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(70)
        .collect();
    let s = s.trim_matches([' ', '.']);
    let stem = s.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            })
        });
    if s.is_empty() {
        "Untitled".into()
    } else if reserved {
        format!("_{s}")
    } else {
        s.into()
    }
}
fn difference(a: &serde_json::Value, b: &serde_json::Value, path: &str) -> String {
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        for (k, v) in a {
            if b.get(k) != Some(v) {
                return difference(
                    v,
                    b.get(k).unwrap_or(&serde_json::Value::Null),
                    &format!("{path}.{k}"),
                );
            }
        }
    }
    if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        if a.len() != b.len() {
            return format!("{path}.length ({} -> {})", a.len(), b.len());
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            if a != b {
                return difference(a, b, &format!("{path}[{i}]"));
            }
        }
    }
    path.into()
}
fn note_hash(note: &Note) -> Result<u64, String> {
    Ok(hash(&serde_json::to_vec(note).map_err(|e| e.to_string())?))
}
fn relocate_assets(note: &mut Note, disk: bool) {
    fn path(s: &mut String, disk: bool) {
        if disk {
            if s == "builtin:architecture" {
                *s = "../../images/echo-architecture.png".into();
            } else if s == "builtin:handwritten" {
                *s = "../../images/echo-handwritten.png".into();
            } else if s.starts_with("images/") {
                *s = format!("../../{s}");
            }
        } else if s == "../../images/echo-architecture.png" {
            *s = "builtin:architecture".into();
        } else if s == "../../images/echo-handwritten.png" {
            *s = "builtin:handwritten".into();
        } else if let Some(t) = s.strip_prefix("../../images/") {
            *s = format!("images/{t}");
        }
    }
    fn md(s: &mut String, disk: bool) {
        let (a, b) = if disk {
            ("](images/", "](../../images/")
        } else {
            ("](../../images/", "](images/")
        };
        *s = s.replace(a, b);
        let (a, b) = if disk {
            ("](<images/", "](<../../images/")
        } else {
            ("](<../../images/", "](<images/")
        };
        *s = s.replace(a, b);
    }
    fn blocks(bs: &mut [crate::doc_core::DocBlock], disk: bool) {
        for b in bs {
            if let crate::doc_core::DocBlockKind::Image { src, .. } = &mut b.kind {
                path(src, disk);
            }
            for r in &mut b.text.runs {
                if let Some(l) = &mut r.marks.link {
                    path(l, disk);
                }
            }
            if let crate::doc_core::DocBlockKind::Table(t) = &mut b.kind {
                for row in &mut t.rows {
                    for c in row {
                        md(c, disk);
                    }
                }
            }
            blocks(&mut b.children, disk);
        }
    }
    for b in &mut note.blocks {
        path(&mut b.image, disk);
        md(&mut b.body, disk);
        if let Some(t) = &mut b.table {
            for row in &mut t.rows {
                for c in row {
                    md(c, disk);
                }
            }
        }
    }
    if let Some(d) = &mut note.doc {
        blocks(&mut d.blocks, disk);
    }
}
impl Store {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            index: self.index.borrow().clone(),
            baseline: self.baseline.borrow().clone(),
            logical: self.logical.borrow().clone(),
            locations: self.locations.borrow().clone(),
        }
    }
    fn accept(&self, (state, result): (Snapshot, Result<(), String>)) -> Result<(), String> {
        *self.index.borrow_mut() = state.index;
        *self.baseline.borrow_mut() = state.baseline;
        *self.logical.borrow_mut() = state.logical;
        *self.locations.borrow_mut() = state.locations;
        result
    }
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.worker.set_waker(wake);
    }
    pub fn saving(&self) -> bool {
        self.pending.borrow().is_some()
    }
    pub fn poll_save(&self) -> Option<Result<(), String>> {
        let answer = match self.pending.borrow().as_ref()?.try_recv() {
            Ok(answer) => Ok(answer),
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("Background save worker stopped".to_string())
            }
        };
        self.pending.borrow_mut().take();
        Some(answer.and_then(|answer| self.accept(answer)))
    }
    pub fn flush_background(&self) -> Result<(), String> {
        let pending = self.pending.borrow_mut().take();
        if let Some(receiver) = pending {
            self.accept(
                receiver
                    .recv()
                    .map_err(|_| "Background save worker stopped".to_string())?,
            )?;
        }
        Ok(())
    }
    /// Submit one durable write. No newer request can cancel this transaction.
    /// The channel is the durability barrier for explicit save, reads, structural edits and drop.
    pub fn save_note_background(&self, note: Note) -> Result<(), String> {
        assert!(!self.saving(), "one durable writer per library");
        let state = self.snapshot();
        let root = self.root.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.worker.submit(move || {
            let mut disk = Store {
                root,
                index: RefCell::new(state.index),
                baseline: RefCell::new(state.baseline),
                logical: RefCell::new(state.logical),
                locations: RefCell::new(state.locations),
                worker: Default::default(),
                pending: RefCell::new(None),
            };
            let result = disk
                .save_note(&note)
                .and_then(|_| disk.set_preference("selected", &note.id));
            let _ = sender.send((disk.snapshot(), result));
        })?;
        *self.pending.borrow_mut() = Some(receiver);
        Ok(())
    }
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(root.join(".echo")).map_err(|e| e.to_string())?;
        Self::recover(&root)?;
        let mut index = match read_optional(&root.join(".echo/index.json"))? {
            Some(bytes) => {
                let i: Index = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Invalid library index: {e}"))?;
                if i.version != 1 {
                    return Err("Unsupported file library version".into());
                }
                i
            }
            None => {
                if root.join("echo.sqlite3").exists() || root.join("noteflow.sqlite3").exists() {
                    return Err("This library requires the one-time scripts/migrate_sqlite.py migration. No data has been changed.".into());
                }
                Index {
                    version: 1,
                    entries: vec![],
                }
            }
        };
        Self::discover(&root, &mut index)?;
        fs::create_dir_all(root.join("images")).map_err(|e| e.to_string())?;
        for (name, data) in [
            (
                "echo-architecture.png",
                include_bytes!("../../assets/architecture.png").as_slice(),
            ),
            (
                "echo-handwritten.png",
                include_bytes!("../../assets/handwritten.png").as_slice(),
            ),
        ] {
            let p = root.join("images").join(name);
            if !p.exists() {
                atomic(&p, data)?;
            }
        }
        let s = Self {
            root,
            index: RefCell::new(index),
            baseline: RefCell::new(HashMap::new()),
            logical: RefCell::new(HashMap::new()),
            locations: RefCell::new(HashMap::new()),
            worker: Default::default(),
            pending: RefCell::new(None),
        };
        for p in [".echo/index.json", ".echo/settings.json"] {
            s.observe(p)?;
        }
        Ok(s)
    }
    fn discover(root: &Path, index: &mut Index) -> Result<(), String> {
        fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result<(), String> {
            if !dir.exists() {
                return Ok(());
            }
            for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let ty = entry.file_type().map_err(|e| e.to_string())?;
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                if ty.is_symlink() {
                    return Err(format!(
                        "Symlink in note library: {}",
                        entry.path().display()
                    ));
                }
                if ty.is_dir() {
                    walk(root, &entry.path(), found)?;
                } else if entry.path().extension().is_some_and(|x| x == "md") {
                    // Index paths and Markdown image references always use `/`, even
                    // when read_dir returns Windows-native backslash separators.
                    let path = entry.path();
                    let parts = path.strip_prefix(root).map_err(|e| e.to_string())?
                        .iter().map(|part| part.to_str().ok_or("Non-Unicode note filename"))
                        .collect::<Result<Vec<_>, _>>()?;
                    found.push(parts.join("/"));
                }
            }
            Ok(())
        }
        let mut paths = vec![];
        for dir in ["Documents", "Canvas"] {
            walk(root, &root.join(dir), &mut paths)?;
        }
        paths.sort();
        let mut discovered = vec![];
        let mut ids = HashSet::new();
        for path in paths {
            let md = fs::read_to_string(safe_path(root, &path)?).map_err(|e| e.to_string())?;
            let layout = if path.starts_with("Canvas/") {
                Some(path.trim_end_matches(".md").to_string() + ".canvas.json")
            } else {
                None
            };
            let note = Self::read_note(root, &path, layout.as_deref(), &md)?;
            if !ids.insert(note.id.clone()) {
                return Err(format!(
                    "Duplicate note ID in {path}; copies must have unique identities"
                ));
            }
            discovered.push(Entry {
                id: note.id,
                path,
                layout,
            });
        }
        let order: HashMap<_, _> = index
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        discovered.sort_by_key(|e| order.get(&e.id).copied().unwrap_or(usize::MAX));
        index.entries = discovered;
        Ok(())
    }
    fn read_note(root: &Path, path: &str, layout: Option<&str>, md: &str) -> Result<Note, String> {
        let layout = layout
            .map(|p| fs::read_to_string(safe_path(root, p)?).map_err(|e| e.to_string()))
            .transpose()?;
        if file_format::is_echo_file(md) {
            file_format::decode(md, layout.as_deref()).map_err(|e| format!("{path}: {e}"))
        } else {
            if layout.is_some() {
                return Err("Canvas Markdown requires card identities and a layout file".into());
            }
            let parts: Vec<_> = path.split('/').collect();
            let book = parts
                .get(1)
                .ok_or("A document must be inside a space folder")?;
            let preferences: BTreeMap<String, String> =
                match read_optional(&root.join(".echo/settings.json"))? {
                    Some(bytes) => serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Invalid settings: {e}"))?,
                    None => BTreeMap::new(),
                };
            let registered = preferences
                .get("book_order")
                .is_some_and(|order| order.split(',').any(|key| key == *book));
            if !crate::library_data::BOOKS.contains(book) && !registered {
                return Err(format!("Unknown space folder: {book}"));
            }
            let id = format!("file-{:016x}", hash(path.as_bytes()));
            let name = Path::new(path).file_stem().unwrap().to_string_lossy();
            file_format::import_markdown(md, &id, book, &name)
        }
    }
    fn observe(&self, rel: &str) -> Result<(), String> {
        let value = fingerprint(&safe_path(&self.root, rel)?)?;
        self.baseline.borrow_mut().insert(rel.into(), value);
        Ok(())
    }
    fn recover(root: &Path) -> Result<(), String> {
        let journal = root.join(".echo/transaction.json");
        let Some(bytes) = read_optional(&journal)? else {
            return Ok(());
        };
        let tx: Transaction =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid write journal: {e}"))?;
        if tx.version != 1 {
            return Err("Unsupported transaction version".into());
        }
        // Validate every target first; a changed external file is never overwritten.
        for c in &tx.changes {
            let current = fingerprint(&safe_path(root, &c.path)?)?;
            let after = c.content.as_ref().map(|s| hash(s.as_bytes()));
            if current != c.before && current != after {
                return Err(format!(
                    "External edit conflicts with interrupted save: {}",
                    c.path
                ));
            }
        }
        for c in &tx.changes {
            let path = safe_path(root, &c.path)?;
            match &c.content {
                Some(s) => {
                    if fingerprint(&path)? != Some(hash(s.as_bytes())) {
                        atomic(&path, s.as_bytes())?;
                    }
                }
                None => {
                    if path.exists() {
                        fs::remove_file(&path).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        fs::remove_file(journal).map_err(|e| e.to_string())?;
        // Windows commits each replacement with MOVEFILE_WRITE_THROUGH. It has no
        // POSIX directory fsync; a surviving journal is replayed idempotently on reopen.
        #[cfg(target_os = "macos")]
        fs::File::open(root.join(".echo"))
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn commit(&self, desired: BTreeMap<String, Option<String>>) -> Result<(), String> {
        if self.root.join(".echo/transaction.json").exists() {
            return Err("An interrupted save is pending; reopen the library to complete it".into());
        }
        let mut changes = vec![];
        for (path, content) in desired {
            let absolute = safe_path(&self.root, &path)?;
            let current = fingerprint(&absolute)?;
            let expected = self.baseline.borrow().get(&path).copied().unwrap_or(None);
            if current != expected {
                return Err(format!(
                    "File changed outside Echo; reopen the library before saving: {path}"
                ));
            }
            if current != content.as_ref().map(|s| hash(s.as_bytes())) {
                changes.push(Change {
                    path,
                    before: current,
                    content,
                });
            }
        }
        if changes.is_empty() {
            return Ok(());
        }
        let tx = Transaction {
            version: 1,
            changes,
        };
        atomic(
            &self.root.join(".echo/transaction.json"),
            json(&tx)?.as_bytes(),
        )?;
        Self::recover(&self.root)?;
        for c in tx.changes {
            self.observe(&c.path)?;
        }
        Ok(())
    }
    pub fn load(&self) -> Result<Vec<Note>, String> {
        self.flush_background()?;
        let mut notes = vec![];
        for entry in &self.index.borrow().entries {
            let md = fs::read_to_string(safe_path(&self.root, &entry.path)?)
                .map_err(|e| format!("{}: {e}", entry.path))?;
            let mut note = Self::read_note(&self.root, &entry.path, entry.layout.as_deref(), &md)?;
            if note.id != entry.id {
                return Err(format!("Note identity mismatch: {}", entry.path));
            }
            relocate_assets(&mut note, false);
            self.observe(&entry.path)?;
            if let Some(p) = &entry.layout {
                self.observe(p)?;
            }
            self.logical
                .borrow_mut()
                .insert(note.id.clone(), note_hash(&note)?);
            self.locations
                .borrow_mut()
                .insert(note.id.clone(), (note.title.clone(), note.notebook.clone()));
            notes.push(note);
        }
        Ok(notes)
    }
    pub fn load_note(&self, id: &str) -> Result<Note, String> {
        self.flush_background()?;
        let entry = self
            .index
            .borrow()
            .entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or("Document is not in the file library")?;
        let md =
            fs::read_to_string(safe_path(&self.root, &entry.path)?).map_err(|e| e.to_string())?;
        let mut note = Self::read_note(&self.root, &entry.path, entry.layout.as_deref(), &md)?;
        if note.id != id {
            return Err("Document identity changed outside Echo".into());
        }
        relocate_assets(&mut note, false);
        self.observe(&entry.path)?;
        if let Some(p) = &entry.layout {
            self.observe(p)?;
        }
        self.logical
            .borrow_mut()
            .insert(note.id.clone(), note_hash(&note)?);
        self.locations
            .borrow_mut()
            .insert(note.id.clone(), (note.title.clone(), note.notebook.clone()));
        Ok(note)
    }
    fn entry(note: &Note) -> Entry {
        let mode = match note.format {
            crate::doc_core::NoteFormat::Document => "Documents",
            crate::doc_core::NoteFormat::Canvas => "Canvas",
        };
        let stem = format!(
            "{}/{}/{}--{}",
            mode,
            file_name(&note.notebook),
            file_name(&note.title),
            file_name(&note.id)
        );
        Entry {
            id: note.id.clone(),
            path: format!("{stem}.md"),
            layout: if note.format == crate::doc_core::NoteFormat::Canvas {
                Some(format!("{stem}.canvas.json"))
            } else {
                None
            },
        }
    }
    fn plan_note(
        &self,
        note: &Note,
        desired: &mut BTreeMap<String, Option<String>>,
    ) -> Result<Entry, String> {
        let mut entry = Self::entry(note);
        let old = self
            .index
            .borrow()
            .entries
            .iter()
            .find(|e| e.id == note.id)
            .cloned();
        if self.locations.borrow().get(&note.id)
            == Some(&(note.title.clone(), note.notebook.clone()))
        {
            if let Some(old) = &old {
                entry = old.clone();
            }
        }
        if self.logical.borrow().get(&note.id) == Some(&note_hash(note)?)
            && old.as_ref().is_some_and(|e| e.path == entry.path)
        {
            return Ok(entry);
        }
        let mut disk = note.clone();
        relocate_assets(&mut disk, true);
        let (mut md, layout) = file_format::encode(&disk)?;
        if let Some(previous) = &old {
            let source = fs::read_to_string(safe_path(&self.root, &previous.path)?)
                .map_err(|e| e.to_string())?;
            md = file_format::preserve_frontmatter(md, &source)?;
        }
        // A codec error must fail before touching any file.
        let read = file_format::decode(&md, layout.as_deref())?;
        if read != disk {
            return Err(format!(
                "Markdown round-trip validation failed for {} at {}",
                note.title,
                difference(
                    &serde_json::to_value(&disk).unwrap(),
                    &serde_json::to_value(&read).unwrap(),
                    "note"
                )
            ));
        }
        desired.insert(entry.path.clone(), Some(md));
        if let Some(path) = &entry.layout {
            desired.insert(path.clone(), layout);
        }
        if let Some(old) = old {
            for p in std::iter::once(old.path).chain(old.layout) {
                if p != entry.path && Some(&p) != entry.layout.as_ref() {
                    desired.insert(p, None);
                }
            }
        }
        Ok(entry)
    }
    pub fn save(&mut self, notes: &[Note], selected: &str) -> Result<(), String> {
        self.save_with_preferences(notes, selected, &[])
    }
    pub fn save_with_preferences(
        &mut self,
        notes: &[Note],
        selected: &str,
        changes: &[(&str, &str)],
    ) -> Result<(), String> {
        self.flush_background()?;
        let mut desired = BTreeMap::new();
        let mut ids = HashSet::new();
        let mut entries = vec![];
        for note in notes {
            if !ids.insert(&note.id) {
                return Err("Duplicate note ID".into());
            }
            entries.push(self.plan_note(note, &mut desired)?);
        }
        for old in &self.index.borrow().entries {
            if !ids.contains(&old.id) {
                desired.insert(old.path.clone(), None);
                if let Some(p) = &old.layout {
                    desired.insert(p.clone(), None);
                }
            }
        }
        let index = Index {
            version: 1,
            entries,
        };
        desired.insert(".echo/index.json".into(), Some(json(&index)?));
        let mut prefs = self.preferences()?;
        prefs.insert("selected".into(), selected.into());
        for (key, value) in changes {
            prefs.insert((*key).into(), (*value).into());
        }
        desired.insert(".echo/settings.json".into(), Some(json(&prefs)?));
        self.commit(desired)?;
        *self.index.borrow_mut() = index;
        *self.locations.borrow_mut() = notes
            .iter()
            .map(|n| (n.id.clone(), (n.title.clone(), n.notebook.clone())))
            .collect();
        *self.logical.borrow_mut() = notes
            .iter()
            .map(|n| Ok((n.id.clone(), note_hash(n)?)))
            .collect::<Result<_, String>>()?;
        Ok(())
    }
    pub fn save_note(&mut self, note: &Note) -> Result<(), String> {
        self.flush_background()?;
        let Some(idx) = self
            .index
            .borrow()
            .entries
            .iter()
            .position(|e| e.id == note.id)
        else {
            return Err(format!("Document {} has not been created", note.id));
        };
        let mut desired = BTreeMap::new();
        let entry = self.plan_note(note, &mut desired)?;
        let mut index = self.index.borrow().clone();
        if index.entries[idx].path != entry.path {
            index.entries[idx] = entry;
            desired.insert(".echo/index.json".into(), Some(json(&index)?));
        }
        self.commit(desired)?;
        *self.index.borrow_mut() = index;
        self.logical
            .borrow_mut()
            .insert(note.id.clone(), note_hash(note)?);
        self.locations
            .borrow_mut()
            .insert(note.id.clone(), (note.title.clone(), note.notebook.clone()));
        Ok(())
    }
    fn preferences(&self) -> Result<BTreeMap<String, String>, String> {
        match read_optional(&self.root.join(".echo/settings.json"))? {
            None => Ok(BTreeMap::new()),
            Some(v) => serde_json::from_slice(&v).map_err(|e| format!("Invalid settings: {e}")),
        }
    }
    pub fn set_preference(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_preferences(&[(key, value)])
    }
    pub fn set_preferences(&self, changes: &[(&str, &str)]) -> Result<(), String> {
        self.flush_background()?;
        let mut prefs = self.preferences()?;
        for (key, value) in changes {
            prefs.insert((*key).into(), (*value).into());
        }
        self.commit(BTreeMap::from([(
            ".echo/settings.json".into(),
            Some(json(&prefs)?),
        )]))
    }
    pub fn preference(&self, key: &str) -> Option<String> {
        self.flush_background()
            .expect("pending library write must finish before reading preferences");
        self.preferences()
            .expect("library settings must be valid JSON")
            .get(key)
            .cloned()
    }
    pub fn selected(&self) -> Option<String> {
        self.preference("selected")
    }
}
pub fn data_root() -> PathBuf {
    if let Some(root) = std::env::var_os("ECHO_DATA_DIR") {
        return PathBuf::from(root);
    }
    crate::platform::data_root()
}

#[derive(Deserialize)]
struct Import {
    notes: Vec<Note>,
    preferences: BTreeMap<String, String>,
}
/// One-time transfer input produced by the migration script. The application has
/// no database driver and never uses the legacy database as a live backend.
pub fn import_library(input: &Path, target: &Path) -> Result<(), String> {
    if target.join(".echo/index.json").exists() {
        return Err("Import target already contains a library".into());
    }
    let mut input: Import = serde_json::from_slice(&fs::read(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for note in &mut input.notes {
        for block in &mut note.blocks {
            if !block.body_html.is_empty() {
                block.body = std::mem::take(&mut block.body_html);
            }
        }
    }
    let mut store = Store::open(target.into())?;
    let selected = input
        .preferences
        .get("selected")
        .map(String::as_str)
        .unwrap_or("");
    store.save(&input.notes, selected)?;
    for (key, value) in &input.preferences {
        store.set_preference(key, value)?;
    }
    if store.load()? != input.notes {
        return Err("Migration verification failed; original library remains intact".into());
    }
    println!(
        "Verified {} notes in Markdown file storage",
        input.notes.len()
    );
    Ok(())
}

#[cfg(test)]
mod file_tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("echo-file-test-{}", crate::identity::uid())))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn note() -> Note {
        file_format::import_markdown("# Note\n\nBody\n", "test-note", "工作", "Note").unwrap()
    }
    #[test]
    fn files_survive_index_loss_and_external_markdown_is_discovered() {
        let temp = Temp::new();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let note = note();
        store.save(&[note.clone()], &note.id).unwrap();
        drop(store);
        fs::remove_file(temp.0.join(".echo/index.json")).unwrap();
        let external = temp.0.join("Documents/工作/External.md");
        fs::write(
            &external,
            "---\ntags: [work, test]\ncustom: value\n---\n# External\n\n*   Keep this bullet\n",
        )
        .unwrap();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let notes = store.load().unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.id == note.id));
        let original = fs::read(&external).unwrap();
        store.save(&notes, &note.id).unwrap();
        assert_eq!(fs::read(&external).unwrap(), original);
        let mut edited = notes
            .iter()
            .find(|n| n.title == "External")
            .unwrap()
            .clone();
        edited.doc.as_mut().unwrap().blocks[0].text = crate::doc_core::RichText::plain("Changed");
        store.save_note(&edited).unwrap();
        let output = fs::read_to_string(&external).unwrap();
        assert!(output.contains("tags: [work, test]\ncustom: value"));
        assert_eq!(store.load_note(&edited.id).unwrap(), edited);
    }
    #[test]
    fn canvas_move_restore_rename_keeps_text_and_geometry() {
        let temp = Temp::new();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let mut note = note();
        note.format = crate::doc_core::NoteFormat::Canvas;
        note.doc = None;
        note.blocks.push(crate::library_data::Block::new(
            crate::library_data::Kind::Text,
            123.5,
            456.,
            333.,
            77.,
            "Card",
            "**body**\n\n第二段",
        ));
        store.save(&[note.clone()], &note.id).unwrap();
        let old = store.index.borrow().entries[0].clone();
        let layout = fs::read_to_string(temp.0.join(old.layout.as_ref().unwrap())).unwrap();
        assert!(!layout.contains("第二段"));
        note.title = "Renamed".into();
        note.notebook = "回收站".into();
        store.save(&[note.clone()], &note.id).unwrap();
        assert!(!temp.0.join(old.path).exists());
        assert!(!temp.0.join(old.layout.unwrap()).exists());
        assert_eq!(store.load().unwrap(), vec![note.clone()]);
        note.notebook = "工作".into();
        store.save(&[note.clone()], &note.id).unwrap();
        drop(store);
        assert_eq!(
            Store::open(temp.0.clone()).unwrap().load().unwrap(),
            vec![note]
        );
    }
    #[test]
    fn reserved_notebook_names_save_and_reopen_with_portable_index_paths() {
        let temp = Temp::new();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let note = file_format::import_markdown("# 中文 笔记\n\nBody\n", "reserved-note", "CON", "中文 笔记").unwrap();
        store.save(&[note.clone()], &note.id).unwrap();
        drop(store);
        let mut reopened = Store::open(temp.0.clone()).unwrap();
        assert_eq!(reopened.load().unwrap(), vec![note]);
        assert!(reopened.index.borrow().entries.iter().all(|e| {
            e.path.starts_with("Documents/_CON/") && !e.path.contains('\\')
        }));
    }
    #[test]
    fn interrupted_multi_file_save_is_completed_on_reopen() {
        let temp = Temp::new();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let note = note();
        store.save(&[note.clone()], &note.id).unwrap();
        let path = store.index.borrow().entries[0].path.clone();
        let original = fs::read_to_string(temp.0.join(&path)).unwrap();
        let modified = original.replace("Body", "Recovered");
        let tx = Transaction {
            version: 1,
            changes: vec![
                Change {
                    path: path.clone(),
                    before: Some(hash(original.as_bytes())),
                    content: Some(modified.clone()),
                },
                Change {
                    path: ".echo/settings.json".into(),
                    before: fingerprint(&temp.0.join(".echo/settings.json")).unwrap(),
                    content: Some("{\"ui_language\":\"en\"}".into()),
                },
            ],
        };
        atomic(
            &temp.0.join(".echo/transaction.json"),
            json(&tx).unwrap().as_bytes(),
        )
        .unwrap();
        atomic(&temp.0.join(&path), modified.as_bytes()).unwrap();
        drop(store);
        let store = Store::open(temp.0.clone()).unwrap();
        assert_eq!(store.preference("ui_language").as_deref(), Some("en"));
        assert!(!temp.0.join(".echo/transaction.json").exists());
        assert_eq!(
            store.load().unwrap()[0].doc.as_ref().unwrap().blocks[0]
                .text
                .text,
            "Recovered"
        );
    }
    #[test]
    fn conflicting_external_edits_are_never_overwritten() {
        let temp = Temp::new();
        let mut store = Store::open(temp.0.clone()).unwrap();
        let mut note = note();
        store.save(&[note.clone()], &note.id).unwrap();
        let path = store.index.borrow().entries[0].path.clone();
        let external = fs::read_to_string(temp.0.join(&path))
            .unwrap()
            .replace("Body", "External");
        fs::write(temp.0.join(&path), &external).unwrap();
        note.doc.as_mut().unwrap().blocks[0].text = crate::doc_core::RichText::plain("Local");
        assert!(store.save_note(&note).is_err());
        assert_eq!(fs::read_to_string(temp.0.join(&path)).unwrap(), external);
        assert!(!temp.0.join(".echo/transaction.json").exists());
    }
}

#[cfg(test)]
mod portable_names {
    #[test]
    fn device_names_are_safe_in_notebook_directories_on_every_platform() {
        for name in ["CON", "nul", "Aux.txt", "COM1", "LPT9.log", "COM¹", "CONIN$"] {
            assert!(super::file_name(name).starts_with('_'), "{name}");
        }
        assert_eq!(super::file_name("  中文:笔记. "), "中文_笔记");
        assert_eq!(super::file_name("COM10"), "COM10");
        assert_eq!(super::file_name(" . "), "Untitled");
    }
}
