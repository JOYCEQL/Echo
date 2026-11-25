//! Shared, embedded hand-drawn page artwork. Decode once using the app's image cache.
use rune::{El, ImageCache, Pixels, el};

#[derive(Default)]
pub struct PageIcons {
    document: Pixels,
    canvas: Pixels,
}

impl PageIcons {
    pub fn load(&mut self, cache: &ImageCache) {
        self.document = cache
            .load(include_bytes!("../assets/pages/document.png").as_slice())
            .pixels().clone();
        self.canvas = cache
            .load(include_bytes!("../assets/pages/canvas.png").as_slice())
            .pixels().clone();
    }

    pub fn render(&self, format: crate::doc_core::NoteFormat, size: f32) -> El {
        let pixels = match format {
            crate::doc_core::NoteFormat::Document => &self.document,
            crate::doc_core::NoteFormat::Canvas => &self.canvas,
        };
        el().size(size, size).min_w(size).min_h(size).pixels(pixels).inert()
    }
}
