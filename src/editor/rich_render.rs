//! Derived formula previews. Sources remain in the document; one lazy worker and a
//! bounded cache serve both hosts. No file, network or browser is used by the renderer.
use rune::{Image, ImageCache, Waker};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, mpsc},
};

pub const MAX_SOURCE: usize = 16 * 1024;
const CAPACITY: usize = 64;
// RGBA at 2x plus mip levels. Bound retained preview pixels separately from entry count.
const PIXEL_BUDGET: f32 = 24. * 1024. * 1024.;
fn pixel_cost(value: &Preview) -> f32 {
    match value {
        Preview::Ready(f) => f.width * f.height * 4. * 4. * 1.34,
        _ => 0.,
    }
}
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Key {
    source: String,
    color: u32,
    size: u32,
    inline: bool,
}
#[derive(Clone)]
pub struct Formula {
    pub image: Image,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}
#[derive(Clone)]
pub enum Preview {
    Pending,
    Ready(Formula),
    Error(String),
}
struct Svg {
    bytes: String,
    width: f32,
    height: f32,
    baseline: f32,
}
struct Entry {
    used: u64,
    frame: u64,
    value: Preview,
}
struct Worker {
    send: mpsc::SyncSender<Key>,
    receive: mpsc::Receiver<(Key, Result<Svg, String>)>,
}
#[derive(Default)]
struct Inner {
    entries: HashMap<Key, Entry>,
    tick: u64,
    frame: u64,
    worker: Option<Worker>,
    waker: Option<Waker>,
}
#[derive(Default)]
pub struct Formulas(RefCell<Inner>);
impl Formulas {
    pub fn begin_frame(&self) { self.0.borrow_mut().frame += 1; }
    pub fn set_waker(&self, waker: Waker) {
        self.0.borrow_mut().waker = Some(waker);
    }
    pub fn get(&self, source: &str, color: u32, size: f32, inline: bool) -> Preview {
        if source.len() > MAX_SOURCE {
            return Preview::Error("公式超过 16 KiB，无法预览".into());
        }
        if source.trim().is_empty() {
            return Preview::Error("请输入 LaTeX 公式".into());
        }
        let key = Key {
            source: source.into(),
            color,
            size: size.to_bits(),
            inline,
        };
        let mut inner = self.0.borrow_mut();
        inner.tick += 1;
        let tick = inner.tick;
        let frame = inner.frame;
        if let Some(entry) = inner.entries.get_mut(&key) {
            entry.used = tick;
            entry.frame = frame;
            return entry.value.clone();
        }
        if inner.entries.len() >= CAPACITY {
            let oldest = inner
                .entries
                .iter()
                .filter(|(_, e)| e.frame != frame && !matches!(e.value, Preview::Pending))
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone());
            let Some(oldest) = oldest else {
                return if inner.entries.values().any(|e| matches!(e.value, Preview::Pending)) {
                    Preview::Pending
                } else { Preview::Error("当前页面公式数量超过预览上限，请拆分内容".into()) };
            };
            inner.entries.remove(&oldest);
        }
        if inner.worker.is_none() {
            let Some(waker) = inner.waker.clone() else {
                return Preview::Error("公式渲染服务尚未初始化".into());
            };
            let (send, jobs) = mpsc::sync_channel::<Key>(CAPACITY);
            let (results, receive) = mpsc::channel();
            match std::thread::Builder::new()
                .name("echo-formulas".into())
                .spawn(move || {
                    while let Ok(key) = jobs.recv() {
                        let result = formula_svg(
                            &key.source,
                            key.color,
                            f32::from_bits(key.size),
                            key.inline,
                        );
                        if results.send((key, result)).is_err() {
                            break;
                        }
                        waker();
                    }
                }) {
                Ok(_) => inner.worker = Some(Worker { send, receive }),
                Err(e) => return Preview::Error(format!("无法启动公式渲染：{e}")),
            }
        }
        match inner.worker.as_ref().unwrap().send.try_send(key.clone()) {
            Ok(()) => {
                inner.entries.insert(
                    key,
                    Entry {
                        used: tick,
                        frame,
                        value: Preview::Pending,
                    },
                );
                Preview::Pending
            }
            Err(mpsc::TrySendError::Full(_)) => Preview::Pending,
            Err(mpsc::TrySendError::Disconnected(_)) => Preview::Error("公式渲染服务已停止".into()),
        }
    }
    pub fn poll(&self, cache: Option<&ImageCache>) -> bool {
        let Some(cache) = cache else {
            return false;
        };
        let mut inner = self.0.borrow_mut();
        let Some(worker) = &inner.worker else { return false; };
        let mut results = Vec::new();
        let stopped = loop {
            match worker.receive.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        let mut changed = !results.is_empty();
        for (key, mut result) in results {
            let frame = inner.frame;
            if let Ok(svg) = &result {
                let cost = svg.width * svg.height * 4. * 4. * 1.34;
                while inner
                    .entries
                    .values()
                    .map(|e| pixel_cost(&e.value))
                    .sum::<f32>()
                    + cost
                    > PIXEL_BUDGET
                {
                    let Some(oldest) = inner
                        .entries
                        .iter()
                        .filter(|(_, e)| e.frame != frame && matches!(e.value, Preview::Ready(_)))
                        .min_by_key(|(_, e)| e.used)
                        .map(|(k, _)| k.clone())
                    else {
                        result = Err("当前页面公式预览超过内存上限，请拆分内容".into());
                        break;
                    };
                    inner.entries.remove(&oldest);
                }
            }
            if let Some(entry) = inner.entries.get_mut(&key) {
                entry.value = match result {
                    Ok(svg) => Preview::Ready(Formula {
                        image: cache.svg(
                            Arc::<[u8]>::from(svg.bytes.into_bytes()),
                            svg.width.max(svg.height),
                            2.,
                        ),
                        width: svg.width,
                        height: svg.height,
                        baseline: svg.baseline,
                    }),
                    Err(error) => Preview::Error(error),
                };
            }
        }
        if stopped {
            for entry in inner.entries.values_mut() {
                if matches!(entry.value, Preview::Pending) {
                    entry.value = Preview::Error("公式渲染服务已停止".into());
                    changed = true;
                }
            }
        }
        changed
    }
}
fn formula_svg(source: &str, color: u32, size: f32, inline: bool) -> Result<Svg, String> {
    if source.len() > MAX_SOURCE {
        return Err("公式超过 16 KiB，无法预览".into());
    }
    let ast = ratex_parser::parser::parse(source).map_err(|e| format!("公式语法错误：{e}"))?;
    let color = ratex_types::color::Color::parse(&format!("#{color:06x}")).ok_or("无效公式颜色")?;
    let options = ratex_layout::LayoutOptions::default()
        .with_color(color)
        .with_style(if inline {
            ratex_types::math_style::MathStyle::Text
        } else {
            ratex_types::math_style::MathStyle::Display
        });
    let layout = ratex_layout::layout(&ast, &options);
    ratex_layout::validate_layout(&layout)?;
    let list = ratex_layout::to_display_list(&layout);
    let pad = 2.;
    let width = list.width as f32 * size + 2. * pad;
    let height = (list.height + list.depth) as f32 * size + 2. * pad;
    if !width.is_finite()
        || !height.is_finite()
        || width > 4096.
        || height > 2048.
        || width * height > 128. * 1024.
    {
        return Err("公式尺寸过大，请拆分公式".into());
    }
    let opts = ratex_svg::SvgOptions {
        font_size: size as f64,
        padding: pad as f64,
        embed_glyphs: true,
        ..Default::default()
    };
    let bytes =
        ratex_svg::render_to_svg_with_color_syntax(&list, &opts, ratex_svg::SvgColorSyntax::Rgb);
    // Embedded glyphs must be self-contained. Missing glyphs must not silently use a host font.
    if bytes.contains("<text") {
        return Err("公式含有当前数学字体无法显示的字符".into());
    }
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("公式图形过于复杂，请拆分公式".into());
    }
    Ok(Svg {
        bytes,
        width,
        height,
        baseline: list.height as f32 * size + pad,
    })
}
pub fn language(lang: &str) -> Option<bool> {
    if lang.eq_ignore_ascii_case("mermaid") {
        Some(false)
    } else if ["math", "latex", "tex"]
        .iter()
        .any(|l| lang.eq_ignore_ascii_case(l))
    {
        Some(true)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formulas_render_offline_with_outlined_fonts_and_diagnostics() {
        for source in [
            r"E=mc^2",
            r"\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
            r"\int_0^1 x^2\,dx",
            r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        ] {
            let svg = formula_svg(source, 0xeeeeee, 20., false).unwrap();
            assert!(svg.width > 4. && svg.height > 4.);
            assert!(!svg.bytes.contains("<text"));
            assert!(svg.bytes.contains("<path"));
        }
        assert!(formula_svg(r"\notARealCommand{x}", 0, 20., false).is_err());
        assert!(formula_svg(&"x".repeat(MAX_SOURCE + 1), 0, 20., false).is_err());
    }
}

fn mix_oklab(a: rune::Color, b: rune::Color, t: f32) -> rune::Color {
    let ca = a.to_oklab();
    let cb = b.to_oklab();
    rune::Color::from_oklab(std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * t))
}

fn align_row(inner: rune::El, align: crate::doc_core::DocAlign) -> rune::El {
    use crate::doc_core::DocAlign;
    match align {
        DocAlign::Left => rune::row().w_full().child(inner),
        DocAlign::Center => rune::row().w_full().justify_center().child(inner),
        DocAlign::Right => rune::row().w_full().justify_end().child(inner),
    }
}

pub fn block_preview(
    s: &crate::doc_editor::RenderState<'_>,
    b: &crate::doc_core::DocBlock,
    width: f32,
) -> rune::El {
    use crate::{
        doc_core::{DocAlign, DocBlockKind, DocPos, DocSel},
        icons::{ALIGN_CENTER, ALIGN_LEFT, ALIGN_RIGHT, CHECK, COMPOSE, COPY, ico},
        ui::{c, label},
    };
    use rune::{Cursor, el, row};
    let DocBlockKind::Code { lang, .. } = &b.kind else {
        unreachable!()
    };
    let bid = b.id;
    let align = b.align;
    let source = b.text.text.clone();
    let math = language(lang) == Some(true);
    let theme = s.theme();
    let dark = theme.is_dark();
    let colors = crate::ui::editor_colors(theme);
    let ink_hex = crate::ui::theme_ink(theme);
    let muted_hex = crate::ui::theme_muted(theme);
    let accent_hex = crate::ui::accent(s);
    let accent_soft_hex = crate::ui::accent_soft(s);
    let copied = s
        .editor
        .active
        .copied_block
        .is_some_and(|(id, at)| id == bid && at.elapsed().as_secs_f32() < 1.6);
    let hovered = s.editor.active.hover_block == Some(bid);
    let show_actions = hovered || copied;

    let enter_edit = move |s: &mut crate::model::State, tx: &mut rune::Tx| {
        crate::canvas_document::activate_block(s, bid);
        s.editor.active.focused = true;
        s.editor.active.popover = None;
        s.editor.active.core.sel = DocSel::caret(DocPos::new(bid, 0));
        tx.blur();
        tx.instant();
    };

    let mut align_group = row().gap(1.).center_y();
    for (key, title, icon, target) in [
        ("left", "左对齐", &ALIGN_LEFT, DocAlign::Left),
        ("center", "居中对齐", &ALIGN_CENTER, DocAlign::Center),
        ("right", "右对齐", &ALIGN_RIGHT, DocAlign::Right),
    ] {
        let active = align == target;
        align_group = align_group.child(
            el().key(format!("preview-align-{key}-{bid}"))
                .size(24., 24.)
                .radius(5.)
                .center()
                .role("button")
                .focusable(true)
                .label(s.tr(title))
                .help(s.tr(title))
                .cursor(Cursor::Pointer)
                .fill(if active {
                    c(accent_soft_hex)
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(if active {
                    c(accent_soft_hex)
                } else {
                    c(ink_hex).alpha(0.06)
                })
                .on_press_tx(move |s: &mut crate::model::State, tx| {
                    crate::canvas_document::activate_block(s, bid);
                    s.editor.active.core.set_align(&[bid], target);
                    s.editor.active.focused = false;
                    crate::doc_editor::sync_to_state(s);
                    tx.instant();
                })
                .child(ico(icon, 13.).color(c(if active { accent_hex } else { muted_hex }))),
        );
    }

    let copy_source = source.clone();
    let copy_ok_hex = theme.choose(0x2ea121, 0x72c79b);
    let actions = row()
        .key(format!("preview-actions-{bid}"))
        .gap(3.)
        .padding(3.)
        .radius(8.)
        .center_y()
        .fill(c(colors.raised).alpha(if dark { 0.96 } else { 0.98 }))
        .border(1., c(colors.border))
        .shadow(0., 2., c(0x1f2329).alpha(if dark { 0.28 } else { 0.08 }))
        .opacity(if show_actions { 1. } else { 0. })
        .instant_changes()
        .child(align_group)
        .child(el().w(1.).h(12.).fill(c(colors.border)))
        .child(
            row()
                .key(format!("preview-edit-{bid}"))
                .h(24.)
                .px(8.)
                .gap(4.)
                .radius(5.)
                .center_y()
                .role("button")
                .focusable(true)
                .label(s.tr("编辑源码"))
                .cursor(Cursor::Pointer)
                .hover(c(ink_hex).alpha(0.06))
                .on_press_tx(enter_edit)
                .child(ico(&COMPOSE, 12.5).color(c(muted_hex)))
                .child(label(s.tr("编辑源码"), 11.5, 500., muted_hex)),
        )
        .child(el().w(1.).h(12.).fill(c(colors.border)))
        .child(
            row()
                .key(format!("preview-copy-{bid}"))
                .h(24.)
                .px(8.)
                .gap(4.)
                .radius(5.)
                .center_y()
                .role("button")
                .focusable(true)
                .label(if copied {
                    s.tr("已复制")
                } else {
                    s.tr("复制")
                })
                .cursor(Cursor::Pointer)
                .hover(c(ink_hex).alpha(0.06))
                .on_press_tx(move |s: &mut crate::model::State, tx| {
                    tx.copy(copy_source.clone());
                    s.editor.active.copied_block = Some((bid, std::time::Instant::now()));
                    tx.instant();
                })
                .child(
                    ico(if copied { &CHECK } else { &COPY }, 12.5).color(c(if copied {
                        copy_ok_hex
                    } else {
                        muted_hex
                    })),
                )
                .child(label(
                    if copied {
                        s.tr("已复制")
                    } else {
                        s.tr("复制")
                    },
                    11.5,
                    500.,
                    if copied { copy_ok_hex } else { muted_hex },
                )),
        );

    let hover_wash = if hovered {
        c(ink_hex).alpha(if dark { 0.035 } else { 0.025 })
    } else {
        rune::Color::TRANSPARENT
    };

    if math {
        let inner_w = (width - 24.).max(1.);
        let content = match s.formulas.get(&source, ink_hex, 21., false) {
            Preview::Ready(f) => {
                if let Some(error) = f.image.error() {
                    error_banner(s, &error.to_string(), enter_edit)
                } else {
                    let formula_el = el().size(f.width, f.height).pixels(f.image.pixels());
                    let stage = if f.width <= inner_w {
                        align_row(formula_el, align)
                            .min_h(54.)
                            .px(12.)
                            .py(14.)
                            .center_y()
                    } else {
                        el().w_full()
                            .min_h(54.)
                            .px(12.)
                            .py(14.)
                            .child(el().w(inner_w).scroll_x().child(formula_el))
                    };
                    stage
                        .radius(10.)
                        .fill(hover_wash)
                        .cursor(Cursor::Pointer)
                        .on_press_tx(enter_edit)
                }
            }
            Preview::Pending => align_row(
                label(s.tr("正在排版…"), 13., 400., muted_hex),
                align,
            )
            .min_h(54.)
            .px(12.)
            .py(14.)
            .center_y()
            .radius(10.)
            .fill(hover_wash)
            .cursor(Cursor::Pointer)
            .on_press_tx(enter_edit),
            Preview::Error(e) => error_banner(s, &e, enter_edit),
        };
        return el()
            .key(format!("rich-preview-{bid}"))
            .w(width)
            .radius(10.)
            .child(content)
            .child(actions.pin(None, Some(6.), Some(6.), None));
    }

    let page_bg = c(crate::ui::page_surface(s));
    let content = if source.len() > MAX_SOURCE {
        error_banner(s, "图表超过 16 KiB，无法预览", enter_edit)
    } else {
        let ink = c(ink_hex);
        let muted = c(muted_hex);
        let accent_col = c(accent_hex);
        let mut style = rune_mermaid::DiagramStyle::neutral(dark).min_scale(0.);
        style.max_width = (width - 16.).max(1.);
        style.text = ink;
        style.muted = muted;
        style.surface = page_bg;
        style.surface_border = rune::Color::TRANSPARENT;
        style.node_fill = if dark {
            mix_oklab(c(0x313236), accent_col, 0.10)
        } else {
            mix_oklab(c(0xf7f8fa), accent_col, 0.08)
        };
        style.node_border = if dark {
            mix_oklab(c(0x525660), accent_col, 0.34)
        } else {
            mix_oklab(c(0xc6cdd8), accent_col, 0.36)
        };
        style.line = if dark {
            mix_oklab(c(0x9096a4), accent_col, 0.22)
        } else {
            mix_oklab(c(0x7c8594), accent_col, 0.22)
        };
        style.cluster_fill = accent_col.alpha(if dark { 0.065 } else { 0.045 });
        style.note_fill = if dark {
            mix_oklab(c(0x37342b), accent_col, 0.08)
        } else {
            mix_oklab(c(0xfffbeb), accent_col, 0.05)
        };
        style.note_border = if dark {
            mix_oklab(c(0x6b6043), accent_col, 0.15)
        } else {
            mix_oklab(c(0xe6d39c), accent_col, 0.15)
        };
        style.font = crate::ui::font(13.5, 450.).lh(19.);

        match rune_mermaid::render_with_hover(
            &source,
            &style,
            rune_mermaid::Hover::Neighbours,
            accent_col,
        ) {
            Some(diagram) => align_row(diagram, align)
                .px(8.)
                .py(10.)
                .radius(10.)
                .fill(hover_wash)
                .on_double_press_tx(enter_edit),
            None => error_banner(s, "无法绘制图表，请检查 Mermaid 语法或图表类型", enter_edit),
        }
    };

    el().key(format!("rich-preview-{bid}"))
        .w(width)
        .radius(10.)
        .child(content)
        .child(actions.pin(None, Some(6.), Some(6.), None))
}

fn error_banner(
    s: &crate::model::State,
    message: &str,
    on_press: impl Fn(&mut crate::model::State, &mut rune::Tx) + 'static,
) -> rune::El {
    use crate::ui::{c, label};
    let theme = s.theme();
    rune::el()
        .w_full()
        .px(16.)
        .py(14.)
        .radius(10.)
        .fill(c(theme.choose(0xfff6f5, 0x352727)))
        .border(1., c(theme.choose(0xf4d2ce, 0x5c3c3c)))
        .cursor(rune::Cursor::Pointer)
        .on_press_tx(on_press)
        .child(
            label(
                format!("{}：{message}", s.tr("预览失败")),
                13.,
                450.,
                theme.choose(0xb84034, 0xff9385),
            )
            .wrap(),
        )
}

/// Raw-source ranges, including delimiters, excluding code spans and escaped dollars.
pub fn inline_ranges(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    if !text.contains('$') { return Vec::new(); }
    pulldown_cmark::Parser::new_ext(text, pulldown_cmark::Options::ENABLE_MATH)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            if let pulldown_cmark::Event::InlineMath(source) = event {
                Some((range, source.into_string()))
            } else {
                None
            }
        })
        .collect()
}
#[derive(Clone, Default)]
pub struct InlinePreview {
    pub text: String,
    pub images: Vec<rune::InlineImage>,
    // Replacements map a single zero-width display marker to the intact source expression.
    pub replacements: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}
impl InlinePreview {
    pub fn raw_to_display(&self, raw: usize) -> usize {
        let mut delta = 0;
        for (src, dst) in &self.replacements {
            if raw < src.start {
                break;
            }
            if raw < src.end {
                return dst.start;
            }
            delta += src.len() - dst.len();
        }
        raw.saturating_sub(delta)
    }
    pub fn display_to_raw(&self, display: usize) -> usize {
        let mut delta = 0;
        for (src, dst) in &self.replacements {
            if display < dst.start {
                break;
            }
            if display < dst.end {
                return src.start;
            }
            delta += src.len() - dst.len();
        }
        display + delta
    }
}
/// All formulas in one paragraph become preview objects together, so a pending
/// worker cannot leave half of a paragraph in source mode. Errors are explicit.
pub fn inline_preview(
    s: &crate::model::State,
    rich: &crate::doc_core::RichText,
    size: f32,
    color: u32,
    max_width: f32,
) -> Result<Option<InlinePreview>, String> {
    let ranges: Vec<_> = inline_ranges(&rich.text)
        .into_iter()
        .filter(|(r, _)| {
            !rich
                .iter_spans()
                .any(|(span, marks)| marks.code && span.start < r.end && r.start < span.end)
        })
        .collect();
    if ranges.is_empty() {
        return Ok(None);
    }
    let mut preview = InlinePreview::default();
    let mut end = 0;
    let mut pending = false;
    for (range, source) in ranges {
        let value = s.formulas.get(&source, color, size, true);
        let f = match value {
            Preview::Ready(f) => {
                if let Some(e) = f.image.error() {
                    return Err(e.to_string());
                }
                f
            }
            Preview::Pending => {
                pending = true;
                continue;
            }
            Preview::Error(e) => return Err(e),
        };
        preview.text.push_str(&rich.text[end..range.start]);
        let at = preview.text.len();
        preview.text.push('\u{200b}');
        let scale = (max_width / f.width).min(1.);
        preview.images.push(rune::InlineImage {
            at,
            width: f.width * scale,
            height: f.height * scale,
            baseline: f.baseline * scale,
            pixels: f.image.pixels().clone(),
        });
        end = range.end;
        preview.replacements.push((range, at..preview.text.len()));
    }
    if pending {
        return Err("正在排版…".into());
    }
    preview.text.push_str(&rich.text[end..]);
    Ok(Some(preview))
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    use crate::{doc_core::*, file_format};
    #[test]
    fn rich_formula_markdown_roundtrips_without_escaping_latex() {
        let source = "正文 $\\frac{a_1}{b^2}$ 结束\n\n$$\n\\int_0^1 x^2\\,dx\n$$\n\n```mermaid\nflowchart LR\nA --> B\n```\n";
        let mut next = 1;
        let blocks = file_format::parse_standard(source, &mut next).unwrap();
        assert_eq!(blocks.len(), 3, "{blocks:?}");
        assert_eq!(blocks[0].text.text, r"正文 $\frac{a_1}{b^2}$ 结束");
        assert!(matches!(&blocks[1].kind, DocBlockKind::Code { lang, .. } if lang == "math"));
        assert_eq!(blocks[1].text.text, r"\int_0^1 x^2\,dx");
        let mut doc = DocDocument::empty();
        doc.blocks = blocks;
        doc.next_id = next;
        let exported = file_format::write_document_body(&doc).unwrap();
        assert!(exported.contains(r"$\frac{a_1}{b^2}$"));
        let mut next = 10;
        let again = file_format::parse_standard(&exported, &mut next).unwrap();
        for (a, b) in doc.blocks.iter().zip(&again) {
            assert_eq!(a.text, b.text);
            assert_eq!(a.kind, b.kind);
        }
        assert_eq!(again.len(), doc.blocks.len());
    }
    #[test]
    fn rich_inline_delimiters_exclude_code_and_escaped_currency() {
        let raw = r"价格 \$5，代码 `$x$`，公式 $x_1^2$";
        let formulas = inline_ranges(raw);
        assert_eq!(formulas.len(), 1);
        assert_eq!(formulas[0].1, "x_1^2");
        assert!(crate::input_rules::inline(r"$a_b_c").is_none());
    }
    #[test]
    fn rich_math_fence_enter_and_undo_preserve_source() {
        let mut doc = DocDocument::empty();
        let id = doc.blocks[0].id;
        doc.blocks[0].text = RichText::plain("$$");
        let mut core = DocCore::new(doc);
        core.sel = DocSel::caret(DocPos::new(id, 2));
        core.enter();
        assert!(
            matches!(&core.block(id).unwrap().kind, DocBlockKind::Code { lang, .. } if lang == "math")
        );
        assert_eq!(core.sel.head, DocPos::new(id, 0));
        core.undo();
        assert_eq!(core.block(id).unwrap().text.text, "$$");
    }
}

#[cfg(test)]
mod layout_tests {
    #[test]
    fn rich_inline_images_reserve_height_and_width_at_retina_scale() {
        fn view(height: &f32, _: &mut rune::Cx<f32>) -> rune::El {
            rune::text("前文 \u{200b} 后文")
                .key("formula-line")
                .w(300.)
                .wrap()
                .font(rune::TextStyle::new(16., 400.).lh(26.))
                .inline_images(&[rune::InlineImage {
                    at: "前文 ".len(),
                    width: 90.,
                    height: *height,
                    baseline: *height - 8.,
                    pixels: rune::Pixels::new(),
                }])
        }
        for scale in [1., 2.] {
            let mut rt = rune::Runtime::new(70., view, (400., 200.), rune::Env::new(false, scale));
            rt.frame(0.);
            let rect = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some("formula-line"))
                .unwrap()
                .rect;
            assert!(
                rect[3] >= 70.,
                "formula must enlarge the line: {rect:?}, scale={scale}"
            );
            let images = [rune::InlineImage {
                at: "前文 ".len(),
                width: 90.,
                height: 70.,
                baseline: 62.,
                pixels: rune::Pixels::new(),
            }];
            let at = rune::text::doc_text::hit_test_images(
                "前文 \u{200b} 后文",
                &[],
                &images,
                rune::TextStyle::new(16., 400.).lh(26.),
                Some(300.),
                rune::Align::Start,
                299.,
                40.,
            );
            assert_eq!(at, "前文 \u{200b} 后文".len());
        }
    }
    #[test]
    fn rich_mermaid_native_diagrams_and_unsupported_type() {
        let style = rune_mermaid::DiagramStyle::neutral(false);
        for source in [
            "flowchart LR\nA[开始] --> B[完成]",
            "sequenceDiagram\nAlice->>Bob: Hello",
            "stateDiagram-v2\n[*] --> Ready\nReady --> [*]",
            "classDiagram\nA <|-- B",
            "pie\n\"One\": 2\n\"Two\": 3",
            "mindmap\n  root\n    One\n    Two",
        ] {
            assert!(rune_mermaid::render(source, &style).is_some(), "{source}");
        }
        assert!(rune_mermaid::render("notADiagram\nA --> B", &style).is_none());
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn rich_formula_cache_keeps_current_frame_and_reports_worker_failure() {
        let formulas = Formulas::default();
        formulas.set_waker(Arc::new(|| {}));
        let cache = ImageCache::new(Arc::new(|| {}));
        let image = cache.svg(Arc::<[u8]>::from(&b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"/>"[..]), 1., 1.);
        for n in 0..CAPACITY {
            formulas.0.borrow_mut().entries.insert(Key { source: n.to_string(), color: 0, size: 16f32.to_bits(), inline: true }, Entry {
                used: n as u64, frame: 0, value: Preview::Ready(Formula { image: image.clone(), width: 1., height: 1., baseline: 1. }),
            });
        }
        assert!(matches!(formulas.get("x", 0, 16., true), Preview::Error(_)));
        assert_eq!(formulas.0.borrow().entries.len(), CAPACITY);
        formulas.begin_frame();
        assert!(matches!(formulas.get("x", 0, 16., true), Preview::Pending));
        assert_eq!(formulas.0.borrow().entries.len(), CAPACITY);
        let failed = Formulas::default();
        let (send, _jobs) = mpsc::sync_channel(1);
        let (results, receive) = mpsc::channel(); drop(results);
        failed.0.borrow_mut().worker = Some(Worker { send, receive });
        failed.0.borrow_mut().entries.insert(Key { source: "x".into(), color: 0, size: 16f32.to_bits(), inline: true }, Entry { used: 0, frame: 0, value: Preview::Pending });
        assert!(failed.poll(Some(&cache)));
        assert!(matches!(failed.get("x", 0, 16., true), Preview::Error(_)));
        assert!(!failed.poll(Some(&cache)));
    }
}
