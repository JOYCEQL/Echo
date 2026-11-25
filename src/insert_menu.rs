//! One command vocabulary and one presentation for both editing models.
use crate::{
    icons::*,
    model::State,
    ui::{c, label},
};
use rune::{Cursor, El, Icon, ScrollSpec, Tx, col, el, row};

#[derive(Clone, Copy, Debug)]
pub struct SlashCmd {
    pub id: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    pub hint: &'static str,
    pub keywords: &'static str,
    pub icon: &'static Icon,
}

impl SlashCmd {
    pub fn matches(&self, q_lower: &str) -> bool {
        q_lower.is_empty()
            || self.label.to_ascii_lowercase().contains(q_lower)
            || self.desc.to_ascii_lowercase().contains(q_lower)
            || self.hint.to_ascii_lowercase().contains(q_lower)
            || self.keywords.contains(q_lower)
            || crate::i18n::Language::English.text(self.label).to_lowercase().contains(q_lower)
            || crate::i18n::Language::English.text(self.desc).to_lowercase().contains(q_lower)
    }
}

pub const SLASH_CMDS: &[SlashCmd] = &[
    SlashCmd {
        id: "text",
        label: "正文",
        desc: "普通文本段落",
        hint: "P",
        keywords: "text paragraph zhengwen wenben p zw",
        icon: &TYPE,
    },
    SlashCmd {
        id: "h1",
        label: "一级标题",
        desc: "大章节主标题",
        hint: "#",
        keywords: "h1 heading1 title bt1 biaoti heading bt",
        icon: &HEADING_1,
    },
    SlashCmd {
        id: "h2",
        label: "二级标题",
        desc: "二级小节标题",
        hint: "##",
        keywords: "h2 heading2 bt2 biaoti heading bt",
        icon: &HEADING_2,
    },
    SlashCmd {
        id: "h3",
        label: "三级标题",
        desc: "三级子标题",
        hint: "###",
        keywords: "h3 heading3 bt3 biaoti heading bt",
        icon: &HEADING_3,
    },
    SlashCmd {
        id: "bullet",
        label: "无序列表",
        desc: "项目符号列表",
        hint: "-",
        keywords: "bullet list ul wuxu liebiao wx wxlb",
        icon: &LIST,
    },
    SlashCmd {
        id: "ordered",
        label: "有序列表",
        desc: "数字编号列表",
        hint: "1.",
        keywords: "ordered list ol youxu bianhao number yx yxlb",
        icon: &LIST_ORDERED,
    },
    SlashCmd {
        id: "todo",
        label: "任务待办",
        desc: "可勾选复选框",
        hint: "[]",
        keywords: "todo task check daiban renwu db checklist",
        icon: &CHECK_SQUARE,
    },
    SlashCmd {
        id: "quote",
        label: "引用块",
        desc: "左侧竖线引文",
        hint: ">",
        keywords: "quote blockquote yinyong yy",
        icon: &QUOTE,
    },
    SlashCmd {
        id: "callout",
        label: "高亮块",
        desc: "带图标与背景的提示框",
        hint: "!>",
        keywords: "callout highlight tip gaoliang tishi sk sikao ts 思考",
        icon: &BULB,
    },
    SlashCmd {
        id: "code",
        label: "代码块",
        desc: "等宽代码与行号切换",
        hint: "```",
        keywords: "code block daima dm dmk",
        icon: &CODE,
    },
    SlashCmd {
        id: "mermaid", label: "Mermaid图表", desc: "流程图、时序图与关系图", hint: "图表",
        keywords: "mermaid diagram flowchart tubiao liuchengtu", icon: &CODE,
    },
    SlashCmd {
        id: "math", label: "数学公式", desc: "LaTeX 数学公式", hint: "∑",
        keywords: "math latex tex formula gongshi shuxue", icon: &TYPE,
    },
    SlashCmd {
        id: "divider",
        label: "分割线",
        desc: "水平分隔线",
        hint: "---",
        keywords: "divider hr line fengexian",
        icon: &MINUS,
    },
    SlashCmd {
        id: "table",
        label: "表格",
        desc: "可编辑多列表格",
        hint: "3×3",
        keywords: "table grid biaoge bg",
        icon: &TABLE,
    },
    SlashCmd {
        id: "image",
        label: "图片",
        desc: "从本地选择或插入图片",
        hint: "IMG",
        keywords: "image picture tupian img tp photo",
        icon: &IMAGE,
    },
    SlashCmd {
        id: "bold",
        label: "加粗",
        desc: "切换粗体输入样式",
        hint: "⌘B",
        keywords: "bold jiacu cuti strong b jc",
        icon: &BOLD,
    },
    SlashCmd {
        id: "italic",
        label: "斜体",
        desc: "切换斜体输入样式",
        hint: "⌘I",
        keywords: "italic xieti em i xt",
        icon: &ITALIC,
    },
    SlashCmd {
        id: "underline",
        label: "下划线",
        desc: "切换下划线样式",
        hint: "⌘U",
        keywords: "underline xiahuaxian u xhx",
        icon: &UNDERLINE,
    },
    SlashCmd {
        id: "strike",
        label: "删除线",
        desc: "切换删除线样式",
        hint: "⇧⌘X",
        keywords: "strike strikethrough shanchuxian s scx",
        icon: &STRIKETHROUGH,
    },
    SlashCmd {
        id: "inline_code",
        label: "行内代码",
        desc: "切换等宽代码片段",
        hint: "⌘E",
        keywords: "inline code hangneidaima e hndm hangnei",
        icon: &TERMINAL,
    },
];

pub fn filtered(query: &str) -> Vec<SlashCmd> {
    let q = query.trim().to_lowercase();
    SLASH_CMDS
        .iter()
        .copied()
        .filter(|cmd| cmd.matches(&q))
        .collect()
}
pub fn render(
    s: &State,
    key: &str,
    query: &str,
    selected: usize,
    viewport_height: f32,
    choose: fn(&mut State, &str, &mut Tx),
) -> El {
    let theme = s.theme();
    let cmds = filtered(query);
    let empty = query.trim().is_empty();
    let grid_count = if empty { 4 } else { 0 };
    let natural = 36. + if empty { 48. } else { 0. } + ((cmds.len() - grid_count) as f32 * 32. - 2.).max(0.);
    let height = natural.max(66.).min(360.).min((viewport_height - 24.).max(66.));
    let mut list = col()
        .key("insert-menu-options").instant_changes()
        .w_full()
        .gap(2.)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .max_h(height - 36.).min_h(0.).shrink();
    if empty {
        let mut grid = row().w_full().gap(4.).mb(6.);
        for (idx, cmd) in cmds.iter().take(4).enumerate() {
            let cid = cmd.id;
            grid = grid.child(
                el().key(format!("slash-item-{cid}"))
                    .w(53.)
                    .h(40.)
                    .radius(6.)
                    .center()
                    .fill(if selected == idx {
                        c(crate::ui::accent_soft(s))
                    } else {
                        c(crate::ui::editor_colors(theme).inset)
                    })
                    .hover(c(crate::ui::accent_soft(s)))
                    .role("button")
                    .label(s.tr(cmd.label))
                    .help(s.tr(cmd.label))
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut State, tx| choose(s, cid, tx))
                    .child(paper_icon(cmd.icon, 26., category_tint(cmd.icon), theme)),
            );
        }
        list = list.child(grid);
    }
    for (idx, cmd) in cmds.iter().enumerate().skip(grid_count) {
        let cid = cmd.id;
        list = list.child(
            row()
                .key(format!("slash-item-{cid}"))
                .w_full()
                .h(30.)
                .px(8.)
                .gap(10.)
                .radius(5.)
                .center_y()
                .fill(if selected == idx {
                    c(crate::ui::accent_soft(s))
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(c(crate::ui::editor_colors(theme).inset))
                .cursor(Cursor::Pointer)
                .role("button")
                .label(s.tr(cmd.label))
                .on_press_tx(move |s: &mut State, tx| choose(s, cid, tx))
                .child(paper_icon(cmd.icon, 22., category_tint(cmd.icon), theme))
                .child(label(s.tr(cmd.label), 13., if selected == idx { 550. } else { 450. }, if selected == idx { crate::ui::accent(s) } else { crate::ui::theme_ink(theme) }).grow(1.))
                .child(label(cmd.hint, 11., 400., crate::ui::theme_subtle(theme))),
        );
    }
    if cmds.is_empty() {
        list = list.child(
            label(s.tr("没有匹配的模块"), 13., 400., crate::ui::theme_muted(theme))
                .key("doc-slash-empty")
                .padding(8.),
        );
    }
    crate::overlays::surface(s, col()
        .key(key)
        // The menu belongs to the active editor: pointer-down must not blur it
        // and remove the menu before pointer-up dispatches the chosen command.
        .preserve_focus_on_press(true)
        // Rune resolves this against the trigger's geometry in the same layout pass.
        // Reading cached window rects here would position the first frame one edit behind.
        .anchored_fit("doc-slash-anchor", rune::Side::Bottom, rune::Align::Start, 6.)
        .anchored_unscaled()
        .w(247.)
        .max_h(height).min_h(0.)
        .padding(8.)
        .gap(6.)
        , 12.)
        // A caret-anchored menu must not travel into place after it appears.
        .transition(crate::overlays::fade_transition())
        .child(
            label(
                if empty {
                    s.tr("基础类型")
                } else {
                    s.tr("筛选结果")
                },
                11.,
                500.,
                crate::ui::theme_muted(theme),
            )
            .h(14.)
            .px(4.),
        )
        .child(list)
}
