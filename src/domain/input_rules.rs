//! The structured document's Markdown shortcuts, shared by both input adapters.
use crate::doc_core::{DocBlockKind, DocMark};
use std::ops::Range;

pub fn block(prefix: &str) -> Option<DocBlockKind> {
    Some(match prefix {
        "# " => DocBlockKind::Heading { level: 1 },
        "## " => DocBlockKind::Heading { level: 2 },
        "### " => DocBlockKind::Heading { level: 3 },
        "#### " => DocBlockKind::Heading { level: 4 },
        "##### " => DocBlockKind::Heading { level: 5 },
        "###### " => DocBlockKind::Heading { level: 6 },
        "- " | "* " | "+ " => DocBlockKind::Bullet,
        "[] " | "[ ] " | "- [ ] " | "* [ ] " => DocBlockKind::Todo { done: false },
        "[x] " | "[X] " | "- [x] " | "- [X] " => DocBlockKind::Todo { done: true },
        "> " => DocBlockKind::Quote,
        _ => {
            let number = prefix.strip_suffix(". ")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            DocBlockKind::Ordered { start: None }
        }
    })
}
pub fn inline(prefix: &str) -> Option<(Range<usize>, Range<usize>, DocMark)> {
    if !matches!(prefix.as_bytes().last(), Some(b'*' | b'_' | b'~' | b'`')) {
        return None;
    }
    // Preserve TeX punctuation while authoring an inline formula.
    let dollars = prefix.char_indices().filter(|(i, c)| *c == '$'
        && prefix[..*i].chars().rev().take_while(|c| *c == '\\').count() % 2 == 0).count();
    if dollars % 2 == 1 { return None; }
    for (token, mark) in [
        ("**", DocMark::Bold),
        ("__", DocMark::Bold),
        ("~~", DocMark::Strike),
        ("`", DocMark::Code),
        ("*", DocMark::Italic),
        ("_", DocMark::Italic),
    ] {
        if !prefix.ends_with(token) || prefix.len() <= token.len() * 2 {
            continue;
        }
        let end = prefix.len() - token.len();
        let before = &prefix[..end];
        let Some(open) = before.rfind(token) else {
            continue;
        };
        if before[..open].ends_with('\\') {
            continue;
        }
        if token.len() == 1
            && ((open > 0 && before.as_bytes()[open - 1] == token.as_bytes()[0])
                || before.get(open + 1..open + 2) == Some(token))
        {
            continue;
        }
        let content = open + token.len()..end;
        let text = &prefix[content.clone()];
        if text.is_empty()
            || text.starts_with(char::is_whitespace)
            || text.ends_with(char::is_whitespace)
        {
            continue;
        }
        return Some((open..prefix.len(), content, mark));
    }
    None
}

pub fn link(prefix: &str) -> Option<(Range<usize>, Range<usize>, &str)> {
    if !prefix.ends_with(')') {
        return None;
    }
    let open = prefix.rfind('[')?;
    let split = open + prefix[open..].find("](")?;
    let url = &prefix[split + 2..prefix.len() - 1];
    if split == open + 1
        || !(url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("mailto:"))
    {
        return None;
    }
    Some((open..prefix.len(), open + 1..split, url))
}
