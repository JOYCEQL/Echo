//! Rune text and clipboard adapters for the structured document model.
use crate::doc_core::*;
use crate::editor_palette::{bg_color_hex, text_color_hex};
impl From<DocAlign> for rune::Align {
    fn from(a: DocAlign) -> Self {
        match a {
            DocAlign::Left => rune::Align::Start,
            DocAlign::Center => rune::Align::Center,
            DocAlign::Right => rune::Align::End,
        }
    }
}
impl RichText {
    pub fn to_rune_spans(
        &self,
        base_size: f32,
        theme: crate::model::Theme,
        accent: u32,
    ) -> Vec<rune::Span> {
        let mut out = Vec::new();
        for (r, m) in self.iter_spans() {
            let mut has_any = false;
            let mut sp = rune::Span::new(r.start, r.end);
            if m.bold {
                sp = sp.bold();
                has_any = true;
            }
            if m.italic {
                sp = sp.italic();
                has_any = true;
            }
            if m.code {
                sp = sp
                    .mono()
                    .size((base_size * 0.9).max(12.0))
                    .color(crate::ui::c(accent))
                    .bg(crate::ui::c(crate::ui::editor_colors(theme).inset), 2.5);
                has_any = true;
            }
            if let Some(hex) = m
                .color
                .as_deref()
                .and_then(|name| text_color_hex(name, theme))
            {
                sp = sp.color(crate::ui::c(hex));
                has_any = true;
            }
            if let Some(hex) = m.bg.as_deref().and_then(|name| bg_color_hex(name, theme)) {
                sp = sp.bg(crate::ui::c(hex), 2.0);
                has_any = true;
            }
            if m.underline || m.link.is_some() {
                let ucolor = m
                    .link
                    .as_ref()
                    .map(|_| crate::ui::c(accent))
                    .unwrap_or_else(|| crate::ui::c(crate::ui::theme_ink(theme)));
                if m.link.is_some() {
                    sp = sp.color(ucolor);
                }
                sp = sp.underline(ucolor);
                has_any = true;
            }
            if has_any {
                out.push(sp);
            }
        }
        out
    }
}
impl DocCore {
    pub fn paste_markdown_or_text(&mut self, raw: &str) -> Result<(), String> {
        let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
        if self.sel.head.block == TITLE_ID
            || self.is_code(self.sel.head.block)
            || !normalized.contains('\n')
        {
            self.paste_text(&normalized);
            return Ok(());
        }
        let mut next = self.doc.next_id;
        let blocks = crate::file_format::parse_standard(&normalized, &mut next)?;
        self.doc.next_id = next;
        self.paste_blocks(blocks);
        Ok(())
    }

    pub fn paste_native(&mut self, edit: &rune::TextEdit) {
        if edit.text.is_empty() {
            return;
        }
        if self.sel.head.block == TITLE_ID || self.is_code(self.sel.head.block) {
            self.paste_text(&edit.text);
            return;
        }
        let mut parsed = Vec::new();
        for paragraph in edit.paragraphs() {
            let mut rich = RichText::plain(&edit.text[paragraph.range.clone()]);
            for mark in edit.marks() {
                let a = mark.start.max(paragraph.range.start);
                let b = mark.end.min(paragraph.range.end);
                if a >= b {
                    continue;
                }
                rich.update_marks(a - paragraph.range.start..b - paragraph.range.start, |m| {
                    match &mark.mark {
                        rune::Mark::Bold => m.bold = true,
                        rune::Mark::Italic => m.italic = true,
                        rune::Mark::Underline => m.underline = true,
                        rune::Mark::Strike => m.strike = true,
                        rune::Mark::Code => m.code = true,
                        rune::Mark::Link(url) => m.link = Some(url.clone()),
                        rune::Mark::Color(rgb) => m.color = Some(format!("#{rgb:06x}")),
                        rune::Mark::Highlight(rgb) => m.bg = Some(format!("#{rgb:06x}")),
                        _ => {}
                    }
                });
            }
            if paragraph.soft_break
                && let Some(previous) = parsed.last_mut()
            {
                let previous: &mut DocBlock = previous;
                previous.text.push("\n", TextMarks::default());
                let at = previous.text.len();
                previous.text.replace(at..at, &rich);
                continue;
            }
            let kind = match paragraph.block {
                rune::Block::Heading(level) => DocBlockKind::Heading { level },
                rune::Block::Bullet(_) => DocBlockKind::Bullet,
                rune::Block::Numbered(_) => DocBlockKind::Ordered { start: None },
                rune::Block::Task(_, done) => DocBlockKind::Todo { done },
                rune::Block::Code => DocBlockKind::Code {
                    lang: "Plain Text".into(),
                    wrap: true,
                },
                _ => DocBlockKind::Text,
            };
            let mut block = self.doc.new_block(kind, "");
            block.text = rich;
            if paragraph.block == rune::Block::Quote {
                block = self.doc.new_container(DocBlockKind::Quote, block);
            }
            parsed.push(block);
        }
        self.paste_blocks(parsed);
    }
    pub fn clipboard_content(&self) -> rune::ClipboardContent {
        let (a, b) = if self.in_block_mode() {
            let ids = self.selected_blocks_ids();
            let (Some(&first), Some(&last)) = (ids.first(), ids.last()) else {
                return rune::ClipboardContent::new("");
            };
            (DocPos::new(first, 0), DocPos::new(last, self.len_of(last)))
        } else {
            self.ordered()
        };
        let content = rune::ClipboardContent::new(self.selected_text());
        if a.block == b.block && self.is_code(a.block) {
            return content;
        }
        let mut html = String::new();
        let mut between = false;
        for unit in self.units() {
            if unit.id == a.block {
                between = true;
            }
            if between
                && !unit.text
                && (a != b || self.in_block_mode())
                && let Some(DocBlock {
                    kind: DocBlockKind::Table(table),
                    ..
                }) = self.block(unit.id)
            {
                html.push_str("<table>");
                for row in &table.rows {
                    html.push_str("<tr>");
                    for cell in row {
                        html.push_str("<td>");
                        html.push_str(&crate::file_format::escape(cell).replace('\n', "<br>"));
                        html.push_str("</td>");
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</table>");
            } else if between && let Some(text) = self.text_of(unit.id) {
                let start = if unit.id == a.block { a.offset } else { 0 };
                let end = if unit.id == b.block {
                    b.offset
                } else {
                    text.len()
                };
                if unit.is_code {
                    html.push_str("<pre><code>");
                    html.push_str(&crate::file_format::escape(&text.text[start..end]));
                    html.push_str("</code></pre>");
                } else {
                    html.push_str("<p>");
                    html.push_str(&crate::file_format::rich_html(text.slice(start..end)));
                    html.push_str("</p>");
                }
            }
            if unit.id == b.block {
                break;
            }
        }
        content.with_html(html)
    }
}
