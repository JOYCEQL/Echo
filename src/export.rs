//! Export decodes every card through the same codec, regardless of viewport or mounted sessions.
use crate::{
    doc_core::{DocBlock, DocBlockKind, DocDocument, NoteFormat, RichText},
    model::{Kind, State},
};
use std::path::Path;

fn copy_images(
    blocks: &mut [DocBlock],
    root: &Path,
    assets: &Path,
    asset_name: &str,
) -> Result<(), String> {
    for block in blocks {
        if let DocBlockKind::Image { src, .. } = &mut block.kind {
            let builtin = match src.as_str() {
                "builtin:architecture" => Some((
                    "architecture.png",
                    include_bytes!("../assets/architecture.png").as_slice(),
                )),
                "builtin:handwritten" => Some((
                    "handwritten.png",
                    include_bytes!("../assets/handwritten.png").as_slice(),
                )),
                _ => None,
            };
            if let Some((name, bytes)) = builtin {
                std::fs::create_dir_all(assets).map_err(|e| e.to_string())?;
                std::fs::write(assets.join(name), bytes).map_err(|e| e.to_string())?;
                *src = format!("{asset_name}/{name}");
            } else if src.starts_with("images/") {
                let file = Path::new(src)
                    .file_name()
                    .ok_or("Invalid image path")?
                    .to_string_lossy()
                    .into_owned();
                std::fs::create_dir_all(assets).map_err(|e| e.to_string())?;
                std::fs::copy(root.join(&*src), assets.join(&file)).map_err(|e| e.to_string())?;
                *src = format!("{asset_name}/{file}");
            }
        }
        copy_images(&mut block.children, root, assets, asset_name)?;
    }
    Ok(())
}

impl State {
    pub fn export_markdown(&mut self, path: &Path) {
        self.finish_edit();
        if self.document_save_failed {
            return;
        }
        let result = self.write_markdown_export(path);
        self.message = Some(match result {
            Ok(()) => format!("已导出 {}", path.display()),
            Err(error) => format!("{}: {error}", self.tr("无法导出 Markdown")),
        });
    }

    fn write_markdown_export(&self, path: &Path) -> Result<(), String> {
        let note = self.note().ok_or("No active note")?;
        let mut doc = match note.format {
            NoteFormat::Document => note.doc.clone().ok_or("Document payload is missing")?,
            NoteFormat::Canvas => {
                let mut doc = DocDocument::empty();
                doc.title = RichText::plain(&note.title);
                doc.blocks.clear();
                let mut cards: Vec<_> = note.blocks.iter().collect();
                cards.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
                for card in cards {
                    if card.is_textual() {
                        doc.blocks
                            .extend(crate::file_format::card_document(card)?.blocks);
                    } else {
                        let kind = match card.kind {
                            Kind::Image | Kind::Handwritten => DocBlockKind::Image {
                                src: card.image.clone(),
                                width: None,
                                caption: card.title.clone(),
                            },
                            Kind::Divider => DocBlockKind::Divider,
                            Kind::Shape => continue, // Canvas decoration has no Markdown content.
                            _ => unreachable!("textual cards handled above"),
                        };
                        let block = doc.new_block(kind, "");
                        doc.blocks.push(block);
                    }
                }
                doc
            }
        };
        let stem = path
            .file_stem()
            .ok_or("Export file name is missing")?
            .to_string_lossy();
        let asset_name = format!("{stem}.assets");
        let assets = path
            .parent()
            .ok_or("Export parent is missing")?
            .join(&asset_name);
        copy_images(&mut doc.blocks, &self.store.root, &assets, &asset_name)?;
        let content = crate::file_format::document_markdown(&doc)?;
        std::fs::write(path, content).map_err(|error| error.to_string())
    }
}
