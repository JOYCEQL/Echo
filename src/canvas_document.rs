//! A canvas card can host the exact structured editor. Only the outer geometry is canvas-owned.
use crate::{
    doc_core::DocBlockKind,
    model::{Block, State, WorkspaceMode},
};

pub fn enabled(s: &State) -> bool {
    s.mode == WorkspaceMode::Document || s.editor.canvas_owner().is_some()
}

/// Canvas geometry is two-dimensional, so Rune's one-dimensional virtual_rows is not
/// applicable. Its drawn-layout observer supplies the actual scroll/zoom transform;
/// only changes to this overscanned mounted set commit a new view.
pub fn refresh_visible(s: &mut State, size: [f32; 2], offset: [f32; 2]) -> bool {
    if s.mode != WorkspaceMode::Canvas {
        return false;
    }
    let Some(note) = s.note() else {
        return false;
    };
    let zoom = note.zoom;
    let pad = 320. / zoom;
    let left = offset[0] / zoom - pad;
    let top = offset[1] / zoom + crate::canvas::ORIGIN_Y - pad;
    let right = (offset[0] + size[0]) / zoom + pad;
    let bottom = (offset[1] + size[1]) / zoom + crate::canvas::ORIGIN_Y + pad;
    let visible: std::collections::HashSet<_> = note
        .blocks
        .iter()
        .filter(|b| {
            (b.x <= right && b.x + b.w >= left && b.y <= bottom && b.y + b.h.max(b.min_h) >= top)
                || s.selected.as_deref() == Some(&b.id)
                || s.draft.as_deref() == Some(&b.id)
                || (s.editor.canvas_owner() == Some(&b.id) && s.editor.active.focused)
                || s.arrange_anim_until.is_some()
        })
        .map(|b| b.id.clone())
        .collect();
    if visible == s.canvas_visible {
        return false;
    }
    let missing: Vec<_> = note
        .blocks
        .iter()
        .filter(|b| {
            visible.contains(&b.id)
                && b.is_textual()
                && s.editor.canvas_owner() != Some(&b.id)
                && s.editor.card(&b.id).is_none()
        })
        .cloned()
        .collect();
    s.canvas_visible = visible;
    for b in missing {
        s.mount_editor(&b);
    }
    s.editor.release_offscreen_layouts(&s.canvas_visible);
    true
}

pub fn mount(s: &mut State, b: &Block) {
    let doc = match crate::file_format::card_document(b) {
        Ok(doc) => doc,
        Err(error) => {
            s.message = Some(format!("{}: {error}", s.tr("无法读取笔记")));
            return;
        }
    };
    s.editor
        .mount_card(&s.current, &b.id, doc, s.canvas_viewport_rect);
}

pub fn session<'a>(s: &'a State, owner: &str) -> Option<&'a crate::doc_editor::DocSession> {
    s.editor.card(owner)
}
pub fn session_mut<'a>(
    s: &'a mut State,
    owner: &str,
) -> Option<&'a mut crate::doc_editor::DocSession> {
    s.editor.card_mut(owner)
}
pub fn session_for_block(s: &mut State, id: u64) -> Option<&mut crate::doc_editor::DocSession> {
    s.editor.for_block(id)
}

pub fn activate(s: &mut State, owner: &str) {
    if s.editor.card(owner).is_none() {
        if let Some(b) = s
            .note()
            .and_then(|n| n.blocks.iter().find(|b| b.id == owner && b.is_textual()))
            .cloned()
        {
            mount(s, &b);
        }
    }
    if s.editor.canvas_owner() != Some(owner) {
        if s.editor.card(owner).is_none() {
            return;
        }
        s.finish_edit();
        if s.document_save_failed || !s.editor.activate_card(owner) {
            return;
        }
    }
    s.canvas_visible.insert(owner.into());
    s.selected = None;
    s.editor.active.viewport_rect = s.canvas_viewport_rect;
    s.editor.active.focused = true;
}
pub fn activate_block(s: &mut State, id: u64) {
    if s.mode != WorkspaceMode::Canvas {
        return;
    }
    if let Some(owner) = s.editor.owner_of(id).map(str::to_owned) {
        activate(s, &owner);
    }
}

pub fn sync(s: &mut State) {
    let Some(owner) = s.editor.canvas_owner().map(str::to_owned) else {
        return;
    };
    if !s.editor.active.core.dirty {
        return;
    }
    let body = match crate::file_format::write_document_body(&s.editor.active.core.doc) {
        Ok(body) => body,
        Err(error) => {
            s.message = Some(format!("{}: {error}", s.tr("保存失败")));
            return;
        }
    };
    let blank = s.editor.active.core.doc.blocks.iter().all(|b| {
        matches!(b.kind, DocBlockKind::Text)
            && b.text.text.trim().is_empty()
            && b.children.is_empty()
    });
    let is_draft = s.draft.as_deref() == Some(&owner);
    if is_draft && !blank {
        s.remember();
        s.draft = None;
    }
    let preview = s.editor.active.core.doc.preview_text();
    if let Some(b) = s.block_mut(&owner) {
        b.body = body;
        b.structured = true;
        b.kind = crate::model::Kind::Text;
        b.title.clear();
        b.body_html.clear();
        b.table = None;
    }
    s.editor.active.core.dirty = false;
    if !is_draft || !blank {
        if let Some(n) = s.note_mut() {
            n.preview = preview;
        }
        if let Some(n) = s.note_mut() {
            n.updated = crate::model::now();
            n.stamp = "刚刚".into();
        }
        s.queue_document_save();
    }
}
