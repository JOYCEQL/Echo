//! Vector artwork and saved themes for notebook icons.
use rune::{El, Icon, icon};
use crate::{doc_core::NoteFormat, icons::{FILE_TEXT, LAYOUT_GRID, ico}, model::State, ui::{c, theme_ink}};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style { #[default] Solid, Pals, Color, Ink, Journal, Monochrome }

impl Style {
    pub fn key(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Pals => "pals",
            Self::Color => "color",
            Self::Ink => "ink",
            Self::Journal => "journal",
            Self::Monochrome => "monochrome",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "纯色极简",
            Self::Pals => "笔记搭子",
            Self::Color => "彩色实心",
            Self::Ink => "彩色手绘",
            Self::Journal => "装帧手账",
            Self::Monochrome => "单色线稿",
        }
    }

    pub fn from_preference(value: Option<&str>) -> Self {
        match value {
            None | Some("solid") => Self::Solid,
            Some("paper" | "pals") => Self::Pals,
            Some("color") => Self::Color,
            Some("ink") => Self::Ink,
            Some("journal") => Self::Journal,
            Some("monochrome") => Self::Monochrome,
            Some(value) => panic!("invalid notebook icon style: {value}"),
        }
    }

    pub fn page_icon(self, format: NoteFormat, size: f32, color: u32) -> El {
        match (self, format) {
            (Self::Solid, NoteFormat::Document) => ico(&SOLID_DOCUMENT, size).color(c(color)),
            (Self::Solid, NoteFormat::Canvas) => ico(&SOLID_CANVAS, size).color(c(color)),
            (Self::Monochrome, NoteFormat::Document) => outline_icon(&MONO_DOCUMENT, size, color),
            (Self::Monochrome, NoteFormat::Canvas) => outline_icon(&MONO_CANVAS, size, color),
            (_, NoteFormat::Document) => ico(&FILE_TEXT, size).color(c(color)),
            (_, NoteFormat::Canvas) => ico(&LAYOUT_GRID, size).color(c(color)),
        }
    }
}

// Preserve the surface's light or dark foreground level with a neutral pencil gray.
fn outline_icon(glyph: &'static Icon, size: f32, muted: u32) -> El {
    let gray = (muted >> 8) & 0xff;
    ico(glyph, size).color(c(gray * 0x010101))
}

static SOLID_WORK: Icon = Icon { name: "echo-solid-work", view: 24., body: include_str!("../assets/notebooks/solid/work.svg") };
static SOLID_PRODUCT: Icon = Icon { name: "echo-solid-product", view: 24., body: include_str!("../assets/notebooks/solid/product.svg") };
static SOLID_DESIGN: Icon = Icon { name: "echo-solid-design", view: 24., body: include_str!("../assets/notebooks/solid/design.svg") };
static SOLID_STUDY: Icon = Icon { name: "echo-solid-study", view: 24., body: include_str!("../assets/notebooks/solid/study.svg") };
static SOLID_LIFE: Icon = Icon { name: "echo-solid-life", view: 24., body: include_str!("../assets/notebooks/solid/life.svg") };
static SOLID_IDEAS: Icon = Icon { name: "echo-solid-ideas", view: 24., body: include_str!("../assets/notebooks/solid/ideas.svg") };
static SOLID_NOTEBOOK: Icon = Icon { name: "echo-solid-notebook", view: 24., body: include_str!("../assets/notebooks/solid/notebook.svg") };
static SOLID_DOCUMENT: Icon = Icon { name: "echo-solid-document", view: 24., body: include_str!("../assets/notebooks/solid/document.svg") };
static SOLID_CANVAS: Icon = Icon { name: "echo-solid-canvas", view: 24., body: include_str!("../assets/notebooks/solid/canvas.svg") };

fn solid_glyph(book: &str) -> &'static Icon {
    match book { "工作" => &SOLID_WORK, "产品" => &SOLID_PRODUCT, "设计" => &SOLID_DESIGN,
        "学习" => &SOLID_STUDY, "生活" => &SOLID_LIFE, "灵感" => &SOLID_IDEAS, _ => &SOLID_NOTEBOOK }
}

static MONO_FOLDER: Icon = Icon { name: "echo-mono-folder", view: 24., body: include_str!("../assets/notebooks/monochrome/folder.svg") };
static MONO_FOLDER_OPEN: Icon = Icon { name: "echo-mono-folder-open", view: 24., body: include_str!("../assets/notebooks/monochrome/folder-open.svg") };
static MONO_DOCUMENT: Icon = Icon { name: "echo-mono-document", view: 24., body: include_str!("../assets/notebooks/monochrome/document.svg") };
static MONO_CANVAS: Icon = Icon { name: "echo-mono-canvas", view: 24., body: include_str!("../assets/notebooks/monochrome/canvas.svg") };

pub static PRODUCT: Icon = Icon { name: "echo-notebook-color-product", view: 32., body: include_str!("../assets/notebooks/product.svg") };
pub static WORK: Icon = Icon { name: "echo-notebook-color-work", view: 32., body: include_str!("../assets/notebooks/work.svg") };
pub static DESIGN: Icon = Icon { name: "echo-notebook-color-design", view: 32., body: include_str!("../assets/notebooks/design.svg") };
pub static STUDY: Icon = Icon { name: "echo-notebook-color-study", view: 32., body: include_str!("../assets/notebooks/study.svg") };
pub static LIFE: Icon = Icon { name: "echo-notebook-color-life", view: 32., body: include_str!("../assets/notebooks/life.svg") };
pub static IDEAS: Icon = Icon { name: "echo-notebook-color-ideas", view: 32., body: include_str!("../assets/notebooks/ideas.svg") };
pub static NOTEBOOK: Icon = Icon { name: "echo-notebook-color-notebook", view: 32., body: include_str!("../assets/notebooks/notebook.svg") };

// Original colored ink illustrations: flat pastel fills, dark contours, no tile.
pub static INK_PRODUCT: Icon = Icon { name: "echo-notebook-ink-product", view: 32., body: include_str!("../assets/notebooks/ink/product.svg") };
pub static INK_WORK: Icon = Icon { name: "echo-notebook-ink-work", view: 32., body: include_str!("../assets/notebooks/ink/work.svg") };
pub static INK_DESIGN: Icon = Icon { name: "echo-notebook-ink-design", view: 32., body: include_str!("../assets/notebooks/ink/design.svg") };
pub static INK_STUDY: Icon = Icon { name: "echo-notebook-ink-study", view: 32., body: include_str!("../assets/notebooks/ink/study.svg") };
pub static INK_LIFE: Icon = Icon { name: "echo-notebook-ink-life", view: 32., body: include_str!("../assets/notebooks/ink/life.svg") };
pub static INK_IDEAS: Icon = Icon { name: "echo-notebook-ink-ideas", view: 32., body: include_str!("../assets/notebooks/ink/ideas.svg") };
pub static INK_NOTEBOOK: Icon = Icon { name: "echo-notebook-ink-notebook", view: 32., body: include_str!("../assets/notebooks/ink/notebook.svg") };

static PALS_WORK: Icon = Icon { name: "echo-notebook-pals-work", view: 32., body: include_str!("../assets/notebooks/pals/work.svg") };
static PALS_PRODUCT: Icon = Icon { name: "echo-notebook-pals-product", view: 32., body: include_str!("../assets/notebooks/pals/product.svg") };
static PALS_DESIGN: Icon = Icon { name: "echo-notebook-pals-design", view: 32., body: include_str!("../assets/notebooks/pals/design.svg") };
static PALS_STUDY: Icon = Icon { name: "echo-notebook-pals-study", view: 32., body: include_str!("../assets/notebooks/pals/study.svg") };
static PALS_LIFE: Icon = Icon { name: "echo-notebook-pals-life", view: 32., body: include_str!("../assets/notebooks/pals/life.svg") };
static PALS_IDEAS: Icon = Icon { name: "echo-notebook-pals-ideas", view: 32., body: include_str!("../assets/notebooks/pals/ideas.svg") };
static PALS_NOTEBOOK: Icon = Icon { name: "echo-notebook-pals-notebook", view: 32., body: include_str!("../assets/notebooks/pals/notebook.svg") };

fn pals_glyph(book: &str) -> &'static Icon {
    match book { "工作" => &PALS_WORK, "产品" => &PALS_PRODUCT, "设计" => &PALS_DESIGN,
        "学习" => &PALS_STUDY, "生活" => &PALS_LIFE, "灵感" => &PALS_IDEAS, _ => &PALS_NOTEBOOK }
}

pub static JOURNAL_WORK: Icon = Icon { name: "echo-notebook-journal-work", view: 32., body: include_str!("../assets/notebooks/journal/work.svg") };
pub static JOURNAL_PRODUCT: Icon = Icon { name: "echo-notebook-journal-product", view: 32., body: include_str!("../assets/notebooks/journal/product.svg") };
pub static JOURNAL_DESIGN: Icon = Icon { name: "echo-notebook-journal-design", view: 32., body: include_str!("../assets/notebooks/journal/design.svg") };
pub static JOURNAL_STUDY: Icon = Icon { name: "echo-notebook-journal-study", view: 32., body: include_str!("../assets/notebooks/journal/study.svg") };
pub static JOURNAL_LIFE: Icon = Icon { name: "echo-notebook-journal-life", view: 32., body: include_str!("../assets/notebooks/journal/life.svg") };
pub static JOURNAL_IDEAS: Icon = Icon { name: "echo-notebook-journal-ideas", view: 32., body: include_str!("../assets/notebooks/journal/ideas.svg") };
pub static JOURNAL_NOTEBOOK: Icon = Icon { name: "echo-notebook-journal-notebook", view: 32., body: include_str!("../assets/notebooks/journal/notebook.svg") };

fn journal_glyph(book: &str) -> &'static Icon {
    match book { "工作" => &JOURNAL_WORK, "产品" => &JOURNAL_PRODUCT, "设计" => &JOURNAL_DESIGN,
        "学习" => &JOURNAL_STUDY, "生活" => &JOURNAL_LIFE, "灵感" => &JOURNAL_IDEAS, _ => &JOURNAL_NOTEBOOK }
}

pub fn ink_glyph(book: &str) -> &'static Icon {
    match book { "工作" => &INK_WORK, "产品" => &INK_PRODUCT, "设计" => &INK_DESIGN,
        "学习" => &INK_STUDY, "生活" => &INK_LIFE, "灵感" => &INK_IDEAS, _ => &INK_NOTEBOOK }
}

pub fn glyph(book: &str) -> &'static Icon {
    match book { "工作" => &WORK, "产品" => &PRODUCT, "设计" => &DESIGN,
        "学习" => &STUDY, "生活" => &LIFE, "灵感" => &IDEAS, _ => &NOTEBOOK }
}
pub fn render(s: &State, book: &str, size: f32, open: bool, muted: u32) -> El {
    match s.notebook_icon_style {
        Style::Solid => {
            let color = if open { crate::ui::nav_ink(s) } else { crate::ui::nav_subtle(s) };
            let glyph_size = (size * 0.72).clamp(16., 22.);
            rune::el().size(size, size).center().inert().child(
                ico(solid_glyph(book), glyph_size).color(c(color))
            )
        }
        Style::Monochrome => rune::el().size(size, size).center().inert().child(
            outline_icon(if open { &MONO_FOLDER_OPEN } else { &MONO_FOLDER }, size.min(21.), muted)
        ),
        Style::Pals => icon(pals_glyph(book), size).inert(),
        Style::Journal => icon(journal_glyph(book), size).inert(),
        Style::Color => icon(glyph(book), size).inert(),
        Style::Ink => icon(ink_glyph(book), size)
            .color(c(theme_ink(s.theme()))).inert(),
    }
}
