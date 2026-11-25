//! Table payload shared by documents and persistence; no UI state.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableData {
    pub rows: Vec<Vec<String>>,
    #[serde(default)]
    pub widths: Vec<f32>,
    #[serde(default)]
    pub align: Vec<u8>,
}
impl Default for TableData {
    fn default() -> Self {
        Self {
            rows: vec![vec![String::new(); 3]; 3],
            widths: vec![1.; 3],
            align: vec![0; 3],
        }
    }
}
impl TableData {
    pub fn cols(&self) -> usize {
        self.rows.first().map_or(1, |r| r.len().max(1))
    }
}
