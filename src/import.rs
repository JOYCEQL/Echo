//! Import external Markdown as independent, editable notebook pages.
use crate::{
    doc_core::{DocBlock, DocBlockKind},
    model::{Note, State, now, uid},
};
use rune::{FileDialogOptions, Tx};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

const EXTENSIONS: &[&str] = &["md", "markdown"];

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|ext| ext.eq_ignore_ascii_case(e)))
}

pub fn choose_file(s: &mut State, tx: &mut Tx) {
    let title = s.tr("导入 Markdown");
    let options = FileDialogOptions {
        title: title.into(),
        ..Default::default()
    }
    .filter("Markdown", EXTENSIONS);
    let book = s.book.clone();
    tx.open_file(options, move |s: &mut State, answer, tx| match answer {
        Ok(Some(path)) => import_files(s, &book, [path.as_path()], tx),
        Ok(None) => {}
        Err(error) => s.message = Some(format!("{}：{error}", s.tr("无法导入 Markdown"))),
    });
}

pub fn drop_markdown(s: &mut State, paths: &[PathBuf], tx: &mut Tx) {
    let book = s.book.clone();
    import_files(s, &book, paths.iter().map(PathBuf::as_path), tx);
}

pub fn import_files<'a>(
    s: &mut State,
    book: &str,
    paths: impl IntoIterator<Item = &'a Path>,
    tx: &mut Tx,
) {
    if !s.book_order.iter().any(|key| key == book) {
        s.message = Some(s.tr("请先选择一个空间").into());
        return;
    }
    let mut assets = Assets::default();
    let result = (|| {
        let mut notes = Vec::new();
        for path in paths {
            let note = read_page(path, book, &s.store.root, &mut assets)
                .map_err(|error| format!("{}：{error}", path.display()))?;
            notes.push(note);
        }
        if !notes.is_empty() {
            s.insert_imported_notes(notes, book)?;
            s.message = None;
            tx.blur();
            tx.animated();
        }
        Ok::<_, String>(())
    })();
    // Recovery may still apply a journal that already references these files.
    if result.is_ok() || s.store.root.join(".echo/transaction.json").exists() {
        for (_, (file, _)) in assets.files {
            if let Err(error) = file.keep() {
                s.message = Some(format!("{}：{error}", s.tr("无法导入 Markdown")));
            }
        }
    }
    if let Err(error) = result {
        s.message = Some(format!("{}：{error}", s.tr("无法导入 Markdown")));
    }
}

fn read_page(path: &Path, book: &str, root: &Path, assets: &mut Assets) -> Result<Note, String> {
    if !is_markdown(path) {
        return Err("Expected a .md or .markdown file".into());
    }
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let source = source.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let name = path
        .file_stem()
        .ok_or("Missing file name")?
        .to_string_lossy();
    let mut note = crate::file_format::import_markdown(&source, &uid(), book, &name)?;
    let doc = note.doc.as_mut().expect("Markdown imports are documents");
    if doc.blocks.is_empty() {
        let paragraph = doc.new_block(DocBlockKind::Text, "");
        doc.blocks.push(paragraph);
    }
    let directory = path.canonicalize().map_err(|e| e.to_string())?;
    assets.collect(
        &mut doc.blocks,
        directory.parent().ok_or("Missing file directory")?,
        root,
    )?;
    note.preview = doc.preview_text();
    note.updated = now();
    note.stamp = "刚刚".into();
    Ok(note)
}

#[derive(Default)]
struct Assets {
    files: HashMap<PathBuf, (tempfile::TempPath, String)>,
}

impl Assets {
    fn collect(
        &mut self,
        blocks: &mut [DocBlock],
        directory: &Path,
        root: &Path,
    ) -> Result<(), String> {
        for block in blocks {
            if let DocBlockKind::Image { src, .. } = &mut block.kind {
                if let Some(relative) = self.image(src, directory, root)? {
                    *src = relative;
                }
            }
            self.collect(&mut block.children, directory, root)?;
        }
        Ok(())
    }
    fn image(
        &mut self,
        src: &str,
        directory: &Path,
        root: &Path,
    ) -> Result<Option<String>, String> {
        if src.contains(':') || src.starts_with("//") {
            return Ok(None);
        }
        let decoded = percent_encoding::percent_decode_str(src)
            .decode_utf8()
            .map_err(|e| e.to_string())?;
        let path = Path::new(decoded.as_ref());
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !path.is_relative() || !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
            return Ok(None);
        }
        let file = directory
            .join(path)
            .canonicalize()
            .map_err(|e| format!("{src}: {e}"))?;
        if !file.starts_with(directory) || !file.is_file() {
            return Err(format!("Image outside document directory: {src}"));
        }
        let entry = match self.files.entry(file) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let asset =
                    copy_image(entry.key(), root, &ext).map_err(|e| format!("{src}: {e}"))?;
                entry.insert(asset)
            }
        };
        Ok(Some(entry.1.clone()))
    }
}

fn copy_image(source: &Path, root: &Path, ext: &str) -> io::Result<(tempfile::TempPath, String)> {
    let images = crate::storage::safe_path(root, "images").map_err(io::Error::other)?;
    fs::create_dir_all(&images)?;
    let mut temp = tempfile::Builder::new()
        .prefix("import-")
        .suffix(&format!(".{ext}"))
        .tempfile_in(images)?;
    io::copy(&mut fs::File::open(source)?, temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    let relative = format!(
        "images/{}",
        temp.path().file_name().unwrap().to_string_lossy()
    );
    Ok((temp.into_temp_path(), relative))
}
