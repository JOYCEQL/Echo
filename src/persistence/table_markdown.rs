use crate::table_data::TableData;
use std::ops::Range;
fn row_cells(line: &str) -> Vec<String> {
    let line = line.trim().trim_matches('|');
    let mut cells = vec![];
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut ticks = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            current.push('|');
            continue;
        }
        if ch == '`' {
            ticks = !ticks;
        }
        if ch == '|' && !ticks {
            cells.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    cells.push(current.trim().to_string());
    cells
}
/// Parse Markdown table payloads, excluding fenced code examples.
pub fn parse(source: &str) -> Vec<(Range<usize>, TableData)> {
    let mut offset = 0;
    let lines: Vec<_> = source
        .split_inclusive('\n')
        .map(|l| {
            let start = offset;
            offset += l.len();
            (start, offset, l.trim_end_matches(['\r', '\n']))
        })
        .collect();
    let mut found = vec![];
    let mut i = 0;
    let mut fence: Option<(char, usize)> = None;
    while i < lines.len() {
        let l = lines[i].2.trim();
        let marker = l.chars().next().unwrap_or(' ');
        let count = l.chars().take_while(|c| *c == marker).count();
        if (marker == '`' || marker == '~') && count >= 3 {
            if let Some((ch, n)) = fence {
                if marker == ch && count >= n {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            i += 1;
            continue;
        }
        if fence.is_some() || !l.contains('|') {
            i += 1;
            continue;
        }
        let mut sep = i + 1;
        while sep < lines.len() && lines[sep].2.trim().is_empty() {
            sep += 1;
        }
        if sep >= lines.len() {
            break;
        }
        let separators = row_cells(lines[sep].2);
        if separators.is_empty()
            || !separators.iter().all(|v| {
                let d = v.trim_matches(':');
                d.len() >= 3 && d.chars().all(|c| c == '-')
            })
        {
            i += 1;
            continue;
        }
        let header = row_cells(l);
        if header.len() != separators.len() {
            i += 1;
            continue;
        }
        let mut rows = vec![header];
        let mut end = lines[sep].1;
        let mut j = sep + 1;
        loop {
            let mut next = j;
            while next < lines.len() && lines[next].2.trim().is_empty() {
                next += 1;
            }
            if next >= lines.len()
                || !lines[next].2.contains('|')
                || lines[next].2.trim().starts_with("```")
            {
                break;
            }
            if next > j {
                let candidate = lines[next].2.trim();
                if !candidate.starts_with('|') && !candidate.ends_with('|') {
                    break;
                }
                let mut maybe_sep = next + 1;
                while maybe_sep < lines.len() && lines[maybe_sep].2.trim().is_empty() {
                    maybe_sep += 1;
                }
                if maybe_sep < lines.len()
                    && row_cells(lines[maybe_sep].2).iter().all(|v| {
                        let d = v.trim_matches(':');
                        d.len() >= 3 && d.chars().all(|c| c == '-')
                    })
                {
                    break;
                }
            }
            rows.push(row_cells(lines[next].2));
            end = lines[next].1;
            j = next + 1;
        }
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
        for r in &mut rows {
            r.resize(cols, String::new());
        }
        let mut align = separators
            .iter()
            .map(|s| {
                if s.starts_with(':') && s.ends_with(':') {
                    1
                } else if s.ends_with(':') {
                    2
                } else {
                    0
                }
            })
            .collect::<Vec<_>>();
        align.resize(cols, 0);
        found.push((
            lines[i].0..end,
            TableData {
                rows,
                widths: vec![1.; cols],
                align,
            },
        ));
        i = j;
    }
    found
}
