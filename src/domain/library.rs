//! Persisted library data; independent of the application and Rune.
use crate::identity::uid;
use serde::{Deserialize, Serialize};

pub const BOOKS: [&str; 7] = ["工作", "产品", "设计", "学习", "生活", "灵感", "回收站"];
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Kind {
    Text,
    Tasks,
    Image,
    Code,
    Handwritten,
    Callout,
    Shape,
    Table,
    Divider,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Block {
    pub id: String,
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub min_h: f32,
    pub title: String,
    pub body: String,
    pub image: String,
    #[serde(default)]
    pub body_html: String,
    #[serde(default)]
    pub structured: bool,
    #[serde(default)]
    pub text_align: crate::doc_core::DocAlign,
    #[serde(default)]
    pub table: Option<crate::table_data::TableData>,
}
impl Block {
    pub fn is_textual(&self) -> bool {
        matches!(
            self.kind,
            Kind::Text | Kind::Tasks | Kind::Code | Kind::Callout | Kind::Table
        )
    }

    pub fn new(kind: Kind, x: f32, y: f32, w: f32, h: f32, title: &str, body: &str) -> Self {
        Self {
            id: uid(),
            kind,
            x,
            y,
            w,
            h,
            min_h: 0.,
            title: title.into(),
            body: body.into(),
            image: String::new(),
            body_html: String::new(),
            structured: false,
            text_align: crate::doc_core::DocAlign::Left,
            table: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Note {
    pub id: String,
    pub notebook: String,
    pub original_book: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub title: String,
    pub preview: String,
    pub stamp: String,
    pub updated: i64,
    pub blocks: Vec<Block>,
    pub zoom: f32,
    pub pan: [f32; 2],
    #[serde(default)]
    pub editor_version: u8,
    #[serde(default)]
    pub format: crate::doc_core::NoteFormat,
    #[serde(default)]
    pub doc: Option<crate::doc_core::DocDocument>,
}
