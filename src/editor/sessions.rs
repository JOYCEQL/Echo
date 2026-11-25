//! Owns the one active editor and suspended card sessions. Hosts never swap fields themselves.
use crate::{
    doc_core::{DocBlock, DocDocument},
    doc_editor::DocSession,
};
use std::collections::HashMap;

#[derive(Default)]
enum Host {
    #[default]
    Empty,
    Document,
    Canvas(String),
}

#[derive(Default)]
pub struct Sessions {
    pub active: DocSession,
    cards: HashMap<String, DocSession>,
    host: Host,
    next_namespace: u64,
}

impl Sessions {
    #[cfg(test)]
    pub fn mounted_cards(&self) -> usize {
        self.cards.len() + usize::from(self.canvas_owner().is_some())
    }
    pub fn canvas_owner(&self) -> Option<&str> {
        match &self.host {
            Host::Canvas(id) => Some(id),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.active = DocSession::default();
        self.cards.clear();
        self.host = Host::Empty;
        // IDs are never reused within this runtime, including after undo and note switches.
    }

    pub fn load_document(&mut self, note: &str, doc: DocDocument, focus: bool) {
        self.clear();
        self.active.load(note, doc, focus);
        self.host = Host::Document;
    }

    pub fn mount_card(
        &mut self,
        note: &str,
        card: &str,
        mut doc: DocDocument,
        viewport: Option<[f32; 4]>,
    ) {
        self.next_namespace = self
            .next_namespace
            .checked_add(1)
            .expect("document ID namespace exhausted");
        let mut next = self
            .next_namespace
            .checked_mul(1 << 32)
            .expect("document ID namespace overflow");
        fn assign(blocks: &mut [DocBlock], next: &mut u64) {
            for block in blocks {
                // Older cards may contain collapsed headings from the document editor.
                block.folded = false;
                block.id = *next;
                *next += 1;
                assign(&mut block.children, next);
            }
        }
        assign(&mut doc.blocks, &mut next);
        doc.next_id = next;
        let mut session = DocSession::default();
        session.load(&format!("{note}:{card}"), doc, false);
        session.viewport_rect = viewport;
        session.core.include_title = false;
        self.cards.insert(card.to_owned(), session);
    }

    /// The application must commit the active edit before handing its session to another host.
    pub fn activate_card(&mut self, owner: &str) -> bool {
        if self.canvas_owner() == Some(owner) {
            return true;
        }
        let Some(next) = self.cards.remove(owner) else {
            return false;
        };
        let previous = std::mem::replace(&mut self.active, next);
        if let Host::Canvas(old) = std::mem::replace(&mut self.host, Host::Canvas(owner.into())) {
            self.cards.insert(old, previous);
        }
        true
    }

    pub fn card(&self, owner: &str) -> Option<&DocSession> {
        if self.canvas_owner() == Some(owner) {
            Some(&self.active)
        } else {
            self.cards.get(owner)
        }
    }

    pub fn card_mut(&mut self, owner: &str) -> Option<&mut DocSession> {
        if self.canvas_owner() == Some(owner) {
            Some(&mut self.active)
        } else {
            self.cards.get_mut(owner)
        }
    }

    pub fn for_block(&mut self, id: u64) -> Option<&mut DocSession> {
        if self.active.core.block(id).is_some() {
            return Some(&mut self.active);
        }
        self.cards
            .values_mut()
            .find(|session| session.core.block(id).is_some())
    }

    pub fn owner_of(&self, id: u64) -> Option<&str> {
        if self.active.core.block(id).is_some() {
            return self.canvas_owner();
        }
        self.cards
            .iter()
            .find(|(_, session)| session.core.block(id).is_some())
            .map(|(owner, _)| owner.as_str())
    }

    pub fn remove_card(&mut self, owner: &str) {
        self.cards.remove(owner);
        if self.canvas_owner() == Some(owner) {
            self.active = DocSession::default();
            self.host = Host::Empty;
        }
    }

    pub fn release_offscreen_layouts(&mut self, visible: &std::collections::HashSet<String>) {
        for (owner, session) in &mut self.cards {
            if !visible.contains(owner) {
                session.text_layouts.borrow_mut().clear();
                session.block_rects.clear();
            }
        }
    }

    pub fn set_canvas_viewport(&mut self, rect: [f32; 4]) {
        if self.canvas_owner().is_some() {
            self.active.viewport_rect = Some(rect);
        }
        for session in self.cards.values_mut() {
            session.viewport_rect = Some(rect);
        }
    }
}
