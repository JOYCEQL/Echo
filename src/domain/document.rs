use serde::{Deserialize, Serialize};

pub const TITLE_ID: u64 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteFormat {
    #[default]
    Canvas,
    Document,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageWidth {
    #[default]
    Standard,
    Wide,
    Full,
}

impl PageWidth {
    pub fn max_px(self) -> Option<f32> {
        match self {
            Self::Standard => Some(820.0),
            Self::Wide => Some(980.0),
            Self::Full => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMarks {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strike: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub code: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRun {
    pub len: usize,
    #[serde(default)]
    pub marks: TextMarks,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichText {
    pub text: String,
    #[serde(default)]
    pub runs: Vec<TextRun>,
}

impl RichText {
    pub fn plain(s: impl Into<String>) -> Self {
        let text = s.into();
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            vec![TextRun {
                len: text.len(),
                marks: TextMarks::default(),
            }]
        };
        Self { text, runs }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn normalize(&mut self) {
        self.runs.retain(|r| r.len > 0);
        let total: usize = self.runs.iter().map(|r| r.len).sum();
        if total != self.text.len() {
            if self.text.is_empty() {
                self.runs.clear();
            } else if total < self.text.len() {
                self.runs.push(TextRun {
                    len: self.text.len() - total,
                    marks: TextMarks::default(),
                });
            } else {
                let mut left = self.text.len();
                let mut trimmed = Vec::new();
                for mut r in self.runs.drain(..) {
                    if left == 0 {
                        break;
                    }
                    let take = r.len.min(left);
                    r.len = take;
                    trimmed.push(r);
                    left -= take;
                }
                self.runs = trimmed;
            }
        }
        self.runs.dedup_by(|next, previous| {
            if next.marks == previous.marks {
                previous.len += next.len;
                true
            } else {
                false
            }
        });
    }

    pub fn iter_spans(&self) -> impl Iterator<Item = (std::ops::Range<usize>, &TextMarks)> {
        let mut off = 0usize;
        self.runs.iter().map(move |r| {
            let start = off;
            off += r.len;
            (start..off, &r.marks)
        })
    }

    pub fn marks_at(&self, offset: usize) -> TextMarks {
        if self.runs.is_empty() {
            return TextMarks::default();
        }
        let target = offset.saturating_sub(1);
        for (r, m) in self.iter_spans() {
            if target >= r.start && target < r.end {
                return m.clone();
            }
        }
        self.runs
            .first()
            .map(|r| r.marks.clone())
            .unwrap_or_default()
    }

    pub fn push(&mut self, s: &str, marks: TextMarks) {
        if s.is_empty() {
            return;
        }
        self.text.push_str(s);
        self.runs.push(TextRun {
            len: s.len(),
            marks,
        });
        self.normalize();
    }

    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        let a = range.start.min(self.text.len());
        let b = range.end.min(self.text.len());
        if a >= b {
            return Self::default();
        }
        let mut out = Self {
            text: self.text[a..b].to_string(),
            runs: Vec::new(),
        };
        for (r, m) in self.iter_spans() {
            let s = r.start.max(a);
            let e = r.end.min(b);
            if s < e {
                out.runs.push(TextRun {
                    len: e - s,
                    marks: m.clone(),
                });
            }
        }
        out.normalize();
        out
    }

    pub fn replace(&mut self, range: std::ops::Range<usize>, with: &Self) {
        let a = range.start.min(self.text.len());
        let b = range.end.min(self.text.len());
        let head = self.slice(0..a);
        let tail = self.slice(b..self.text.len());
        let mut next = head;
        next.text.push_str(&with.text);
        next.runs.extend(with.runs.iter().cloned());
        next.text.push_str(&tail.text);
        next.runs.extend(tail.runs);
        next.normalize();
        *self = next;
    }

    pub fn insert_plain(&mut self, offset: usize, s: &str, marks: TextMarks) {
        if s.is_empty() {
            return;
        }
        let at = offset.min(self.text.len());
        self.normalize();
        self.text.insert_str(at, s);
        let mut start = 0;
        let mut index = self.runs.len();
        for (i, run) in self.runs.iter_mut().enumerate() {
            let end = start + run.len;
            if at >= start && at <= end && run.marks == marks {
                run.len += s.len();
                return;
            }
            if at < end {
                index = i;
                if at > start {
                    let tail = TextRun { len: end - at, marks: run.marks.clone() };
                    run.len = at - start;
                    index += 1;
                    self.runs.insert(index, tail);
                }
                break;
            }
            start = end;
        }
        self.runs.insert(index, TextRun { len: s.len(), marks });
    }

    pub fn split_off(&mut self, offset: usize) -> Self {
        let at = offset.min(self.text.len());
        let tail = self.slice(at..self.text.len());
        *self = self.slice(0..at);
        tail
    }

    pub fn update_marks(
        &mut self,
        range: std::ops::Range<usize>,
        mut f: impl FnMut(&mut TextMarks),
    ) {
        let a = range.start.min(self.text.len());
        let b = range.end.min(self.text.len());
        if a >= b {
            return;
        }
        let mut mid = self.slice(a..b);
        for r in &mut mid.runs {
            f(&mut r.marks);
        }
        mid.normalize();
        self.replace(a..b, &mid);
    }

    pub fn all_have(
        &self,
        range: std::ops::Range<usize>,
        pred: impl Fn(&TextMarks) -> bool,
    ) -> bool {
        let a = range.start.min(self.text.len());
        let b = range.end.min(self.text.len());
        if a >= b {
            return false;
        }
        let mut any = false;
        for (r, m) in self.iter_spans() {
            if r.end > a && r.start < b {
                any = true;
                if !pred(m) {
                    return false;
                }
            }
        }
        any
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DocBlockKind {
    Text,
    Heading {
        level: u8,
    },
    Bullet,
    Ordered {
        #[serde(default)]
        start: Option<u32>,
    },
    Todo {
        #[serde(default)]
        done: bool,
    },
    Quote,
    Callout {
        #[serde(default = "default_callout_emoji")]
        emoji: String,
        #[serde(default = "default_callout_palette")]
        palette: String,
    },
    Code {
        #[serde(default = "default_code_lang")]
        lang: String,
        #[serde(default = "default_true")]
        wrap: bool,
    },
    Divider,
    Image {
        src: String,
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        caption: String,
    },
    Table(crate::table_data::TableData),
}

fn default_callout_emoji() -> String {
    "💡".to_string()
}

fn default_callout_palette() -> String {
    "orange".to_string()
}

fn default_code_lang() -> String {
    "Plain Text".to_string()
}

fn default_true() -> bool {
    true
}

impl DocBlockKind {
    pub fn has_text(&self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Heading { .. }
                | Self::Bullet
                | Self::Ordered { .. }
                | Self::Todo { .. }
                | Self::Code { .. }
        )
    }

    pub fn is_container(&self) -> bool {
        matches!(self, Self::Quote | Self::Callout { .. })
    }

    pub fn is_list(&self) -> bool {
        matches!(
            self,
            Self::Bullet | Self::Ordered { .. } | Self::Todo { .. }
        )
    }

    pub fn can_nest(&self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Heading { .. }
                | Self::Bullet
                | Self::Ordered { .. }
                | Self::Todo { .. }
        )
    }

    pub fn resets_on_enter_split(&self) -> Self {
        match self {
            Self::Heading { .. } => Self::Text,
            Self::Ordered { .. } => Self::Ordered { start: None },
            Self::Todo { .. } => Self::Todo { done: false },
            other => other.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocBlock {
    pub id: u64,
    pub kind: DocBlockKind,
    #[serde(default)]
    pub align: DocAlign,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub folded: bool,
    #[serde(default)]
    pub text: RichText,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DocBlock>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocDocument {
    pub next_id: u64,
    #[serde(default)]
    pub title: RichText,
    #[serde(default)]
    pub blocks: Vec<DocBlock>,
    #[serde(default)]
    pub page_width: PageWidth,
}

impl Default for DocDocument {
    fn default() -> Self {
        Self::empty()
    }
}

impl DocDocument {
    pub fn empty() -> Self {
        let mut d = Self {
            next_id: 1,
            title: RichText::plain(""),
            blocks: Vec::new(),
            page_width: PageWidth::Standard,
        };
        let b = d.new_block(DocBlockKind::Text, "");
        d.blocks.push(b);
        d
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn new_block(&mut self, kind: DocBlockKind, text: impl Into<String>) -> DocBlock {
        let id = self.alloc_id();
        DocBlock {
            id,
            kind,
            align: DocAlign::Left,
            bg: None,
            folded: false,
            text: RichText::plain(text),
            children: Vec::new(),
        }
    }

    pub fn new_container(&mut self, kind: DocBlockKind, first_child: DocBlock) -> DocBlock {
        let id = self.alloc_id();
        DocBlock {
            id,
            kind,
            align: DocAlign::Left,
            bg: None,
            folded: false,
            text: RichText::default(),
            children: vec![first_child],
        }
    }

    pub fn ensure_non_empty(&mut self) {
        if self.blocks.is_empty() {
            let b = self.new_block(DocBlockKind::Text, "");
            self.blocks.push(b);
        }
    }

    pub fn preview_text(&self) -> String {
        fn collect(list: &[DocBlock], out: &mut Vec<String>) {
            for b in list {
                let t = b.text.text.trim();
                if !t.is_empty() {
                    out.push(t.replace('\n', " "));
                    if out.len() >= 3 {
                        return;
                    }
                }
                collect(&b.children, out);
                if out.len() >= 3 {
                    return;
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.blocks, &mut out);
        out.join(" · ")
    }

    pub fn has_headings(&self) -> bool {
        fn contains(blocks: &[DocBlock]) -> bool {
            blocks.iter().any(|b| matches!(b.kind, DocBlockKind::Heading { .. }) || contains(&b.children))
        }
        contains(&self.blocks)
    }

    pub fn headings(&self) -> Vec<(u64, u8, String)> {
        let mut out = Vec::new();
        fn walk(list: &[DocBlock], out: &mut Vec<(u64, u8, String)>) {
            for b in list {
                if let DocBlockKind::Heading { level } = b.kind {
                    let t = b.text.text.trim();
                    out.push((
                        b.id,
                        level,
                        if t.is_empty() {
                            format!("H{level}")
                        } else {
                            t.to_string()
                        },
                    ));
                }
                walk(&b.children, out);
            }
        }
        walk(&self.blocks, &mut out);
        out
    }

    pub fn to_plain_text(&self) -> String {
        let mut parts = Vec::new();
        if !self.title.is_empty() {
            parts.push(self.title.text.clone());
        }
        blocks_to_plain(&self.blocks, 0, &mut parts);
        parts.join("\n")
    }
}

pub fn block_plain_text(block: &DocBlock) -> String {
    let mut parts = Vec::new();
    blocks_to_plain(std::slice::from_ref(block), 0, &mut parts);
    parts.join("\n")
}

fn blocks_to_plain(list: &[DocBlock], depth: usize, out: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    for (idx, b) in list.iter().enumerate() {
        match &b.kind {
            DocBlockKind::Divider => out.push(format!("{indent}---")),
            DocBlockKind::Image { caption, src, .. } => {
                let label = if caption.is_empty() { src } else { caption };
                out.push(format!("{indent}[图片: {label}]"));
            }
            DocBlockKind::Table(t) => {
                for row in &t.rows {
                    let line: Vec<String> = row.iter().map(|c| c.clone()).collect();
                    out.push(format!("{indent}| {} |", line.join(" | ")));
                }
            }
            DocBlockKind::Quote | DocBlockKind::Callout { .. } => {
                blocks_to_plain(&b.children, depth, out);
            }
            DocBlockKind::Todo { done } => {
                let mark = if *done { "[x]" } else { "[ ]" };
                out.push(format!("{indent}{mark} {}", b.text.text));
                blocks_to_plain(&b.children, depth + 1, out);
            }
            DocBlockKind::Bullet => {
                out.push(format!("{indent}• {}", b.text.text));
                blocks_to_plain(&b.children, depth + 1, out);
            }
            DocBlockKind::Ordered { .. } => {
                let n = ordered_number(list, idx);
                out.push(format!("{indent}{n}. {}", b.text.text));
                blocks_to_plain(&b.children, depth + 1, out);
            }
            _ => {
                out.push(format!("{indent}{}", b.text.text));
                blocks_to_plain(&b.children, depth + 1, out);
            }
        }
    }
}

pub fn ordered_number(siblings: &[DocBlock], idx: usize) -> u32 {
    let mut seq = 1u32;
    let mut run_start = idx;
    while run_start > 0 && matches!(siblings[run_start - 1].kind, DocBlockKind::Ordered { .. }) {
        run_start -= 1;
    }
    for (i, b) in siblings[run_start..=idx].iter().enumerate() {
        if let DocBlockKind::Ordered { start: Some(s) } = b.kind {
            seq = s;
        } else if i > 0 {
            seq += 1;
        }
    }
    seq
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocPos {
    pub block: u64,
    pub offset: usize,
}

impl DocPos {
    pub fn new(block: u64, offset: usize) -> Self {
        Self { block, offset }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocSel {
    pub anchor: DocPos,
    pub head: DocPos,
}

impl DocSel {
    pub fn caret(p: DocPos) -> Self {
        Self { anchor: p, head: p }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocUnit {
    pub id: u64,
    pub text: bool,
    pub depth: usize,
    pub is_code: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocPreedit {
    pub block: u64,
    pub offset: usize,
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
struct DocSnapshot {
    title: RichText,
    next_id: u64,
    page_width: PageWidth,
    blocks: Vec<std::sync::Arc<DocBlock>>,
    sel: DocSel,
}
impl DocSnapshot {
    fn capture(doc: &DocDocument, sel: DocSel, previous: Option<&Self>) -> Self {
        let old: std::collections::HashMap<_, _> = previous
            .into_iter()
            .flat_map(|p| &p.blocks)
            .map(|b| (b.id, b))
            .collect();
        Self {
            title: doc.title.clone(),
            next_id: doc.next_id,
            page_width: doc.page_width,
            blocks: doc
                .blocks
                .iter()
                .map(|b| match old.get(&b.id) {
                    Some(old) if old.as_ref() == b => std::sync::Arc::clone(old),
                    _ => std::sync::Arc::new(b.clone()),
                })
                .collect(),
            sel,
        }
    }
    fn document(&self) -> DocDocument {
        DocDocument {
            title: self.title.clone(),
            next_id: self.next_id,
            page_width: self.page_width,
            blocks: self.blocks.iter().map(|b| b.as_ref().clone()).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UndoKind {
    Typing,
    Delete,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocMark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
}

#[derive(Clone, Debug)]
pub struct DocCore {
    /// Canvas cards share document editing without a separate page title.
    pub include_title: bool,
    pub doc: DocDocument,
    pub sel: DocSel,
    pub block_sel: Option<(u64, u64)>,
    pub pending_marks: Option<TextMarks>,
    pub preedit: Option<DocPreedit>,
    pub dirty: bool,
    undo: Vec<DocSnapshot>,
    redo: Vec<DocSnapshot>,
    last_edit: Option<UndoKind>,
    transaction_depth: usize,
    last_edit_at: std::time::Instant,
}

impl DocCore {
    pub fn new(mut doc: DocDocument) -> Self {
        doc.ensure_non_empty();
        let first = doc.blocks.first().map(|b| b.id).unwrap_or(TITLE_ID);
        Self {
            include_title: true,
            doc,
            sel: DocSel::caret(DocPos::new(first, 0)),
            block_sel: None,
            pending_marks: None,
            preedit: None,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            transaction_depth: 0,
            last_edit_at: std::time::Instant::now(),
        }
    }

    pub fn units(&self) -> Vec<DocUnit> {
        let mut out = Vec::new();
        if self.include_title {
            out.push(DocUnit {
                id: TITLE_ID,
                text: true,
                depth: 0,
                is_code: false,
            });
        }
        fn walk(list: &[DocBlock], depth: usize, out: &mut Vec<DocUnit>) {
            let mut hide_below: Option<u8> = None;
            for b in list {
                if let Some(lvl) = hide_below {
                    match b.kind {
                        DocBlockKind::Heading { level } if level <= lvl => hide_below = None,
                        _ => continue,
                    }
                }
                if matches!(b.kind, DocBlockKind::Quote | DocBlockKind::Callout { .. }) {
                    walk(&b.children, depth, out);
                    continue;
                }
                let is_code = matches!(b.kind, DocBlockKind::Code { .. });
                out.push(DocUnit {
                    id: b.id,
                    text: b.kind.has_text(),
                    depth,
                    is_code,
                });
                if let DocBlockKind::Heading { level } = b.kind
                    && b.folded
                {
                    hide_below = Some(level);
                } else {
                    walk(&b.children, depth + 1, out);
                }
            }
        }
        walk(&self.doc.blocks, 0, &mut out);
        out
    }

    pub fn in_block_mode(&self) -> bool {
        self.block_sel.is_some()
    }

    pub fn clear_block_sel(&mut self) {
        self.block_sel = None;
    }

    pub fn block(&self, id: u64) -> Option<&DocBlock> {
        if id == TITLE_ID {
            return None;
        }
        find_block(&self.doc.blocks, id)
    }

    pub fn block_mut(&mut self, id: u64) -> Option<&mut DocBlock> {
        if id == TITLE_ID {
            return None;
        }
        find_block_mut(&mut self.doc.blocks, id)
    }

    pub fn parent_of(&self, id: u64) -> Option<&DocBlock> {
        find_parent(&self.doc.blocks, id)
    }

    pub fn text_of(&self, id: u64) -> Option<&RichText> {
        if id == TITLE_ID {
            return Some(&self.doc.title);
        }
        self.block(id)
            .filter(|b| b.kind.has_text())
            .map(|b| &b.text)
    }

    pub fn text_mut(&mut self, id: u64) -> Option<&mut RichText> {
        if id == TITLE_ID {
            return Some(&mut self.doc.title);
        }
        self.block_mut(id)
            .filter(|b| b.kind.has_text())
            .map(|b| &mut b.text)
    }

    pub fn len_of(&self, id: u64) -> usize {
        self.text_of(id).map_or(0, |t| t.len())
    }

    pub fn is_code(&self, id: u64) -> bool {
        self.block(id)
            .is_some_and(|b| matches!(b.kind, DocBlockKind::Code { .. }))
    }

    pub fn ordered(&self) -> (DocPos, DocPos) {
        let a = self.sel.anchor;
        let b = self.sel.head;
        if a.block == b.block {
            return if a.offset <= b.offset { (a, b) } else { (b, a) };
        }
        let us = self.units();
        let ia = us.iter().position(|u| u.id == a.block).unwrap_or(0);
        let ib = us.iter().position(|u| u.id == b.block).unwrap_or(0);
        if (ia, a.offset) <= (ib, b.offset) {
            (a, b)
        } else {
            (b, a)
        }
    }

    fn fix_pos(&self, us: &[DocUnit], mut p: DocPos) -> DocPos {
        if !us.iter().any(|u| u.id == p.block) {
            p.block = us.get(1).or_else(|| us.first()).map_or(TITLE_ID, |u| u.id);
            p.offset = 0;
        }
        let max = self.len_of(p.block);
        p.offset = clamp_boundary(
            self.text_of(p.block).map(|t| t.text.as_str()).unwrap_or(""),
            p.offset.min(max),
        );
        p
    }

    pub fn fix_selection(&mut self) {
        let us = self.units();
        let anchor = self.fix_pos(&us, self.sel.anchor);
        let head = self.fix_pos(&us, self.sel.head);
        self.sel.anchor = anchor;
        self.sel.head = head;
    }

    fn checkpoint(&mut self, kind: UndoKind) {
        if self.transaction_depth > 0 {
            return;
        }
        let now = std::time::Instant::now();
        let coalesce = matches!(kind, UndoKind::Typing | UndoKind::Delete)
            && self.last_edit == Some(kind)
            && now.duration_since(self.last_edit_at).as_millis() < 600
            && !self.undo.is_empty();
        self.last_edit = Some(kind);
        self.last_edit_at = now;
        self.redo.clear();
        self.dirty = true;
        if coalesce {
            return;
        }
        self.undo
            .push(DocSnapshot::capture(&self.doc, self.sel, self.undo.last()));
        if self.undo.len() > 250 {
            self.undo.remove(0);
        }
    }

    pub fn break_coalescing(&mut self) {
        self.last_edit = None;
    }

    pub fn with_undo(&mut self, f: impl FnOnce(&mut Self)) {
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        self.transaction_depth += 1;
        f(self);
        self.transaction_depth -= 1;
        self.normalize();
        self.fix_selection();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(DocSnapshot::capture(&self.doc, self.sel, Some(&snap)));
        self.doc = snap.document();
        self.sel = snap.sel;
        self.block_sel = None;
        self.pending_marks = None;
        self.preedit = None;
        self.last_edit = None;
        self.dirty = true;
        self.fix_selection();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snap) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push(DocSnapshot::capture(&self.doc, self.sel, self.undo.last()));
        self.doc = snap.document();
        self.sel = snap.sel;
        self.block_sel = None;
        self.pending_marks = None;
        self.preedit = None;
        self.last_edit = None;
        self.dirty = true;
        self.fix_selection();
        true
    }

    pub fn selected_blocks_ids(&self) -> Vec<u64> {
        if let Some((a, b)) = self.block_sel {
            let us = self.units();
            let ia = us.iter().position(|u| u.id == a);
            let ib = us.iter().position(|u| u.id == b);
            if let (Some(i), Some(j)) = (ia, ib) {
                let (lo, hi) = if i <= j { (i, j) } else { (j, i) };
                return us[lo..=hi]
                    .iter()
                    .map(|u| u.id)
                    .filter(|id| *id != TITLE_ID)
                    .collect();
            }
        }
        let (s, e) = self.ordered();
        let us = self.units();
        let si = us.iter().position(|u| u.id == s.block).unwrap_or(0);
        let ei = us.iter().position(|u| u.id == e.block).unwrap_or(0);
        us[si..=ei]
            .iter()
            .map(|u| u.id)
            .filter(|id| *id != TITLE_ID)
            .collect()
    }

    pub fn selected_text(&self) -> String {
        if self.in_block_mode() {
            let ids = self.selected_blocks_ids();
            let mut parts = Vec::new();
            for id in ids {
                if let Some(b) = self.block(id) {
                    if matches!(b.kind, DocBlockKind::Table(_)) {
                        parts.push(block_plain_text(b));
                    } else if !b.text.is_empty() {
                        parts.push(b.text.text.clone());
                    }
                }
            }
            return parts.join("\n");
        }
        let (s, e) = self.ordered();
        if s == e {
            return String::new();
        }
        if s.block == e.block {
            return self
                .text_of(s.block)
                .map(|t| t.slice(s.offset..e.offset).text)
                .unwrap_or_default();
        }
        let us = self.units();
        let si = us.iter().position(|u| u.id == s.block).unwrap_or(0);
        let ei = us.iter().position(|u| u.id == e.block).unwrap_or(0);
        let mut out = Vec::new();
        for u in &us[si..=ei] {
            if !u.text
                && let Some(table) = self
                    .block(u.id)
                    .filter(|b| matches!(b.kind, DocBlockKind::Table(_)))
            {
                out.push(block_plain_text(table));
            } else if u.id == s.block {
                if let Some(t) = self.text_of(u.id) {
                    out.push(t.slice(s.offset..t.len()).text);
                }
            } else if u.id == e.block {
                if let Some(t) = self.text_of(u.id) {
                    out.push(t.slice(0..e.offset).text);
                }
            } else if let Some(t) = self.text_of(u.id) {
                out.push(t.text.clone());
            } else if let Some(b) = self.block(u.id) {
                match &b.kind {
                    DocBlockKind::Divider => out.push("---".into()),
                    DocBlockKind::Image { caption, .. } => out.push(format!("[图片 {caption}]")),
                    _ => {}
                }
            }
        }
        out.join("\n")
    }

    fn delete_selection_inner(&mut self) -> DocPos {
        if let Some((a, b)) = self.block_sel.take() {
            let us = self.units();
            let ia = us.iter().position(|u| u.id == a).unwrap_or(0);
            let ib = us.iter().position(|u| u.id == b).unwrap_or(0);
            let (lo, hi) = if ia <= ib { (ia, ib) } else { (ib, ia) };
            let next_pos = us.get(hi + 1).map(|u| DocPos::new(u.id, 0)).or_else(|| {
                lo.checked_sub(1)
                    .filter(|&k| us[k].id != TITLE_ID)
                    .map(|k| DocPos::new(us[k].id, self.len_of(us[k].id)))
            });
            let ids: Vec<u64> = us[lo..=hi]
                .iter()
                .map(|u| u.id)
                .filter(|id| *id != TITLE_ID)
                .collect();
            for id in ids.into_iter().rev() {
                self.remove_lift(id);
            }
            self.normalize();
            let pos = next_pos.unwrap_or_else(|| {
                let first = self.doc.blocks.first().map(|b| b.id).unwrap_or(TITLE_ID);
                DocPos::new(first, 0)
            });
            self.sel = DocSel::caret(pos);
            self.fix_selection();
            return self.sel.head;
        }

        let (s, e) = self.ordered();
        if s == e {
            return s;
        }
        if s.block == e.block {
            if let Some(t) = self.text_mut(s.block) {
                t.replace(s.offset..e.offset, &RichText::default());
            } else {
                return self.delete_atomic(s.block);
            }
            self.sel = DocSel::caret(s);
            return s;
        }

        let us = self.units();
        let si = us.iter().position(|u| u.id == s.block).unwrap_or(0);
        let ei = us.iter().position(|u| u.id == e.block).unwrap_or(0);
        let start_unit = us[si];
        let end_unit = us[ei];

        // Selection spanning from TITLE into body: clear title tail, drop blocks strictly before end block,
        // truncate end block prefix, keep caret at end of title (or body start if title cleared cleanly).
        if s.block == TITLE_ID {
            let len = self.doc.title.len();
            self.doc.title.replace(s.offset..len, &RichText::default());
            let middle: Vec<u64> = us[si + 1..ei].iter().map(|u| u.id).collect();
            for id in middle.into_iter().rev() {
                self.remove_lift(id);
            }
            if end_unit.text {
                if let Some(t) = self.text_mut(e.block) {
                    t.replace(0..e.offset, &RichText::default());
                }
            } else {
                self.remove_subtree(e.block);
            }
            self.normalize();
            self.sel = DocSel::caret(s);
            return s;
        }

        let tail = if end_unit.text {
            self.text_of(e.block)
                .map(|t| t.slice(e.offset..t.len()))
                .unwrap_or_default()
        } else {
            RichText::default()
        };
        let middle: Vec<u64> = us[si + 1..ei].iter().map(|u| u.id).collect();

        if start_unit.text {
            let tail = if self.is_code(s.block) {
                RichText::plain(tail.text)
            } else {
                tail
            };
            if let Some(t) = self.text_mut(s.block) {
                let len = t.len();
                t.replace(s.offset..len, &tail);
            }
            for id in middle.into_iter().rev() {
                self.remove_lift(id);
            }
            let kids = self
                .block_mut(e.block)
                .map(|b| std::mem::take(&mut b.children))
                .unwrap_or_default();
            self.remove_lift(e.block);
            if self.block(s.block).is_some_and(|b| b.kind.can_nest()) {
                let mut kids = kids;
                if let Some(sb) = self.block_mut(s.block) {
                    kids.append(&mut sb.children);
                    sb.children = kids;
                }
            } else {
                self.insert_all_after(s.block, kids);
            }
            self.normalize();
            self.sel = DocSel::caret(s);
            s
        } else {
            for id in middle.into_iter().rev() {
                self.remove_lift(id);
            }
            if end_unit.text {
                if let Some(t) = self.text_mut(e.block) {
                    t.replace(0..e.offset, &RichText::default());
                }
                self.remove_subtree(s.block);
                self.normalize();
                let p = DocPos::new(e.block, 0);
                self.sel = DocSel::caret(p);
                p
            } else {
                self.remove_subtree(e.block);
                self.delete_atomic(s.block)
            }
        }
    }

    fn delete_atomic(&mut self, id: u64) -> DocPos {
        let us = self.units();
        let i = us.iter().position(|u| u.id == id).unwrap_or(0);
        let next = us.get(i + 1).filter(|u| u.text).map(|u| u.id);
        let prev = i
            .checked_sub(1)
            .and_then(|k| us.get(k))
            .filter(|u| u.text && u.id != TITLE_ID)
            .map(|u| u.id);
        let target_pos = if let Some(n) = next {
            self.remove_subtree(id);
            DocPos::new(n, 0)
        } else if let Some(p) = prev {
            self.remove_subtree(id);
            DocPos::new(p, self.len_of(p))
        } else {
            let nb = self.doc.new_block(DocBlockKind::Text, "");
            let nid = nb.id;
            replace_block(&mut self.doc.blocks, id, nb);
            DocPos::new(nid, 0)
        };
        self.normalize();
        self.sel = DocSel::caret(target_pos);
        self.fix_selection();
        self.sel.head
    }

    fn normalize(&mut self) {
        fn walk(list: &mut Vec<DocBlock>) {
            for b in list.iter_mut() {
                walk(&mut b.children);
            }
            list.retain(|b| !(b.kind.is_container() && b.children.is_empty()));
        }
        walk(&mut self.doc.blocks);
        self.doc.ensure_non_empty();
    }

    pub fn delete_selection(&mut self) {
        if !self.sel.is_collapsed() || self.in_block_mode() {
            self.checkpoint(UndoKind::Delete);
            self.delete_selection_inner();
        }
    }

    pub fn insert_text(&mut self, s: &str) {
        self.checkpoint(UndoKind::Typing);
        let mut p = self.delete_selection_inner();
        if self.text_of(p.block).is_none() {
            let nb = self.doc.new_block(DocBlockKind::Text, "");
            let nid = nb.id;
            self.insert_after(p.block, nb);
            p = DocPos::new(nid, 0);
        }
        let marks = match self.pending_marks.take() {
            Some(m) => m,
            None => self
                .text_of(p.block)
                .map(|t| t.marks_at(p.offset))
                .unwrap_or_default(),
        };
        let marks = if self.is_code(p.block) || p.block == TITLE_ID {
            TextMarks::default()
        } else {
            marks
        };
        if let Some(t) = self.text_mut(p.block) {
            t.insert_plain(p.offset, s, marks);
        }
        self.sel = DocSel::caret(DocPos::new(p.block, p.offset + s.len()));
        if p.block != TITLE_ID && !self.is_code(p.block) {
            self.try_markdown_shortcut(s);
        }
    }

    fn try_markdown_shortcut(&mut self, typed: &str) -> bool {
        let p = self.sel.head;
        let Some(b) = self.block(p.block) else {
            return false;
        };
        let is_plain = matches!(b.kind, DocBlockKind::Text);
        let txt_len = b.text.text.len();
        let prefix = &b.text.text[..p.offset];

        // Block-level shortcuts triggered on space
        if typed == " " && is_plain {
            let new_kind = crate::input_rules::block(prefix);
            if let Some(kind) = new_kind {
                let cut = prefix.len();
                self.checkpoint(UndoKind::Other);
                let tail = self
                    .block_mut(p.block)
                    .map(|b| b.text.slice(cut..b.text.len()))
                    .unwrap_or_default();
                if matches!(kind, DocBlockKind::Quote) {
                    let child = self.doc.new_block(DocBlockKind::Text, "");
                    let cid = child.id;
                    let mut container = self.doc.new_container(DocBlockKind::Quote, child);
                    container.children[0].text = tail;
                    replace_block(&mut self.doc.blocks, p.block, container);
                    self.sel = DocSel::caret(DocPos::new(cid, 0));
                } else if let Some(b) = self.block_mut(p.block) {
                    b.kind = kind;
                    b.text = tail;
                    self.sel = DocSel::caret(DocPos::new(p.block, 0));
                }
                return true;
            }
        }

        // Exact line divider `---` or `***`
        if is_plain && matches!(b.text.text.as_str(), "---" | "***") && p.offset == txt_len {
            self.checkpoint(UndoKind::Other);
            let after = self.doc.new_block(DocBlockKind::Text, "");
            let after_id = after.id;
            if let Some(b) = self.block_mut(p.block) {
                b.kind = DocBlockKind::Divider;
                b.text = RichText::default();
            }
            self.insert_after(p.block, after);
            self.sel = DocSel::caret(DocPos::new(after_id, 0));
            return true;
        }

        // Inline code/bold/italic/strike closing shortcuts
        if let Some((replace, content, mark)) = crate::input_rules::inline(prefix) {
            self.checkpoint(UndoKind::Other);
            if let Some(t) = self.text_mut(p.block) {
                let mut mid = t.slice(content);
                mid.update_marks(0..mid.len(), |m| set_mark(m, mark, true));
                let caret = replace.start + mid.len();
                t.replace(replace, &mid);
                self.sel = DocSel::caret(DocPos::new(p.block, caret));
                self.pending_marks = Some(TextMarks::default());
                return true;
            }
        }
        let prefix = &self.text_of(p.block).expect("text block").text[..p.offset];
        if let Some((replace, content, url)) = crate::input_rules::link(prefix) {
            let url = url.to_owned();
            self.checkpoint(UndoKind::Other);
            if let Some(text) = self.text_mut(p.block) {
                let mut rich = text.slice(content);
                rich.update_marks(0..rich.len(), |m| m.link = Some(url.clone()));
                let caret = replace.start + rich.len();
                text.replace(replace, &rich);
                self.sel = DocSel::caret(DocPos::new(p.block, caret));
                self.pending_marks = Some(TextMarks::default());
                return true;
            }
        }
        false
    }

    pub fn enter(&mut self) {
        // A display-math fence starts a source-editable formula block.
        if self.sel.is_collapsed()
            && let Some(b) = self.block(self.sel.head.block)
            && matches!(b.kind, DocBlockKind::Text)
            && b.text.text == "$$" && self.sel.head.offset == 2
        {
            self.checkpoint(UndoKind::Other);
            self.break_coalescing();
            let id = self.sel.head.block;
            if let Some(b) = self.block_mut(id) {
                b.kind = DocBlockKind::Code { lang: "math".into(), wrap: true };
                b.text = RichText::default();
            }
            self.sel = DocSel::caret(DocPos::new(id, 0));
            return;
        }
        // ```lang + Enter turns block into Code
        if self.sel.is_collapsed()
            && let Some(b) = self.block(self.sel.head.block)
            && matches!(b.kind, DocBlockKind::Text)
            && b.text.text.starts_with("```")
            && self.sel.head.offset == b.text.len()
        {
            let lang_raw = b.text.text.trim_start_matches('`').trim();
            let lang = if lang_raw.is_empty() {
                "Plain Text".to_string()
            } else {
                lang_raw.to_string()
            };
            self.checkpoint(UndoKind::Other);
            self.break_coalescing();
            if let Some(b) = self.block_mut(self.sel.head.block) {
                b.kind = DocBlockKind::Code { lang, wrap: true };
                b.text = RichText::default();
            }
            self.sel = DocSel::caret(DocPos::new(self.sel.head.block, 0));
            return;
        }

        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let p = self.delete_selection_inner();

        if p.block == TITLE_ID {
            let tail = self.doc.title.split_off(p.offset);
            let first_id = self.doc.blocks.first().map(|b| b.id);
            let first_empty = self.doc.blocks.first().is_some_and(|b| {
                matches!(b.kind, DocBlockKind::Text) && b.text.is_empty() && b.children.is_empty()
            });
            if tail.is_empty() && first_empty {
                self.sel = DocSel::caret(DocPos::new(first_id.unwrap(), 0));
            } else {
                let mut nb = self.doc.new_block(DocBlockKind::Text, "");
                nb.text = tail;
                let nid = nb.id;
                self.doc.blocks.insert(0, nb);
                self.sel = DocSel::caret(DocPos::new(nid, 0));
            }
            return;
        }

        let Some(block) = self.block(p.block).cloned() else {
            return;
        };

        if !block.kind.has_text() {
            let nb = self.doc.new_block(DocBlockKind::Text, "");
            let nid = nb.id;
            self.insert_after(p.block, nb);
            self.sel = DocSel::caret(DocPos::new(nid, 0));
            return;
        }

        if matches!(block.kind, DocBlockKind::Code { .. }) {
            let before = &block.text.text[..p.offset];
            let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
            let indent: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            let ins = format!("\n{indent}");
            if let Some(t) = self.text_mut(p.block) {
                t.insert_plain(p.offset, &ins, TextMarks::default());
            }
            self.sel = DocSel::caret(DocPos::new(p.block, p.offset + ins.len()));
            return;
        }

        let empty = block.text.is_empty();

        // Empty list / todo item: outdent if nested, otherwise revert to plain Text
        if empty && block.kind.is_list() {
            if self.parent_of(p.block).is_some_and(|pa| pa.kind.can_nest()) {
                outdent_in_list(&mut self.doc.blocks, p.block);
            } else if let Some(b) = self.block_mut(p.block) {
                b.kind = DocBlockKind::Text;
            }
            self.sel = DocSel::caret(DocPos::new(p.block, 0));
            return;
        }

        // Empty last paragraph in Quote / Callout: exit container
        if empty && matches!(block.kind, DocBlockKind::Text) && block.children.is_empty() {
            if let Some(parent) = self.parent_of(p.block).cloned()
                && parent.kind.is_container()
                && parent.children.last().map(|c| c.id) == Some(p.block)
                && parent.children.len() > 1
            {
                if let Some(b) = self.remove_subtree(p.block) {
                    self.insert_after(parent.id, b);
                    self.sel = DocSel::caret(DocPos::new(p.block, 0));
                    return;
                }
            }
            if self.parent_of(p.block).is_some_and(|pa| pa.kind.can_nest()) {
                outdent_in_list(&mut self.doc.blocks, p.block);
                self.sel = DocSel::caret(DocPos::new(p.block, 0));
                return;
            }
        }

        // A folded heading ends visually after its whole section. Opening a peer
        // there must leave the hidden body attached to the original heading.
        if matches!(block.kind, DocBlockKind::Heading { .. })
            && block.folded
            && p.offset == block.text.len()
        {
            let (parent, siblings, start) = self.siblings(p.block).expect("current heading");
            let end = section_end(siblings, start);
            let mut peer = self.doc.new_block(block.kind, "");
            peer.align = block.align;
            let pos = DocPos::new(peer.id, 0);
            self.siblings_mut(parent).insert(end, peer);
            self.set_head(pos, false);
            return;
        }

        let new_kind = block.kind.resets_on_enter_split();
        if p.offset == 0 && !empty {
            let above_kind = if matches!(block.kind, DocBlockKind::Heading { .. }) {
                DocBlockKind::Text
            } else {
                new_kind
            };
            let mut nb = self.doc.new_block(above_kind, "");
            nb.align = block.align;
            self.insert_before(p.block, nb);
            self.sel = DocSel::caret(DocPos::new(p.block, 0));
            return;
        }

        let tail = self
            .text_mut(p.block)
            .map(|t| t.split_off(p.offset))
            .unwrap_or_default();
        // A split inside the heading creates body text, which must be visible.
        if matches!(block.kind, DocBlockKind::Heading { .. }) && block.folded {
            self.block_mut(p.block).expect("current heading").folded = false;
        }
        let mut nb = self.doc.new_block(new_kind, "");
        nb.text = tail;
        nb.align = block.align;
        if !nb.text.is_empty() {
            nb.bg = block.bg.clone();
        }
        let nid = nb.id;
        if !block.children.is_empty() {
            if let Some(b) = self.block_mut(p.block) {
                b.children.insert(0, nb);
            }
        } else {
            self.insert_after(p.block, nb);
        }
        self.sel = DocSel::caret(DocPos::new(nid, 0));
    }

    pub fn soft_break(&mut self) {
        if self.in_block_mode() || self.sel.head.block == TITLE_ID {
            return;
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let p = self.delete_selection_inner();
        if self.text_of(p.block).is_none() {
            return;
        }
        let marks = self
            .text_of(p.block)
            .map(|t| t.marks_at(p.offset))
            .unwrap_or_default();
        let marks = if self.is_code(p.block) {
            TextMarks::default()
        } else {
            marks
        };
        if let Some(t) = self.text_mut(p.block) {
            t.insert_plain(p.offset, "\n", marks);
        }
        self.sel = DocSel::caret(DocPos::new(p.block, p.offset + 1));
    }

    pub fn backspace(&mut self) {
        if !self.sel.is_collapsed() || self.in_block_mode() {
            self.checkpoint(UndoKind::Delete);
            self.delete_selection_inner();
            return;
        }
        let p = self.sel.head;
        if p.offset > 0 {
            self.checkpoint(UndoKind::Delete);
            if let Some(t) = self.text_mut(p.block) {
                let start = prev_char(&t.text, p.offset);
                t.replace(start..p.offset, &RichText::default());
                self.sel = DocSel::caret(DocPos::new(p.block, start));
            }
            return;
        }
        if p.block == TITLE_ID {
            return;
        }
        let Some(block) = self.block(p.block).cloned() else {
            return;
        };
        if !block.kind.has_text() {
            self.checkpoint(UndoKind::Other);
            self.delete_atomic(p.block);
            return;
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        match block.kind {
            DocBlockKind::Text => {}
            DocBlockKind::Code { .. } => {
                if block.text.is_empty()
                    && let Some(b) = self.block_mut(p.block)
                {
                    b.kind = DocBlockKind::Text;
                }
                return;
            }
            _ => {
                // Non-plain text block at offset 0 first degrades to paragraph.
                if let Some(b) = self.block_mut(p.block) {
                    b.kind = DocBlockKind::Text;
                }
                return;
            }
        }
        if self.parent_of(p.block).is_some_and(|pa| pa.kind.can_nest()) {
            self.outdent_one(p.block);
            return;
        }
        if let Some(parent) = self.parent_of(p.block).cloned()
            && parent.kind.is_container()
            && parent.children.first().map(|c| c.id) == Some(p.block)
        {
            if let Some(b) = self.remove_subtree(p.block) {
                self.insert_before(parent.id, b);
                self.normalize();
                return;
            }
        }
        self.merge_into_previous(p.block);
    }

    fn merge_into_previous(&mut self, id: u64) {
        let us = self.units();
        let Some(i) = us.iter().position(|u| u.id == id) else {
            return;
        };
        if !self.include_title && i == 0 {
            return;
        }
        if self.include_title && i <= 1 {
            // Right below TITLE_ID: move caret to end of title if current is empty (and has sibling below)
            if self.len_of(id) == 0 && us.len() > 2 {
                self.remove_lift(id);
                self.normalize();
            }
            self.sel = DocSel::caret(DocPos::new(TITLE_ID, self.doc.title.len()));
            return;
        }
        let prev = us[i - 1];
        if !prev.text {
            // Preceding block is atomic (Divider/Image/Table): remove it if empty, or focus it
            if self.len_of(id) == 0 {
                self.remove_lift(id);
                self.normalize();
                self.sel = DocSel::caret(DocPos::new(prev.id, 0));
            } else {
                self.delete_atomic(prev.id);
            }
            return;
        }
        let join_at = self.len_of(prev.id);
        let tail = self.text_of(id).cloned().unwrap_or_default();
        let tail = if self.is_code(prev.id) {
            RichText::plain(tail.text)
        } else {
            tail
        };
        if let Some(pt) = self.text_mut(prev.id) {
            pt.replace(join_at..join_at, &tail);
        }
        let kids = self
            .block_mut(id)
            .map(|b| std::mem::take(&mut b.children))
            .unwrap_or_default();
        self.remove_lift(id);
        if self.block(prev.id).is_some_and(|b| b.kind.can_nest()) {
            if let Some(pb) = self.block_mut(prev.id) {
                pb.children.extend(kids);
            }
        } else {
            self.insert_all_after(prev.id, kids);
        }
        self.normalize();
        self.sel = DocSel::caret(DocPos::new(prev.id, join_at));
    }

    pub fn delete_forward(&mut self) {
        if !self.sel.is_collapsed() || self.in_block_mode() {
            self.checkpoint(UndoKind::Delete);
            self.delete_selection_inner();
            return;
        }
        let p = self.sel.head;
        let len = self.len_of(p.block);
        if p.offset < len {
            self.checkpoint(UndoKind::Delete);
            if let Some(t) = self.text_mut(p.block) {
                let end = next_char(&t.text, p.offset);
                t.replace(p.offset..end, &RichText::default());
            }
            return;
        }
        let us = self.units();
        let Some(i) = us.iter().position(|u| u.id == p.block) else {
            return;
        };
        let Some(next) = us.get(i + 1) else {
            return;
        };
        self.checkpoint(UndoKind::Other);
        if !next.text {
            self.delete_atomic(next.id);
            self.sel = DocSel::caret(p);
        } else if p.block != TITLE_ID {
            self.merge_into_previous(next.id);
            self.sel = DocSel::caret(p);
        }
    }

    pub fn select_all(&mut self) {
        self.clear_block_sel();
        let us = self.units();
        let Some(last) = us.last() else {
            return;
        };
        let body_first = us
            .get(usize::from(self.include_title))
            .map(|u| u.id)
            .unwrap_or(TITLE_ID);
        let anchor_at_body = DocPos::new(body_first, 0);
        let head_at_end = DocPos::new(last.id, self.len_of(last.id));
        if self.include_title && self.sel.anchor == anchor_at_body && self.sel.head == head_at_end {
            self.sel = DocSel {
                anchor: DocPos::new(TITLE_ID, 0),
                head: head_at_end,
            };
        } else if self.sel.head.block == TITLE_ID && self.sel.is_collapsed() {
            self.sel = DocSel {
                anchor: DocPos::new(TITLE_ID, 0),
                head: DocPos::new(TITLE_ID, self.doc.title.len()),
            };
        } else {
            self.sel = DocSel {
                anchor: anchor_at_body,
                head: head_at_end,
            };
        }
    }

    pub fn move_h(&mut self, right: bool, word: bool, extend: bool) {
        self.clear_block_sel();
        if !extend && !self.sel.is_collapsed() {
            let (a, b) = self.ordered();
            self.sel = DocSel::caret(if right { b } else { a });
            return;
        }
        let h = self.sel.head;
        let us = self.units();
        let Some(i) = us.iter().position(|u| u.id == h.block) else {
            return;
        };
        if us[i].text {
            let text = self.text_of(h.block).map(|t| t.text.as_str()).unwrap_or("");
            if right && h.offset < text.len() {
                let o = if word {
                    next_word(text, h.offset)
                } else {
                    next_char(text, h.offset)
                };
                self.set_head(DocPos::new(h.block, o), extend);
                return;
            }
            if !right && h.offset > 0 {
                let o = if word {
                    prev_word(text, h.offset)
                } else {
                    prev_char(text, h.offset)
                };
                self.set_head(DocPos::new(h.block, o), extend);
                return;
            }
        }
        if right {
            if let Some(n) = us.get(i + 1) {
                self.set_head(DocPos::new(n.id, 0), extend);
            }
        } else if i > 0 {
            let prev = us[i - 1].id;
            self.set_head(DocPos::new(prev, self.len_of(prev)), extend);
        }
    }

    pub fn set_head(&mut self, p: DocPos, extend: bool) {
        self.clear_block_sel();
        if extend {
            self.sel.head = p;
        } else {
            self.sel = DocSel::caret(p);
        }
        self.pending_marks = None;
        self.preedit = None;
    }

    pub fn toggle_mark(&mut self, mark: DocMark) {
        if self.sel.is_collapsed() {
            let mut m = self.pending_marks.clone().unwrap_or_else(|| {
                self.text_of(self.sel.head.block)
                    .map(|t| t.marks_at(self.sel.head.offset))
                    .unwrap_or_default()
            });
            flip_mark(&mut m, mark);
            self.pending_marks = Some(m);
            return;
        }
        let (s, e) = self.ordered();
        if s.block == TITLE_ID && e.block == TITLE_ID {
            return;
        }
        self.checkpoint(UndoKind::Other);
        let turn_on = !self.selection_has_mark(mark);
        self.update_selected_marks(|m| set_mark(m, mark, turn_on));
    }

    pub fn selection_has_mark(&self, mark: DocMark) -> bool {
        let pred = |m: &TextMarks| match mark {
            DocMark::Bold => m.bold,
            DocMark::Italic => m.italic,
            DocMark::Underline => m.underline,
            DocMark::Strike => m.strike,
            DocMark::Code => m.code,
        };
        if self.sel.is_collapsed() {
            return self.pending_marks.as_ref().map(pred).unwrap_or_else(|| {
                self.text_of(self.sel.head.block)
                    .is_some_and(|t| pred(&t.marks_at(self.sel.head.offset)))
            });
        }
        let (s, e) = self.ordered();
        if s.block == e.block {
            return self
                .text_of(s.block)
                .is_some_and(|t| t.all_have(s.offset..e.offset, pred));
        }
        let us = self.units();
        let si = us.iter().position(|u| u.id == s.block).unwrap_or(0);
        let ei = us.iter().position(|u| u.id == e.block).unwrap_or(0);
        let mut any = false;
        for u in &us[si..=ei] {
            if u.id == TITLE_ID || !u.text || u.is_code {
                continue;
            }
            if let Some(t) = self.text_of(u.id) {
                let a = if u.id == s.block { s.offset } else { 0 };
                let b = if u.id == e.block { e.offset } else { t.len() };
                if a < b {
                    any = true;
                    if !t.all_have(a..b, pred) {
                        return false;
                    }
                }
            }
        }
        any
    }

    pub fn update_selected_marks(&mut self, mut f: impl FnMut(&mut TextMarks)) {
        if self.sel.is_collapsed() {
            let mut m = self.pending_marks.clone().unwrap_or_else(|| {
                self.text_of(self.sel.head.block)
                    .map(|t| t.marks_at(self.sel.head.offset))
                    .unwrap_or_default()
            });
            f(&mut m);
            self.pending_marks = Some(m);
            return;
        }
        self.checkpoint(UndoKind::Other);
        let (s, e) = self.ordered();
        let us = self.units();
        let si = us.iter().position(|u| u.id == s.block).unwrap_or(0);
        let ei = us.iter().position(|u| u.id == e.block).unwrap_or(0);
        for u in &us[si..=ei] {
            if u.id == TITLE_ID || !u.text || u.is_code {
                continue;
            }
            let len = self.len_of(u.id);
            let a = if u.id == s.block { s.offset } else { 0 };
            let b = if u.id == e.block { e.offset } else { len };
            if a < b
                && let Some(t) = self.text_mut(u.id)
            {
                t.update_marks(a..b, &mut f);
            }
        }
    }

    pub fn turn_into(&mut self, ids: &[u64], kind: DocBlockKind) {
        let targets: Vec<u64> = ids.iter().copied().filter(|id| *id != TITLE_ID).collect();
        if targets.is_empty() {
            return;
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        if kind.is_container() {
            for id in targets {
                if let Some(b) = self.block_mut(id)
                    && b.kind.is_container()
                {
                    b.kind = kind.clone();
                    continue;
                }
                let is_inside_same = self.parent_of(id).is_some_and(|p| {
                    std::mem::discriminant(&p.kind) == std::mem::discriminant(&kind)
                });
                if is_inside_same {
                    continue;
                }
                if let Some(b) = self.block(id).cloned() {
                    let container = self.doc.new_container(kind.clone(), b);
                    replace_block(&mut self.doc.blocks, id, container);
                }
            }
            self.normalize();
            self.fix_selection();
            return;
        }
        let mut text_targets = Vec::new();
        for id in targets {
            if let Some(b) = self.block(id)
                && b.kind.is_container()
            {
                text_targets.extend(
                    b.children
                        .iter()
                        .filter(|child| child.kind.has_text())
                        .map(|child| child.id),
                );
                remove_and_lift_in(&mut self.doc.blocks, id);
            } else {
                text_targets.push(id);
            }
        }
        for id in text_targets {
            if let Some(b) = self.block_mut(id) {
                if !b.kind.has_text() {
                    continue;
                }
                if matches!(kind, DocBlockKind::Code { .. }) {
                    b.text = RichText::plain(b.text.text.clone());
                }
                b.kind = kind.clone();
            }
        }
        self.normalize();
        self.fix_selection();
    }

    #[allow(dead_code)]
    pub fn set_align(&mut self, ids: &[u64], align: DocAlign) {
        self.checkpoint(UndoKind::Other);
        for &id in ids {
            if let Some(b) = self.block_mut(id) {
                b.align = align;
            }
        }
    }

    pub fn toggle_todo(&mut self, id: u64) {
        self.checkpoint(UndoKind::Other);
        if let Some(b) = self.block_mut(id)
            && let DocBlockKind::Todo { done } = &mut b.kind
        {
            *done = !*done;
        }
    }

    pub fn go_to_heading(&mut self, id: u64) {
        if !self.units().iter().any(|unit| unit.id == id) {
            self.checkpoint(UndoKind::Other);
            reveal_block(&mut self.doc.blocks, id);
        }
        self.break_coalescing();
        self.set_head(DocPos::new(id, 0), false);
    }

    pub fn toggle_fold(&mut self, id: u64) {
        self.checkpoint(UndoKind::Other);
        if let Some(b) = self.block_mut(id) {
            b.folded = !b.folded;
        }
        let visible = self.units();
        if [self.sel.anchor, self.sel.head]
            .iter()
            .any(|pos| !visible.iter().any(|u| u.id == pos.block))
        {
            self.set_head(DocPos::new(id, self.len_of(id)), false);
        }
    }

    pub fn indent_block(&mut self, id: u64) -> bool {
        if id == TITLE_ID {
            return false;
        }
        self.checkpoint(UndoKind::Other);
        let ok = indent_in_list(&mut self.doc.blocks, id);
        if ok {
            self.dirty = true;
        }
        ok
    }

    pub fn outdent_one(&mut self, id: u64) -> bool {
        if id == TITLE_ID {
            return false;
        }
        self.checkpoint(UndoKind::Other);
        let ok = outdent_in_list(&mut self.doc.blocks, id);
        if ok {
            self.dirty = true;
        }
        ok
    }

    pub fn insert_block_after(&mut self, after_id: u64, b: DocBlock) -> u64 {
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let nid = b.id;
        if after_id == TITLE_ID {
            self.doc.blocks.insert(0, b);
        } else {
            self.insert_after(after_id, b);
        }
        nid
    }

    /// Focuses an existing trailing empty paragraph, or appends one when clicking below the last block.
    pub fn ensure_trailing_empty_paragraph(&mut self) -> DocPos {
        if let Some(last) = self.doc.blocks.last()
            && matches!(last.kind, DocBlockKind::Text)
            && last.text.is_empty()
            && last.children.is_empty()
        {
            let id = last.id;
            if !self.units().iter().any(|u| u.id == id) {
                self.checkpoint(UndoKind::Other);
                reveal_block(&mut self.doc.blocks, id);
            }
            return DocPos::new(id, 0);
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let nb = self.doc.new_block(DocBlockKind::Text, "");
        let nid = nb.id;
        self.doc.blocks.push(nb);
        reveal_block(&mut self.doc.blocks, nid);
        self.dirty = true;
        DocPos::new(nid, 0)
    }

    /// Exits the current block (or enclosing Callout/Quote container) into a paragraph directly below.
    pub fn exit_block_below(&mut self) -> bool {
        let p = self.sel.head;
        if p.block == TITLE_ID {
            return false;
        }
        let target_id = if let Some(parent) = self.parent_of(p.block)
            && parent.kind.is_container()
        {
            parent.id
        } else {
            p.block
        };
        let mut out_pos = None;
        self.with_undo(|core| {
            let nb = core.doc.new_block(DocBlockKind::Text, "");
            let nid = nb.id;
            core.insert_after(target_id, nb);
            reveal_block(&mut core.doc.blocks, nid);
            core.sel = DocSel::caret(DocPos::new(nid, 0));
            out_pos = Some(nid);
        });
        out_pos.is_some()
    }

    fn siblings(&self, id: u64) -> Option<(Option<u64>, &[DocBlock], usize)> {
        if let Some(index) = self.doc.blocks.iter().position(|b| b.id == id) {
            return Some((None, &self.doc.blocks, index));
        }
        let parent = self.parent_of(id)?;
        let index = parent.children.iter().position(|b| b.id == id)?;
        Some((Some(parent.id), &parent.children, index))
    }

    fn siblings_mut(&mut self, parent: Option<u64>) -> &mut Vec<DocBlock> {
        match parent {
            Some(id) => &mut self.block_mut(id).expect("validated parent").children,
            None => &mut self.doc.blocks,
        }
    }

    pub fn move_block(&mut self, src_id: u64, target_id: u64, after: bool) -> bool {
        let Some((from, siblings, start)) = self.siblings(src_id) else {
            return false;
        };
        let end = section_end(siblings, start);
        if find_block(&siblings[start..end], target_id).is_some() {
            return false;
        }
        let moved_blocks = &siblings[start..end];
        let keep_selection = [self.sel.anchor, self.sel.head]
            .iter()
            .all(|p| find_block(moved_blocks, p.block).is_some());
        let mut focus = &siblings[start];
        while focus.kind.is_container() {
            let Some(child) = focus.children.first() else {
                return false;
            };
            focus = child;
        }
        let pos = DocPos::new(focus.id, focus.text.len());
        let Some((to, destination, target)) = self.siblings(target_id) else {
            return false;
        };
        let mut insert = target + usize::from(after);
        if let DocBlockKind::Heading { level } = siblings[start].kind
            && !section_fits_before(level, destination.get(insert))
        {
            return false;
        }
        if from == to {
            if insert == start || insert == end {
                return false;
            }
            if insert > end {
                insert -= end - start;
            }
        }
        self.checkpoint(UndoKind::Other);
        let moved: Vec<_> = self.siblings_mut(from).drain(start..end).collect();
        self.siblings_mut(to).splice(insert..insert, moved);
        reveal_block(&mut self.doc.blocks, src_id);
        if !keep_selection {
            self.set_head(pos, false);
        } else {
            self.clear_block_sel();
        }
        self.normalize();
        self.fix_selection();
        true
    }

    pub fn duplicate_block(&mut self, id: u64) -> Option<u64> {
        let mut clone = self.block(id)?.clone();
        reassign_ids(&mut clone, &mut self.doc);
        let nid = clone.id;
        self.checkpoint(UndoKind::Other);
        self.insert_after(id, clone);
        Some(nid)
    }

    pub fn delete_block(&mut self, id: u64) {
        if id == TITLE_ID {
            return;
        }
        self.checkpoint(UndoKind::Other);
        let us = self.units();
        let ix = us.iter().position(|u| u.id == id);
        self.remove_subtree(id);
        self.normalize();
        if let Some(i) = ix {
            let next = us
                .get(i + 1)
                .or_else(|| i.checked_sub(1).and_then(|k| us.get(k)));
            if let Some(u) = next {
                self.sel = DocSel::caret(DocPos::new(u.id, 0));
            }
        }
        self.fix_selection();
    }

    /// Paste literal text; decoding external formats belongs to the adapter.
    pub fn paste_text(&mut self, raw: &str) {
        let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() {
            return;
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let p = self.delete_selection_inner();
        let clean = if p.block == TITLE_ID {
            normalized.replace('\n', " ")
        } else {
            normalized
        };
        let marks = if self.is_code(p.block) || p.block == TITLE_ID {
            TextMarks::default()
        } else {
            self.text_of(p.block)
                .map(|t| t.marks_at(p.offset))
                .unwrap_or_default()
        };
        if let Some(t) = self.text_mut(p.block) {
            t.insert_plain(p.offset, &clean, marks);
        }
        self.sel = DocSel::caret(DocPos::new(p.block, p.offset + clean.len()));
    }

    /// Insert decoded content as a single undoable edit. IDs are allocated by the caller
    /// from this document before insertion; UI and clipboard types never enter the core.
    pub fn paste_blocks(&mut self, blocks: Vec<DocBlock>) {
        if blocks.is_empty() {
            return;
        }
        self.checkpoint(UndoKind::Other);
        self.break_coalescing();
        let p = self.delete_selection_inner();
        self.paste_parsed(p, blocks);
    }

    fn paste_parsed(&mut self, p: DocPos, mut parsed: Vec<DocBlock>) {
        if parsed.is_empty() {
            return;
        }
        let tail = self
            .text_mut(p.block)
            .map(|t| t.split_off(p.offset))
            .unwrap_or_default();
        let first = parsed.remove(0);
        if matches!(first.kind, DocBlockKind::Text) && first.children.is_empty() {
            if let Some(t) = self.text_mut(p.block) {
                let at = t.len();
                t.replace(at..at, &first.text);
            }
            if parsed.is_empty() {
                if let Some(t) = self.text_mut(p.block) {
                    let end_at = t.len();
                    t.replace(end_at..end_at, &tail);
                    self.sel = DocSel::caret(DocPos::new(p.block, end_at));
                }
                return;
            }
            let last_id = parsed.last().unwrap().id;
            self.insert_all_after(p.block, parsed);
            let append_at = self.len_of(last_id);
            if !tail.is_empty()
                && let Some(lt) = self.text_mut(last_id)
            {
                lt.replace(append_at..append_at, &tail);
            }
            self.sel = DocSel::caret(DocPos::new(last_id, append_at));
        } else {
            parsed.insert(0, first);
            let mut anchor = p.block;
            let mut last_id = anchor;
            for b in parsed {
                last_id = b.id;
                self.insert_after(anchor, b);
                anchor = last_id;
            }
            let append_at = self.len_of(last_id);
            if !tail.is_empty()
                && let Some(lt) = self.text_mut(last_id)
            {
                lt.replace(append_at..append_at, &tail);
            }
            self.sel = DocSel::caret(DocPos::new(last_id, append_at));
        }
    }

    fn insert_after(&mut self, id: u64, b: DocBlock) {
        if !insert_sibling(&mut self.doc.blocks, id, b.clone(), true) {
            self.doc.blocks.push(b);
        }
    }

    fn insert_all_after(&mut self, mut anchor: u64, blocks: Vec<DocBlock>) {
        for b in blocks {
            let nid = b.id;
            self.insert_after(anchor, b);
            anchor = nid;
        }
    }

    fn insert_before(&mut self, id: u64, b: DocBlock) {
        if !insert_sibling(&mut self.doc.blocks, id, b.clone(), false) {
            self.doc.blocks.insert(0, b);
        }
    }

    fn remove_subtree(&mut self, id: u64) -> Option<DocBlock> {
        remove_block(&mut self.doc.blocks, id)
    }

    fn remove_lift(&mut self, id: u64) {
        remove_and_lift_in(&mut self.doc.blocks, id);
    }
}

/// Exclusive sibling boundary shared by folding and section moves.
pub fn section_end(list: &[DocBlock], start: usize) -> usize {
    let DocBlockKind::Heading { level } = list[start].kind else {
        return start + 1;
    };
    let rest = &list[start + 1..];
    start
        + 1
        + rest
            .iter()
            .position(|b| matches!(b.kind, DocBlockKind::Heading { level: next } if next <= level))
            .unwrap_or(rest.len())
}

/// A moved heading must not absorb the destination's following body or subheadings.
pub fn section_fits_before(level: u8, next: Option<&DocBlock>) -> bool {
    next.is_none_or(|b| matches!(b.kind, DocBlockKind::Heading { level: n } if n <= level))
}

/// Open only the folded sections that contain this block.
fn reveal_block(list: &mut [DocBlock], id: u64) -> bool {
    let mut i = 0;
    while i < list.len() {
        let end = section_end(list, i);
        if list[i].id == id {
            return true;
        }
        if reveal_block(&mut list[i].children, id) || reveal_block(&mut list[i + 1..end], id) {
            if matches!(list[i].kind, DocBlockKind::Heading { .. }) {
                list[i].folded = false;
            }
            return true;
        }
        i = end;
    }
    false
}

fn find_block(list: &[DocBlock], id: u64) -> Option<&DocBlock> {
    for b in list {
        if b.id == id {
            return Some(b);
        }
        if let Some(hit) = find_block(&b.children, id) {
            return Some(hit);
        }
    }
    None
}

fn find_block_mut(list: &mut [DocBlock], id: u64) -> Option<&mut DocBlock> {
    for b in list {
        if b.id == id {
            return Some(b);
        }
        if let Some(hit) = find_block_mut(&mut b.children, id) {
            return Some(hit);
        }
    }
    None
}

fn find_parent(list: &[DocBlock], id: u64) -> Option<&DocBlock> {
    for b in list {
        if b.children.iter().any(|c| c.id == id) {
            return Some(b);
        }
        if let Some(hit) = find_parent(&b.children, id) {
            return Some(hit);
        }
    }
    None
}

fn insert_sibling(list: &mut Vec<DocBlock>, target: u64, b: DocBlock, after: bool) -> bool {
    if let Some(idx) = list.iter().position(|item| item.id == target) {
        list.insert(if after { idx + 1 } else { idx }, b);
        return true;
    }
    for item in list.iter_mut() {
        if insert_sibling(&mut item.children, target, b.clone(), after) {
            return true;
        }
    }
    false
}

fn remove_block(list: &mut Vec<DocBlock>, id: u64) -> Option<DocBlock> {
    if let Some(idx) = list.iter().position(|item| item.id == id) {
        return Some(list.remove(idx));
    }
    for item in list.iter_mut() {
        if let Some(removed) = remove_block(&mut item.children, id) {
            return Some(removed);
        }
    }
    None
}

fn remove_and_lift_in(list: &mut Vec<DocBlock>, id: u64) -> bool {
    if let Some(idx) = list.iter().position(|item| item.id == id) {
        let mut b = list.remove(idx);
        let kids = std::mem::take(&mut b.children);
        list.splice(idx..idx, kids);
        return true;
    }
    for item in list.iter_mut() {
        if remove_and_lift_in(&mut item.children, id) {
            return true;
        }
    }
    false
}

fn replace_block(list: &mut Vec<DocBlock>, id: u64, replacement: DocBlock) -> bool {
    for item in list.iter_mut() {
        if item.id == id {
            *item = replacement;
            return true;
        }
        if replace_block(&mut item.children, id, replacement.clone()) {
            return true;
        }
    }
    false
}

fn indent_in_list(list: &mut Vec<DocBlock>, id: u64) -> bool {
    if let Some(idx) = list.iter().position(|b| b.id == id) {
        if idx == 0 || !list[idx - 1].kind.can_nest() {
            return false;
        }
        let b = list.remove(idx);
        list[idx - 1].children.push(b);
        return true;
    }
    for item in list.iter_mut() {
        if indent_in_list(&mut item.children, id) {
            return true;
        }
    }
    false
}

fn outdent_in_list(list: &mut Vec<DocBlock>, id: u64) -> bool {
    for idx in 0..list.len() {
        if let Some(cidx) = list[idx].children.iter().position(|c| c.id == id) {
            if !list[idx].kind.can_nest() {
                return false;
            }
            let b = list[idx].children.remove(cidx);
            list.insert(idx + 1, b);
            return true;
        }
        if outdent_in_list(&mut list[idx].children, id) {
            return true;
        }
    }
    false
}

fn reassign_ids(b: &mut DocBlock, doc: &mut DocDocument) {
    b.id = doc.alloc_id();
    for c in &mut b.children {
        reassign_ids(c, doc);
    }
}

fn clamp_boundary(s: &str, mut byte: usize) -> usize {
    byte = byte.min(s.len());
    while byte > 0 && !s.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn prev_char(s: &str, offset: usize) -> usize {
    s[..offset.min(s.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(i, _)| i)
}

fn next_char(s: &str, offset: usize) -> usize {
    let at = offset.min(s.len());
    s[at..]
        .chars()
        .next()
        .map_or(s.len(), |c| at + c.len_utf8())
}

fn prev_word(s: &str, offset: usize) -> usize {
    let mut i = prev_char(s, offset);
    while i > 0 {
        let p = prev_char(s, i);
        let ch = s[p..i].chars().next().unwrap_or(' ');
        if ch.is_whitespace() || is_word_punct(ch) {
            break;
        }
        i = p;
    }
    i
}

fn next_word(s: &str, offset: usize) -> usize {
    let mut i = next_char(s, offset);
    while i < s.len() {
        let n = next_char(s, i);
        let ch = s[i..n].chars().next().unwrap_or(' ');
        if ch.is_whitespace() || is_word_punct(ch) {
            break;
        }
        i = n;
    }
    i
}

pub fn clamp_utf8(s: &str, offset: usize) -> usize {
    clamp_boundary(s, offset)
}

pub fn word_at(s: &str, offset: usize) -> std::ops::Range<usize> {
    let at = clamp_boundary(s, offset);
    if s.is_empty() {
        return 0..0;
    }
    let mut start = at;
    while start > 0 {
        let p = prev_char(s, start);
        let ch = s[p..start].chars().next().unwrap();
        if ch.is_whitespace() || is_word_punct(ch) {
            break;
        }
        start = p;
    }
    let mut end = at;
    while end < s.len() {
        let n = next_char(s, end);
        let ch = s[end..n].chars().next().unwrap();
        if ch.is_whitespace() || is_word_punct(ch) {
            break;
        }
        end = n;
    }
    if start == end && at < s.len() {
        at..next_char(s, at)
    } else {
        start..end
    }
}

fn is_word_punct(c: char) -> bool {
    matches!(
        c,
        ',' | '.'
            | ';'
            | ':'
            | '!'
            | '?'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '/'
            | '，'
            | '。'
            | '；'
            | '：'
            | '！'
            | '？'
            | '、'
            | '（'
            | '）'
            | '【'
            | '】'
            | '《'
            | '》'
    )
}

fn flip_mark(m: &mut TextMarks, mark: DocMark) {
    match mark {
        DocMark::Bold => m.bold = !m.bold,
        DocMark::Italic => m.italic = !m.italic,
        DocMark::Underline => m.underline = !m.underline,
        DocMark::Strike => m.strike = !m.strike,
        DocMark::Code => m.code = !m.code,
    }
}

fn set_mark(m: &mut TextMarks, mark: DocMark, on: bool) {
    match mark {
        DocMark::Bold => m.bold = on,
        DocMark::Italic => m.italic = on,
        DocMark::Underline => m.underline = on,
        DocMark::Strike => m.strike = on,
        DocMark::Code => m.code = on,
    }
}

#[cfg(test)]
pub fn seed_documents() -> Vec<(String, Option<usize>, DocDocument)> {
    // 0 = root Welcome doc, 1 = child Spec doc (parent index 0)
    let mut welcome = DocDocument::empty();
    welcome.title = RichText::plain("欢迎使用 Echo 结构化文档");
    welcome.blocks.clear();

    let mut intro = RichText::default();
    intro.push("这是基于 ", TextMarks::default());
    intro.push(
        "统一块级文档内核",
        TextMarks {
            bold: true,
            ..Default::default()
        },
    );
    intro.push(
        " 打造的沉浸式整页编辑体验：回车自然分段、跨任意块拖选、块首 Backspace 先降级样式再合并。",
        TextMarks::default(),
    );
    let mut b_intro = welcome.new_block(DocBlockKind::Text, "");
    b_intro.text = intro;

    let mut c_child = welcome.new_block(
        DocBlockKind::Text,
        "试试用鼠标从上方文字一路拖选穿过下面的代码块和高亮块，或者输入 / 与 、 快速插入模块。",
    );
    c_child.text.update_marks(0..0, |_| {});
    let b_callout = welcome.new_container(
        DocBlockKind::Callout {
            emoji: "✨".into(),
            palette: "blue".into(),
        },
        c_child,
    );

    let b_h1 = welcome.new_block(DocBlockKind::Heading { level: 1 }, "核心写作与排版能力");
    let b_t1 = welcome.new_block(
        DocBlockKind::Todo { done: true },
        "统一选区与跨块鼠标拖选 / Cmd+A",
    );
    let b_t2 = welcome.new_block(
        DocBlockKind::Todo { done: false },
        "支持 #、-、1.、[]、>、---、``` 等 Markdown 快捷输入",
    );
    let b_t3 = welcome.new_block(
        DocBlockKind::Todo { done: false },
        "中文输入法光标紧贴预编辑文本与悬浮格式工具栏",
    );
    let b_div = welcome.new_block(DocBlockKind::Divider, "");
    let b_h2 = welcome.new_block(DocBlockKind::Heading { level: 2 }, "代码示例");
    let b_code = welcome.new_block(
        DocBlockKind::Code {
            lang: "Rust".into(),
            wrap: true,
        },
        "fn handle_enter(core: &mut DocCore) {\n    core.enter(); // 标题切正文、列表续编、代码块保留缩进\n}",
    );
    welcome.blocks = vec![
        b_intro, b_callout, b_h1, b_t1, b_t2, b_t3, b_div, b_h2, b_code,
    ];

    let mut subdoc = DocDocument::empty();
    subdoc.title = RichText::plain("快捷键与斜杠指令指南");
    subdoc.blocks = vec![
        subdoc.new_block(DocBlockKind::Heading { level: 2 }, "常用快捷方式"),
        subdoc.new_block(DocBlockKind::Bullet, "输入 / 或 、 唤起模块菜单"),
        subdoc.new_block(DocBlockKind::Bullet, "Tab / Shift+Tab 调整段落与列表缩进"),
        subdoc.new_block(DocBlockKind::Bullet, "Shift+Enter 在同一段落内软换行"),
        subdoc.new_block(
            DocBlockKind::Ordered { start: None },
            "选中文字弹出悬浮工具栏，支持 B / I / U / S / Code / 高亮色",
        ),
        subdoc.new_block(
            DocBlockKind::Ordered { start: None },
            "鼠标悬停左侧出现 + 与 ⠿ 手柄，打字时自动隐藏",
        ),
    ];

    vec![
        ("工作".to_string(), None, welcome),
        ("工作".to_string(), Some(0), subdoc),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_splits_paragraph_without_losing_selection_continuity() {
        let mut core = DocCore::new(DocDocument::empty());
        core.insert_text("HelloWorld");
        let first_id = core.doc.blocks[0].id;
        core.sel = DocSel::caret(DocPos::new(first_id, 5));
        core.enter();
        assert_eq!(core.doc.blocks.len(), 2);
        assert_eq!(core.doc.blocks[0].text.text, "Hello");
        assert_eq!(core.doc.blocks[1].text.text, "World");
        let second_id = core.doc.blocks[1].id;

        // Cross-block selection across first, divider, and second block deletes seamlessly
        core.sel = DocSel {
            anchor: DocPos::new(first_id, 2),
            head: DocPos::new(second_id, 3),
        };
        assert_eq!(core.selected_text(), "llo\nWor");
        core.delete_selection();
        assert_eq!(core.doc.blocks.len(), 1);
        assert_eq!(core.doc.blocks[0].text.text, "Held");
    }

    #[test]
    fn backspace_at_heading_start_degrades_to_text_before_merging() {
        let mut core = DocCore::new(DocDocument::empty());
        core.insert_text("First");
        core.enter();
        core.insert_text("#");
        core.insert_text(" ");
        core.insert_text("Title");
        let h_id = core.doc.blocks[1].id;
        assert!(matches!(
            core.doc.blocks[1].kind,
            DocBlockKind::Heading { level: 1 }
        ));

        core.sel = DocSel::caret(DocPos::new(h_id, 0));
        core.backspace();
        // Step 1: Heading degrades to Text
        assert_eq!(core.doc.blocks.len(), 2);
        assert!(matches!(core.doc.blocks[1].kind, DocBlockKind::Text));

        // Step 2: Second Backspace merges into First
        core.backspace();
        assert_eq!(core.doc.blocks.len(), 1);
        assert_eq!(core.doc.blocks[0].text.text, "FirstTitle");
    }

    #[test]
    fn cross_heterogeneous_blocks_select_and_delete() {
        let mut core = DocCore::new(DocDocument::empty());
        core.insert_text("Alpha");
        core.enter();
        core.insert_text("---");
        assert!(matches!(core.doc.blocks[1].kind, DocBlockKind::Divider));
        core.insert_text("Beta");
        let a_id = core.doc.blocks[0].id;
        let b_id = core.doc.blocks[2].id;
        core.sel = DocSel {
            anchor: DocPos::new(a_id, 2),
            head: DocPos::new(b_id, 2),
        };
        core.delete_selection();
        assert_eq!(core.doc.blocks.len(), 1);
        assert_eq!(core.doc.blocks[0].text.text, "Alta");
    }
}
