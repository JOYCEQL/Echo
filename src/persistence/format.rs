//! Portable Markdown is the content authority. Comments only carry block identity
//! and presentation that Markdown cannot express. Canvas geometry lives separately.
use crate::doc_core::*;
use crate::document_color::{bg_color_hex, text_color_hex};
use crate::library_data::{Block, Kind, Note};
use crate::table_data::TableData;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BLOCK: &str = "<!-- echo-block ";
const CARD: &str = "<!-- echo-card ";

#[derive(Serialize, Deserialize)]
struct Header {
    version: u32,
    note: Note,
    next_id: u64,
    page_width: PageWidth,
}
#[derive(Serialize, Deserialize)]
struct BlockMeta {
    depth: usize,
    prefix: String,
    block: Value,
}
#[derive(Serialize, Deserialize)]
pub struct Layout {
    version: u32,
    blocks: Vec<Block>,
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn json_comment<T: Serialize>(prefix: &str, value: &T) -> Result<String, String> {
    // Literal comment delimiters in a title, URL or colour cannot end the metadata.
    let json = serde_json::to_string(value)
        .map_err(|e| e.to_string())?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('-', "\\u002d");
    Ok(format!("{prefix}{json} -->\n"))
}
fn inline_html<'a>(events: impl Iterator<Item = Event<'a>>) -> String {
    let mut html = String::new();
    // The HTML renderer appends a formatting newline after HardBreak. Keep only
    // the semantic break, or importing and re-exporting doubles every soft line.
    let events = events.map(|event| match event {
        Event::HardBreak => Event::InlineHtml("<br />".into()),
        Event::InlineMath(source) => Event::InlineHtml(escape(&format!("${source}$")).into()),
        Event::DisplayMath(source) => Event::InlineHtml(escape(&format!("$${source}$$")).into()),
        other => other,
    });
    pulldown_cmark::html::push_html(&mut html, events);
    html
}
fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_MATH
}
fn inline_text(s: &str) -> String {
    let mut out = String::new();
    for (index, c) in s.char_indices() {
        match c {
            '\n' => out.push_str("<br />"),
            ' ' if index == 0 || index + 1 == s.len() => out.push_str("&#32;"),
            ' ' => out.push(' '),
            '\t' => out.push_str("&#9;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}
pub fn rich_markdown(rich: &RichText) -> String {
    rich_markup(rich.clone(), inline_math_markdown)
}

fn inline_math_markdown(source: &str) -> String {
    let mut out = String::new();
    let mut end = 0;
    for (range, _) in crate::rich_render::inline_ranges(source) {
        out.push_str(&inline_text(&source[end..range.start]));
        out.push_str(&source[range.clone()]);
        end = range.end;
    }
    out.push_str(&inline_text(&source[end..]));
    out
}

pub fn rich_html(mut rich: RichText) -> String {
    for run in &mut rich.runs {
        if let Some(rgb) = run.marks.color.as_deref().and_then(|c| text_color_hex(c)) {
            run.marks.color = Some(format!("#{rgb:06x}"));
        }
        if let Some(rgb) = run.marks.bg.as_deref().and_then(|c| bg_color_hex(c)) {
            run.marks.bg = Some(format!("#{rgb:06x}"));
        }
    }
    rich_markup(rich, |text| escape(text).replace('\n', "<br />"))
}

fn rich_markup(mut rich: RichText, encode_text: fn(&str) -> String) -> String {
    rich.normalize();
    let mut result = String::new();
    for (range, m) in rich.iter_spans() {
        let raw = &rich.text[range];
        let mut text = if m.code {
            format!("<code>{}</code>", escape(raw).replace('\n', "&#10;"))
        } else {
            encode_text(raw)
        };
        for (enabled, tag) in [
            (m.bold, "strong"),
            (m.italic, "em"),
            (m.underline, "u"),
            (m.strike, "del"),
        ] {
            if enabled {
                text = format!("<{tag}>{text}</{tag}>");
            }
        }
        if m.color.is_some() || m.bg.is_some() {
            let mut css = String::new();
            if let Some(c) = &m.color {
                css.push_str(&format!("color:{};", escape(c)));
            }
            if let Some(c) = &m.bg {
                css.push_str(&format!("background-color:{};", escape(c)));
            }
            text = format!("<span style=\"{css}\">{text}</span>");
        }
        if let Some(link) = &m.link {
            text = format!("<a href=\"{}\">{text}</a>", escape(link));
        }
        result.push_str(&text);
    }
    result
}
fn rich_from_md(source: &str) -> Result<RichText, String> {
    rich_from_events(Parser::new_ext(source, options()))
}
fn rich_from_events<'a>(events: impl Iterator<Item = Event<'a>>) -> Result<RichText, String> {
    let html = inline_html(events)
        .replace("<br>", "<br />")
        .replace("<hr>", "<hr />");
    let xml = format!("<root>{html}</root>");
    let parsed = roxmltree::Document::parse(&xml)
        .map_err(|e| format!("Unsupported or malformed inline HTML: {e}"))?;
    fn walk(n: roxmltree::Node<'_, '_>, marks: TextMarks, out: &mut RichText) {
        if n.is_text() {
            out.push(n.text().unwrap(), marks);
            return;
        }
        if !n.is_element() {
            return;
        }
        let tag = n.tag_name().name();
        let mut marks = marks;
        match tag {
            "strong" | "b" => marks.bold = true,
            "em" | "i" => marks.italic = true,
            "u" => marks.underline = true,
            "del" | "s" | "strike" => marks.strike = true,
            "code" => marks.code = true,
            "a" => marks.link = n.attribute("href").map(str::to_string),
            "img" => {
                // Rich text can keep a linked image's caption and enclosing hyperlink.
                if marks.link.is_none() { marks.link = n.attribute("src").map(str::to_string); }
                out.push(n.attribute("alt").unwrap_or(""), marks);
                return;
            }
            "br" => {
                out.push("\n", marks);
                return;
            }
            _ => {}
        }
        if let Some(style) = n.attribute("style") {
            for rule in style.split(';') {
                if let Some((key, value)) = rule.split_once(':') {
                    match key.trim() {
                        "color" => marks.color = Some(value.trim().to_string()),
                        "background-color" => marks.bg = Some(value.trim().to_string()),
                        _ => {}
                    }
                }
            }
        }
        let mut paragraph_seen = false;
        for child in n.children() {
            if tag == "root" && child.is_text() && child.text().unwrap().trim().is_empty() {
                continue;
            }
            if tag == "root" && child.has_tag_name("p") {
                if paragraph_seen {
                    out.push("\n", TextMarks::default());
                }
                paragraph_seen = true;
            }
            walk(child, marks.clone(), out);
        }
    }
    let mut rich = RichText::default();
    walk(parsed.root_element(), TextMarks::default(), &mut rich);
    rich.normalize();
    Ok(rich)
}
fn fence(body: &str) -> String {
    let n = body
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(2)
        + 1;
    "`".repeat(n)
}
fn code_md(lang: &str, body: &str) -> String {
    let fence = fence(body);
    format!("{fence}{lang}\n{body}\n{fence}")
}
fn table_md(t: &TableData) -> String {
    if t.rows.is_empty() {
        return String::new();
    }
    // Cell content is itself Markdown; protect literal pipes, slashes and newlines.
    let line = |r: &[String]| {
        format!(
            "| {} |",
            r.iter()
                .map(|c| c
                    .replace('\\', "&#92;")
                    .replace('|', "&#124;")
                    .replace('\n', "<br />"))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    let mut lines = vec![
        line(&t.rows[0]),
        format!(
            "| {} |",
            (0..t.cols())
                .map(|i| match t.align.get(i) {
                    Some(1) => ":---:",
                    Some(2) => "---:",
                    _ => "---",
                })
                .collect::<Vec<_>>()
                .join(" | ")
        ),
    ];
    lines.extend(t.rows.iter().skip(1).map(|r| line(r)));
    lines.join("\n")
}
fn read_table(source: &str) -> Result<TableData, String> {
    let parsed = crate::table_markdown::parse(source);
    let Some((_, mut table)) = parsed.into_iter().next() else {
        return Err("Invalid Markdown table".into());
    };
    for row in &mut table.rows {
        for cell in row {
            *cell = cell
                .replace("<br />", "\n")
                .replace("<br>", "\n")
                .replace("&#124;", "|")
                .replace("&#92;", "\\");
        }
    }
    Ok(table)
}
fn block_body(b: &DocBlock) -> String {
    let text = rich_markdown(&b.text);
    match &b.kind {
        DocBlockKind::Text => {
            if text.is_empty() {
                "<!-- echo-empty -->".into()
            } else {
                text
            }
        }
        DocBlockKind::Heading { level } => format!("{} {text}", "#".repeat(*level as usize)),
        DocBlockKind::Bullet => format!("- {text}"),
        DocBlockKind::Ordered { start } => format!("{}. {text}", start.unwrap_or(1)),
        DocBlockKind::Todo { done } => format!("- [{}] {text}", if *done { "x" } else { " " }),
        DocBlockKind::Quote => format!("> {text}"),
        DocBlockKind::Callout { palette, .. } => format!(
            "> [!{}]\n> {text}",
            match palette.as_str() {
                "blue" => "NOTE",
                "green" => "TIP",
                "red" => "CAUTION",
                "purple" => "IMPORTANT",
                _ => "WARNING",
            }
        ),
        DocBlockKind::Code { lang, .. } => {
            code_md(if lang == "Plain Text" { "" } else { lang }, &b.text.text)
        }
        DocBlockKind::Divider => "---".into(),
        DocBlockKind::Image { src, caption, .. } => {
            format!("![{}](<{}>)", inline_text(caption), src.replace('>', "%3E"))
        }
        DocBlockKind::Table(table) => table_md(table),
    }
}
fn write_blocks(
    blocks: &[DocBlock],
    depth: usize,
    prefix: &str,
    out: &mut String,
) -> Result<(), String> {
    for b in blocks {
        let mut meta = serde_json::to_value(b).map_err(|e| e.to_string())?;
        meta["text"] = serde_json::json!({"text":"","runs":[]});
        meta["children"] = serde_json::json!([]);
        if let DocBlockKind::Table(_) = &b.kind {
            meta["kind"]["Table"]["rows"] = serde_json::json!([]);
        }
        // Caption and URL remain in Markdown, not duplicated in block metadata.
        if let DocBlockKind::Image { .. } = &b.kind {
            meta["kind"]["Image"]["src"] = Value::String(String::new());
            meta["kind"]["Image"]["caption"] = Value::String(String::new());
        }
        out.push_str(prefix);
        out.push_str(&json_comment(
            BLOCK,
            &BlockMeta {
                depth,
                prefix: prefix.into(),
                block: meta,
            },
        )?);
        out.push('\n');
        for line in block_body(b).split('\n') {
            out.push_str(prefix);
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        let child_prefix = match b.kind {
            DocBlockKind::Quote | DocBlockKind::Callout { .. } => format!("{prefix}> "),
            _ => format!("{prefix}    "),
        };
        write_blocks(&b.children, depth + 1, &child_prefix, out)?;
    }
    Ok(())
}

/// Strip text from layout data: no Markdown body is mirrored in JSON.
pub fn encode(note: &Note) -> Result<(String, Option<String>), String> {
    let mut meta = note.clone();
    if note.format == NoteFormat::Canvas {
        meta.blocks.clear();
    } else if note.blocks.iter().any(|b| {
        !b.body.is_empty()
            || !b.body_html.is_empty()
            || !b.title.is_empty()
            || !b.image.is_empty()
            || b.table.is_some()
    }) {
        return Err("A document contains legacy canvas content; migrate that content explicitly before saving".into());
    }
    meta.doc = None;
    let doc = note.doc.as_ref();
    let header = Header {
        version: 1,
        note: meta,
        next_id: doc.map_or(1, |d| d.next_id),
        page_width: doc.map_or(PageWidth::Standard, |d| d.page_width),
    };
    let mut md = format!(
        "---\necho: {}\n---\n\n",
        serde_json::to_string(&header).map_err(|e| e.to_string())?
    );
    match note.format {
        NoteFormat::Document => {
            let doc = doc.ok_or("Document note has no document")?;
            md.push_str(&format!("# {}\n\n", rich_markdown(&doc.title)));
            write_blocks(&doc.blocks, 0, "", &mut md)?;
            Ok((md, None))
        }
        NoteFormat::Canvas => {
            md.push_str(&format!("# {}\n\n", inline_text(&note.title)));
            let mut layout = Layout {
                version: 1,
                blocks: vec![],
            };
            for b in &note.blocks {
                let mut geometry = b.clone();
                geometry.title.clear();
                geometry.body.clear();
                geometry.body_html.clear();
                geometry.image.clear();
                if let Some(t) = &mut geometry.table {
                    t.rows.clear();
                }
                layout.blocks.push(geometry);
                md.push_str(&json_comment(CARD, &b.id)?);
                md.push_str(&format!(
                    "\n## {}\n\n<!-- echo-body -->\n",
                    inline_text(&b.title)
                ));
                let body = match b.kind {
                    Kind::Code => code_md(&b.title, &b.body),
                    _ => b.body.clone(),
                };
                md.push_str(&body);
                md.push_str("\n<!-- /echo-body -->\n");
                if !b.image.is_empty() {
                    md.push_str(&format!("\n![image](<{}>)\n", b.image));
                }
                if let Some(t) = &b.table {
                    md.push_str("\n<!-- echo-table -->\n");
                    md.push_str(&table_md(t));
                    md.push_str("\n<!-- /echo-table -->\n");
                }
                // Historical HTML only exists in pre-v3 libraries, migrated before encoding.
                if !b.body_html.is_empty() {
                    return Err("Legacy HTML must be normalized before Markdown migration".into());
                }
                md.push('\n');
            }
            Ok((
                md,
                Some(serde_json::to_string_pretty(&layout).map_err(|e| e.to_string())?),
            ))
        }
    }
}
fn between<'a>(s: &'a str, start: &str, end: &str) -> Result<&'a str, String> {
    let (_, rest) = s
        .split_once(start)
        .ok_or_else(|| format!("Missing {start}"))?;
    let (value, _) = rest
        .rsplit_once(end)
        .ok_or_else(|| format!("Missing {end}"))?;
    Ok(value)
}
fn read_code(s: &str) -> Result<(String, String), String> {
    let mut lang = String::new();
    let mut body = String::new();
    let mut found = false;
    for event in Parser::new_ext(s, options()) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                found = true;
                if let pulldown_cmark::CodeBlockKind::Fenced(info) = kind {
                    lang = info.to_string();
                }
            }
            Event::Text(t) if found => body.push_str(&t),
            _ => {}
        }
    }
    if !found {
        return Err("Expected a fenced code block".into());
    }
    if body.ends_with('\n') {
        body.pop();
    }
    Ok((lang, body))
}
fn read_image(s: &str) -> Result<(String, String), String> {
    let mut src = None;
    let mut caption = String::new();
    let mut inside = false;
    for event in Parser::new_ext(s, options()) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                src = Some(dest_url.to_string());
                inside = true;
            }
            Event::End(TagEnd::Image) => inside = false,
            Event::Text(t) | Event::Code(t) if inside => caption.push_str(&t),
            _ => {}
        }
    }
    Ok((src.ok_or("Expected a Markdown image")?, caption))
}
fn normalize_block_source(source: &str, meta: &BlockMeta) -> Result<String, String> {
    let source = source
        .trim_matches('\n')
        .lines()
        .map(|line| {
            if line.is_empty() {
                Ok("")
            } else {
                line.strip_prefix(&meta.prefix).ok_or_else(|| {
                    "Block indentation changed; keep the Echo metadata line with its block"
                        .to_string()
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    Ok(source.trim_end_matches('\n').to_string())
}
fn primary_end(source: &str) -> usize {
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        if matches!(event, Event::Start(_) | Event::Rule | Event::Html(_)) {
            return range.end;
        }
    }
    source.len()
}
fn decode_block(source: &str, meta: &BlockMeta) -> Result<DocBlock, String> {
    let mut b: DocBlock = serde_json::from_value(meta.block.clone()).map_err(|e| e.to_string())?;
    let normalized = normalize_block_source(source, meta)?;
    let source = &normalized[..primary_end(&normalized)];
    let source = source.trim_end_matches('\n');
    // Markdown syntax, not the saved presentation metadata, determines edits
    // made in another editor (for example changing a paragraph into a heading).
    let expected = match b.kind {
        DocBlockKind::Text => "text",
        DocBlockKind::Heading { .. } => "heading",
        DocBlockKind::Bullet => "bullet",
        DocBlockKind::Ordered { .. } => "ordered",
        DocBlockKind::Todo { .. } => "todo",
        DocBlockKind::Quote => "quote",
        DocBlockKind::Callout { .. } => "callout",
        DocBlockKind::Code { .. } => "code",
        DocBlockKind::Divider => "divider",
        DocBlockKind::Image { .. } => "image",
        DocBlockKind::Table(_) => "table",
    };
    let actual = Parser::new_ext(source, options())
        .find_map(|event| match event {
            Event::Start(Tag::Heading { .. }) => Some("heading"),
            Event::Start(Tag::CodeBlock(_)) => Some("code"),
            Event::Start(Tag::Table(_)) => Some("table"),
            Event::Rule => Some("divider"),
            Event::Start(Tag::List(start)) => Some(if source.starts_with("- [") {
                "todo"
            } else if start.is_some() {
                "ordered"
            } else {
                "bullet"
            }),
            Event::Start(Tag::BlockQuote(_)) => Some(if source.starts_with("> [!") {
                "callout"
            } else {
                "quote"
            }),
            Event::Start(Tag::Paragraph) => Some(if source.trim_start().starts_with("![") {
                "image"
            } else {
                "text"
            }),
            _ => None,
        })
        .unwrap_or("text");
    if actual != expected {
        let mut next = b.id + 1;
        let mut parsed = parse_standard(source, &mut next)?;
        if parsed.len() != 1 {
            return Err("Changed block must have one Markdown block; insert additional paragraphs after a blank line".into());
        }
        let mut changed = parsed.remove(0);
        changed.id = b.id;
        changed.align = b.align;
        changed.bg = b.bg;
        changed.folded = b.folded;
        return Ok(changed);
    }
    match &mut b.kind {
        DocBlockKind::Code { lang, .. } => {
            let (l, text) = read_code(source)?;
            *lang = if l.is_empty() { "Plain Text".into() } else { l };
            b.text = RichText::plain(text);
        }
        DocBlockKind::Table(t) => {
            if source.trim().is_empty() {
                t.rows.clear();
            } else {
                let parsed = read_table(source)?;
                t.rows = parsed.rows;
            }
        }
        DocBlockKind::Image { src, caption, .. } => {
            let (s, c) = read_image(source)?;
            *src = s;
            *caption = c;
        }
        DocBlockKind::Divider => {}
        DocBlockKind::Quote => {
            b.text = rich_from_md(source.strip_prefix("> ").unwrap_or(source))?;
        }
        DocBlockKind::Callout { .. } => {
            let (_, text) = source.split_once('\n').ok_or("Malformed callout")?;
            b.text = rich_from_md(text.strip_prefix("> ").unwrap_or(text))?;
        }
        _ => {
            let text = match &mut b.kind {
                DocBlockKind::Heading { level } => {
                    let (mark, text) = source.split_once(' ').ok_or("Malformed heading")?;
                    if !mark.chars().all(|c| c == '#') {
                        return Err("Heading Markdown syntax is missing".into());
                    }
                    *level = mark.len() as u8;
                    text
                }
                DocBlockKind::Bullet => source
                    .strip_prefix("- ")
                    .or_else(|| source.strip_prefix("* "))
                    .ok_or("Malformed bullet")?,
                DocBlockKind::Ordered { start } => {
                    let (number, text) = source.split_once(". ").ok_or("Malformed ordered list")?;
                    let n = number.parse::<u32>().map_err(|e| e.to_string())?;
                    if start.is_some() || n != 1 {
                        *start = Some(n);
                    }
                    text
                }
                DocBlockKind::Todo { done } => {
                    if let Some(t) = source
                        .strip_prefix("- [x] ")
                        .or_else(|| source.strip_prefix("- [X] "))
                    {
                        *done = true;
                        t
                    } else {
                        *done = false;
                        source.strip_prefix("- [ ] ").ok_or("Malformed task")?
                    }
                }
                _ => {
                    if source == "<!-- echo-empty -->" {
                        ""
                    } else {
                        source
                    }
                }
            };
            b.text = rich_from_md(text)?;
        }
    }
    Ok(b)
}
fn sections<'a>(source: &'a str, marker: &str) -> Result<Vec<(&'a str, &'a str)>, String> {
    // Use the Markdown parser's code ranges, so literal metadata examples inside
    // fenced code never become document boundaries.
    let code: Vec<_> = Parser::new_ext(source, options())
        .into_offset_iter()
        .filter_map(|(e, r)| {
            if matches!(
                e,
                Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(_)))
            ) {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    let mut offsets = vec![];
    let mut pos = 0;
    for line in source.split_inclusive('\n') {
        let clean = line.trim_start_matches([' ', '>']);
        if let Some(json) = clean
            .strip_prefix(marker)
            .and_then(|s| s.trim_end().strip_suffix(" -->"))
        {
            // Nested indented blocks are generated with metadata. Only fenced code
            // suppresses a marker; indentation is the block tree's presentation.
            let in_fence = code.iter().any(|r| r.contains(&pos));
            if !in_fence {
                offsets.push((pos, pos + line.len(), json));
            }
        }
        pos += line.len();
    }
    Ok(offsets
        .iter()
        .enumerate()
        .map(|(i, (_, start, json))| {
            (
                *json,
                &source[*start..offsets.get(i + 1).map_or(source.len(), |x| x.0)],
            )
        })
        .collect())
}
fn append_at(blocks: &mut Vec<DocBlock>, depth: usize, block: DocBlock) -> Result<(), String> {
    if depth == 0 {
        blocks.push(block);
        return Ok(());
    }
    let parent = blocks.last_mut().ok_or("Invalid block hierarchy")?;
    append_at(&mut parent.children, depth - 1, block)
}
pub fn decode(md: &str, layout: Option<&str>) -> Result<Note, String> {
    let (frontmatter, body) = frontmatter(md)?;
    let json = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("echo: "))
        .ok_or("Missing Echo frontmatter")?;
    let body = body.trim_start_matches('\n');
    let h: Header = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if h.version != 1 {
        return Err(format!("Unsupported library file version {}", h.version));
    }
    let mut note = h.note;
    let (title, body) = body
        .strip_prefix("# ")
        .ok_or("Missing page title")?
        .split_once('\n')
        .ok_or("Missing title newline")?;
    let title = rich_from_md(title)?;
    match note.format {
        NoteFormat::Document => {
            let mut doc = DocDocument {
                next_id: h.next_id,
                title,
                blocks: vec![],
                page_width: h.page_width,
            };
            if !doc.title.text.trim().is_empty() {
                note.title = doc.title.text.trim().to_string();
            }
            read_document_body(body, &mut doc)?;
            note.doc = Some(doc);
        }
        NoteFormat::Canvas => {
            note.title = title.text;
            let layout: Layout =
                serde_json::from_str(layout.ok_or("Canvas layout file is missing")?)
                    .map_err(|e| e.to_string())?;
            if layout.version != 1 {
                return Err("Unsupported canvas layout version".into());
            }
            let parts = sections(body, CARD)?;
            if parts.len() != layout.blocks.len() {
                return Err("Canvas cards and layout do not match".into());
            }
            for ((json, source), mut b) in parts.into_iter().zip(layout.blocks) {
                let id: String = serde_json::from_str(json).map_err(|e| e.to_string())?;
                if id != b.id {
                    return Err("Canvas card identity does not match its layout".into());
                }
                let title = source
                    .trim_start_matches('\n')
                    .strip_prefix("## ")
                    .ok_or("Missing card title")?
                    .split_once('\n')
                    .ok_or("Missing card title newline")?
                    .0;
                b.title = rich_from_md(title)?.text;
                let raw = between(source, "<!-- echo-body -->\n", "\n<!-- /echo-body -->")?;
                b.body = if b.kind == Kind::Code {
                    read_code(raw)?.1
                } else {
                    raw.into()
                };
                let tail = source.rsplit_once("<!-- /echo-body -->").unwrap().1;
                if matches!(b.kind, Kind::Image | Kind::Handwritten) && tail.contains("![image]") {
                    b.image = read_image(tail)?.0;
                }
                if let Some(t) = &mut b.table {
                    let raw = between(tail, "<!-- echo-table -->\n", "\n<!-- /echo-table -->")?;
                    t.rows = if raw.is_empty() {
                        vec![]
                    } else {
                        read_table(raw)?.rows
                    };
                }
                note.blocks.push(b);
            }
        }
    }
    Ok(note)
}

/// Ordinary Markdown files are accepted without Echo metadata. The content is
/// parsed into the native document model, never retained as a private JSON body.
pub fn parse_standard(source: &str, next_id: &mut u64) -> Result<Vec<DocBlock>, String> {
    let events: Vec<_> = Parser::new_ext(source, options())
        .into_offset_iter()
        .collect();
    fn parse(
        events: &[(Event<'_>, std::ops::Range<usize>)],
        source: &str,
        next: &mut u64,
    ) -> Result<Vec<DocBlock>, String> {
        let mut out = vec![];
        let mut i = 0;
        while i < events.len() {
            let (event, range) = &events[i];
            // Use the resolved inline events from the complete file, including link definitions.
            // An inline picture becomes an editable image block between its surrounding text.
            if matches!(event, Event::Start(Tag::Paragraph | Tag::Heading { .. })) {
                let stop = events[i + 1..].iter().position(|(e, _)| matches!(e, Event::End(TagEnd::Paragraph | TagEnd::Heading(_))))
                    .ok_or("Unclosed Markdown paragraph")? + i + 1;
                let inline = &events[i + 1..stop];
                let mut push = |kind, text| {
                    let id = *next;
                    *next += 1;
                    out.push(DocBlock { id, kind, align: DocAlign::Left, bg: None, folded: false, text, children: vec![] });
                };
                let kind = match event {
                    Event::Start(Tag::Heading { level, .. }) => DocBlockKind::Heading { level: *level as u8 },
                    _ => DocBlockKind::Text,
                };
                if let [(Event::DisplayMath(formula), _)] = inline {
                    push(DocBlockKind::Code { lang: "math".into(), wrap: true }, RichText::plain(formula.trim_matches('\n')));
                } else {
                    let mut start = 0;
                    let mut depth = 0;
                    let mut at = 0;
                    while at < inline.len() {
                        if let Event::Start(Tag::Image { dest_url, .. }) = &inline[at].0 {
                            if depth == 0 {
                                if start < at {
                                    push(kind.clone(), rich_from_events(inline[start..at].iter().map(|(e, _)| e.clone()))?);
                                }
                                let end = inline[at + 1..].iter().position(|(e, _)| matches!(e, Event::End(TagEnd::Image)))
                                    .ok_or("Unclosed Markdown image")? + at + 1;
                                let caption = rich_from_events(inline[at + 1..end].iter().map(|(e, _)| e.clone()))?.text;
                                push(DocBlockKind::Image {src: dest_url.to_string(), caption, width: None}, RichText::default());
                                at = end + 1;
                                start = at;
                                continue;
                            }
                        }
                        match &inline[at].0 { Event::Start(_) => depth += 1, Event::End(_) => depth -= 1, _ => {} }
                        at += 1;
                    }
                    if start < inline.len() || inline.is_empty() {
                        push(kind, rich_from_events(inline[start..].iter().map(|(e, _)| e.clone()))?);
                    }
                }
                i = stop + 1;
                continue;
            }
            let kind = match event {
                Event::DisplayMath(_) => Some(DocBlockKind::Code { lang: "math".into(), wrap: true }),
                Event::Start(Tag::CodeBlock(_)) => Some(DocBlockKind::Code {
                    lang: "Plain Text".into(),
                    wrap: true,
                }),
                Event::Start(Tag::Table(_)) => Some(DocBlockKind::Table(TableData::default())),
                Event::Rule => Some(DocBlockKind::Divider),
                _ => None,
            };
            if let Some(mut kind) = kind {
                let raw = &source[range.clone()];
                let mut text = RichText::default();
                match &mut kind {
                    DocBlockKind::Code { lang, .. } => {
                        if let Event::DisplayMath(formula) = event {
                            text = RichText::plain(formula.trim_matches('\n'));
                        } else {
                            let (language, body) = read_code(raw)?;
                            if !language.is_empty() { *lang = language; }
                            text = RichText::plain(body);
                        }
                    }
                    DocBlockKind::Table(t) => *t = read_table(raw)?,
                    DocBlockKind::Divider => {}
                    _ => unreachable!(),
                }
                let id = *next;
                *next += 1;
                out.push(DocBlock {
                    id,
                    kind,
                    align: DocAlign::Left,
                    bg: None,
                    folded: false,
                    text,
                    children: vec![],
                });
                if matches!(event, Event::Start(_)) {
                    let mut depth = 1;
                    i += 1;
                    while i < events.len() && depth > 0 {
                        match events[i].0 {
                            Event::Start(_) => depth += 1,
                            Event::End(_) => depth -= 1,
                            _ => {}
                        }
                        i += 1;
                    }
                    continue;
                }
            } else if let Event::Start(Tag::List(start)) = event {
                let mut end = i + 1;
                let mut depth = 1;
                while end < events.len() && depth > 0 {
                    match events[end].0 {
                        Event::Start(Tag::List(_)) => depth += 1,
                        Event::End(TagEnd::List(_)) => depth -= 1,
                        _ => {}
                    }
                    end += 1;
                }
                let mut j = i + 1;
                let mut number = *start;
                while j + 1 < end {
                    if matches!(events[j].0, Event::Start(Tag::Item)) {
                        let mut k = j + 1;
                        let mut depth = 1;
                        while k < end && depth > 0 {
                            match events[k].0 {
                                Event::Start(Tag::Item) => depth += 1,
                                Event::End(TagEnd::Item) => depth -= 1,
                                _ => {}
                            }
                            k += 1;
                        }
                        let raw = &source[events[j].1.clone()];
                        let mut lines = raw.lines();
                        let first = lines.next().unwrap_or("").trim_start();
                        let marker = first.find(char::is_whitespace).unwrap_or(first.len());
                        let after = &first[marker..];
                        let indent = marker + after.len() - after.trim_start().len();
                        let mut content = after.trim_start();
                        let kind = if let Some(tail) = content.strip_prefix("[x] ").or_else(|| content.strip_prefix("[X] ")) {
                            content = tail;
                            DocBlockKind::Todo { done: true }
                        } else if let Some(tail) = content.strip_prefix("[ ] ") {
                            content = tail;
                            DocBlockKind::Todo { done: false }
                        } else if let Some(n) = number {
                            number = Some(n + 1);
                            DocBlockKind::Ordered { start: Some(n as u32) }
                        } else { DocBlockKind::Bullet };
                        let mut body = content.to_string();
                        for line in lines {
                            let spaces = line.len() - line.trim_start_matches(' ').len();
                            body.push('\n');
                            body.push_str(&line[spaces.min(indent)..]);
                        }
                        let id = *next;
                        *next += 1;
                        let mut children = parse_standard(&body, next)?;
                        let text = if children.first().is_some_and(|b| matches!(b.kind, DocBlockKind::Text)) {
                            children.remove(0).text
                        } else { RichText::default() };
                        let block = DocBlock { id, kind, align: DocAlign::Left, bg: None, folded: false, text, children };
                        out.push(block);
                        j = k;
                    } else {
                        j += 1;
                    }
                }
                i = end;
                continue;
            } else if matches!(event, Event::Start(Tag::BlockQuote(_))) {
                let raw = &source[range.clone()];
                let stripped = raw
                    .lines()
                    .map(|l| {
                        l.strip_prefix("> ")
                            .or_else(|| l.strip_prefix('>'))
                            .unwrap_or(l)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let id = *next;
                *next += 1;
                let children = parse_standard(&stripped, next)?;
                out.push(DocBlock {
                    id,
                    kind: DocBlockKind::Quote,
                    align: DocAlign::Left,
                    bg: None,
                    folded: false,
                    text: RichText::default(),
                    children,
                });
                let mut depth = 1;
                i += 1;
                while i < events.len() && depth > 0 {
                    match events[i].0 {
                        Event::Start(Tag::BlockQuote(_)) => depth += 1,
                        Event::End(TagEnd::BlockQuote(_)) => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                continue;
            } else if let Event::Html(h) = event {
                if !h.trim().starts_with("<!--") {
                    return Err("Unsupported block HTML; source file has not been modified".into());
                }
            }
            i += 1;
        }
        Ok(out)
    }
    parse(&events, source, next_id)
}

fn frontmatter(md: &str) -> Result<(&str, &str), String> {
    if let Some(rest) = md.strip_prefix("---\n") {
        let (metadata, body) = rest
            .split_once("\n---\n")
            .ok_or("Unclosed YAML frontmatter; original file has not been changed")?;
        Ok((metadata, body))
    } else {
        Ok(("", md))
    }
}
pub fn preserve_frontmatter(encoded: String, previous: &str) -> Result<String, String> {
    let (old, _) = frontmatter(previous)?;
    let extra = old
        .lines()
        .filter(|line| !line.starts_with("echo: "))
        .collect::<Vec<_>>()
        .join("\n");
    if extra.is_empty() {
        return Ok(encoded);
    }
    let (metadata, body) = frontmatter(&encoded)?;
    Ok(format!("---\n{extra}\n{metadata}\n---\n{body}"))
}
pub fn is_echo_file(md: &str) -> bool {
    md.starts_with("---\n")
        && md
            .split("\n---\n")
            .next()
            .is_some_and(|fm| fm.lines().any(|line| line.starts_with("echo: ")))
}
pub fn import_markdown(md: &str, id: &str, book: &str, name: &str) -> Result<Note, String> {
    // A leading horizontal rule is valid Markdown too. Strip only a closed header.
    let md = md.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n"))
        .map_or(md, |(_, body)| body);
    let mut next = 1;
    let mut blocks = parse_standard(md, &mut next)?;
    let title = if blocks
        .first()
        .is_some_and(|b| matches!(b.kind, DocBlockKind::Heading { level: 1 }) && !b.text.text.trim().is_empty())
    {
        blocks.remove(0).text
    } else {
        RichText::plain(name)
    };
    Ok(Note {
        id: id.into(),
        notebook: book.into(),
        original_book: book.into(),
        parent_id: None,
        title: title.text.clone(),
        preview: String::new(),
        stamp: String::new(),
        updated: 0,
        blocks: vec![],
        zoom: 1.,
        pan: [0., 0.],
        editor_version: 6,
        format: NoteFormat::Document,
        doc: Some(DocDocument {
            next_id: next,
            title,
            blocks,
            page_width: PageWidth::Standard,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(doc: DocDocument) -> Note {
        Note {
            id: "roundtrip".into(),
            notebook: "工作".into(),
            original_book: "工作".into(),
            parent_id: None,
            title: doc.title.text.clone(),
            preview: "".into(),
            stamp: "".into(),
            updated: 1,
            blocks: vec![],
            zoom: 1.,
            pan: [0., 0.],
            editor_version: 6,
            format: NoteFormat::Document,
            doc: Some(doc),
        }
    }
    #[test]
    fn markdown_preserves_all_document_content_and_presentation() {
        let mut d = DocDocument::empty();
        d.title = RichText::plain("中文 & English <Title>");
        d.blocks.clear();
        d.page_width = PageWidth::Wide;
        let mut text = d.new_block(DocBlockKind::Text, "");
        for marks in [
            TextMarks::default(),
            TextMarks {
                bold: true,
                italic: true,
                underline: true,
                strike: true,
                code: false,
                color: Some("#aa0011".into()),
                bg: Some("#ffff00".into()),
                link: Some("https://example.com/?a=1&b=2".into()),
            },
            TextMarks {
                code: true,
                ..Default::default()
            },
        ] {
            text.text
                .push(" 你好 ** * _ & < > \\ ` 🚀\n第二行 \t ", marks);
        }
        text.align = DocAlign::Center;
        text.bg = Some("#ffffff".into());
        text.folded = true;
        text.children
            .push(d.new_block(DocBlockKind::Todo { done: true }, "nested task"));
        d.blocks.push(text);
        let mut quote = d.new_block(DocBlockKind::Quote, "");
        let mut call = d.new_block(
            DocBlockKind::Callout {
                emoji: "✨".into(),
                palette: "purple".into(),
            },
            "callout text",
        );
        call.children
            .push(d.new_block(DocBlockKind::Bullet, "child"));
        quote.children.push(call);
        d.blocks.push(quote);
        for kind in [
            DocBlockKind::Heading { level: 3 },
            DocBlockKind::Ordered { start: Some(9) },
            DocBlockKind::Todo { done: false },
            DocBlockKind::Text,
        ] {
            let b = d.new_block(kind, "");
            d.blocks.push(b);
        }
        let code = d.new_block(
            DocBlockKind::Code {
                lang: "rust".into(),
                wrap: false,
            },
            "let x = \"```\";\n<!-- echo-block {\"fake\":true} -->\n",
        );
        d.blocks.push(code);
        let image = d.new_block(
            DocBlockKind::Image {
                src: "images/a b.png".into(),
                width: Some(345.),
                caption: "a & b 中文".into(),
            },
            "",
        );
        d.blocks.push(image);
        let table = d.new_block(
            DocBlockKind::Table(TableData {
                rows: vec![
                    vec!["一|二".into(), "**bold**".into()],
                    vec!["a\\b\nc".into(), "".into()],
                ],
                widths: vec![2.5, 1.],
                align: vec![1, 2],
            }),
            "",
        );
        d.blocks.push(table);
        let original = note(d);
        let (md, layout) = encode(&original).unwrap();
        assert_eq!(decode(&md, layout.as_deref()).unwrap(), original);
        assert!(md.contains("```"));
        assert!(md.contains("- [x] nested task"));
        assert!(
            !md.contains("\"runs\":[{\"len\""),
            "rich text must be encoded in the Markdown, not hidden JSON"
        );
    }
    #[test]
    fn external_plain_markdown_parses_lists_tables_and_rich_text() {
        let n=import_markdown("# My note\n\n**Bold** and *italic*\n\n- [x] Done\n- Next\n    - Child\n\n> Quote\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n", "external", "工作", "Other").unwrap();
        let d = n.doc.as_ref().unwrap();
        assert_eq!(d.title.text, "My note");
        assert!(
            d.blocks
                .iter()
                .any(|b| matches!(b.kind, DocBlockKind::Todo { done: true }))
        );
        assert!(d.blocks.iter().any(|b| !b.children.is_empty()));
        assert!(
            d.blocks
                .iter()
                .any(|b| matches!(b.kind, DocBlockKind::Table(_)))
        );
        let (md, layout) = encode(&n).unwrap();
        assert_eq!(decode(&md, layout.as_deref()).unwrap(), n);
    }
    #[test]
    fn external_paragraphs_before_and_after_annotated_blocks_survive() {
        let mut d = DocDocument::empty();
        d.title = RichText::plain("Title");
        d.blocks.clear();
        let b = d.new_block(
            DocBlockKind::Code {
                lang: "rust".into(),
                wrap: true,
            },
            "fn main() {}
",
        );
        d.blocks.push(b);
        let n = note(d);
        let (md, _) = encode(&n).unwrap();
        let md = md.replacen("# Title\n\n", "# Title\n\nAdded before\n\n", 1) + "\nAdded after\n";
        let parsed = decode(&md, None).unwrap();
        let d = parsed.doc.as_ref().unwrap();
        assert_eq!(d.blocks.len(), 3);
        assert_eq!(d.blocks[0].text.text, "Added before");
        assert_eq!(d.blocks[2].text.text, "Added after");
        assert_eq!(d.blocks[1].text.text, "fn main() {}\n");
        let (saved, layout) = encode(&parsed).unwrap();
        assert_eq!(decode(&saved, layout.as_deref()).unwrap(), parsed);
    }
    #[test]
    fn edits_to_markdown_are_the_authoritative_body() {
        let mut d = DocDocument::empty();
        d.title = RichText::plain("Title");
        d.blocks[0].text = RichText::plain("Original正文");
        let n = note(d);
        let (md, _) = encode(&n).unwrap();
        let changed = md.replace("Original正文", "Externally edited正文");
        assert_eq!(
            decode(&changed, None).unwrap().doc.unwrap().blocks[0]
                .text
                .text,
            "Externally edited正文"
        );
        let heading = md.replace("Original正文", "## New heading");
        let b = decode(&heading, None)
            .unwrap()
            .doc
            .unwrap()
            .blocks
            .remove(0);
        assert!(matches!(b.kind, DocBlockKind::Heading { level: 2 }));
        assert_eq!(b.text.text, "New heading");
    }
}

pub fn document_markdown(doc: &DocDocument) -> Result<String, String> {
    let mut out = format!("# {}\n\n", rich_markdown(&doc.title));
    write_blocks(&doc.blocks, 0, "", &mut out)?;
    Ok(out)
}

pub fn write_document_body(doc: &DocDocument) -> Result<String, String> {
    let mut out = String::new();
    write_blocks(&doc.blocks, 0, "", &mut out)?;
    Ok(out)
}
pub fn read_document_body(body: &str, doc: &mut DocDocument) -> Result<(), String> {
    let parts = sections(body, BLOCK)?;
    if parts.is_empty() && !body.trim().is_empty() {
        doc.blocks = parse_standard(body, &mut doc.next_id)?;
    } else {
        if let Some((json, _)) = parts.first() {
            let offset = json.as_ptr() as usize - body.as_ptr() as usize;
            let line_start = body[..offset].rfind('\n').map_or(0, |i| i + 1);
            let leading = &body[..line_start];
            if !leading.trim().is_empty() {
                doc.blocks = parse_standard(leading, &mut doc.next_id)?;
            }
        }
        for (json, source) in parts {
            let meta: BlockMeta = serde_json::from_str(json).map_err(|e| e.to_string())?;
            let block = decode_block(source, &meta)?;
            doc.next_id = doc.next_id.max(block.id + 1);
            append_at(&mut doc.blocks, meta.depth, block)?;
            let normalized = normalize_block_source(source, &meta)?;
            let tail = &normalized[primary_end(&normalized)..];
            if !tail.trim().is_empty() {
                for extra in parse_standard(tail, &mut doc.next_id)? {
                    append_at(&mut doc.blocks, meta.depth, extra)?;
                }
            }
        }
    }
    Ok(())
}

/// Decode a card payload into the sole editable document model.
/// Pre-structured metadata is a file input format, never a runtime editor choice.
pub fn card_document(card: &Block) -> Result<DocDocument, String> {
    if !card.is_textual() {
        return Err("Card has no editable document".into());
    }
    let mut doc = DocDocument::empty();
    doc.blocks.clear();
    if card.structured {
        read_document_body(&card.body, &mut doc)?;
    } else {
        if !card.title.is_empty() && card.kind != Kind::Code {
            let title = doc.new_block(DocBlockKind::Heading { level: 2 }, &card.title);
            doc.blocks.push(title);
        }
        match card.kind {
            Kind::Code => {
                let block = doc.new_block(
                    DocBlockKind::Code {
                        lang: card.title.clone(),
                        wrap: true,
                    },
                    &card.body,
                );
                doc.blocks.push(block);
            }
            Kind::Table => {
                let table = card
                    .table
                    .clone()
                    .ok_or("Table card has no table payload")?;
                let block = doc.new_block(DocBlockKind::Table(table), "");
                doc.blocks.push(block);
            }
            Kind::Text | Kind::Tasks | Kind::Callout => {
                if !card.body_html.is_empty() {
                    return Err("HTML-only card requires explicit library import".into());
                }
                let mut blocks = parse_standard(&card.body, &mut doc.next_id)?;
                if card.kind == Kind::Callout {
                    let mut block = doc.new_block(
                        DocBlockKind::Callout {
                            emoji: "💡".into(),
                            palette: "orange".into(),
                        },
                        "",
                    );
                    block.children.append(&mut blocks);
                    doc.blocks.push(block);
                } else {
                    doc.blocks.append(&mut blocks);
                }
            }
            _ => unreachable!("textual card kind checked above"),
        }
        for block in &mut doc.blocks {
            block.align = card.text_align;
        }
    }
    doc.ensure_non_empty();
    Ok(doc)
}
