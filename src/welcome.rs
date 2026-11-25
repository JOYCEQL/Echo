//! Editable examples for a new library, written through the normal Markdown store.
use crate::{
    doc_core::{DocBlockKind as K, DocDocument, NoteFormat, RichText, TextMarks},
    model::{Block, Kind, Note},
    storage::Store,
    table_data::TableData,
};

pub const NOTEBOOKS: &str = "工作,产品,设计,学习,生活,灵感";
const IMAGE: &str = "images/welcome-walk.png";

fn paragraph(doc: &mut DocDocument, kind: K, text: &str) {
    let b = doc.new_block(kind, text);
    doc.blocks.push(b);
}
fn heading(doc: &mut DocDocument, level: u8, text: &str) {
    paragraph(doc, K::Heading { level }, text);
}
fn section(title: &str) -> DocDocument {
    let mut doc = DocDocument::empty();
    doc.blocks.clear();
    heading(&mut doc, 2, title);
    doc
}

fn container(doc: &mut DocDocument, kind: K, text: &str) {
    let child = doc.new_block(K::Text, text);
    let block = doc.new_container(kind, child);
    doc.blocks.push(block);
}

fn route_image(doc: &mut DocDocument, width: f32) {
    paragraph(doc, K::Image { src: IMAGE.into(), width: Some(width),
        caption: "周末散步草图 · 旧书店、河边、面包房".into() }, "");
}

fn itinerary(doc: &mut DocDocument) {
    let rows = [["停靠点", "留多久", "想做什么"], ["旧书店", "15 分钟", "挑一本薄书"],
        ["河边", "30 分钟", "找张长椅读几页"], ["面包房", "15 分钟", "带走两个肉桂卷"]];
    paragraph(doc, K::Table(TableData {
        rows: rows.into_iter().map(|row| row.into_iter().map(String::from).collect()).collect(),
        widths: vec![1., 1., 1.6], align: vec![0, 0, 0],
    }), "");
}

fn guide() -> DocDocument {
    let mut doc = DocDocument::empty();
    doc.title = RichText::plain("欢迎使用 Echo · 结构化入门");
    doc.blocks.clear();
    paragraph(&mut doc, K::Text, "你现在打开的是 Echo 的「工作」空间。你可以在这里写文档、摆放画布，所有内容都保存在自己的电脑上，断网也能用。这里有两页可以随便改的示例：这一页带你把想法写成文章，旁边的《周末散步 · 自由画布入门》留给还没排好顺序的材料。");
    heading(&mut doc, 2, "从一个小计划写起");
    paragraph(&mut doc, K::Text, "假设周六多出一小时，你想去哪儿？点进下面这句话，把目的地换成你常去的地方。Echo 会自动保存，下次打开可以接着写。");
    let mut sentence = doc.new_block(K::Text, "");
    sentence.text.push("这周六，我想去", TextMarks::default());
    sentence.text.push("河边的旧书店", TextMarks { bold: true, ..Default::default() });
    sentence.text.push("，挑一本能在长椅上读完的小书。", TextMarks::default());
    doc.blocks.push(sentence);
    paragraph(&mut doc, K::Todo { done: false }, "改好目的地后，点一下这个复选框");
    heading(&mut doc, 2, "给长文搭一个骨架");
    paragraph(&mut doc, K::Text, "在新的一行输入 /，选择「标题」。同一层的标题适合并列的章节，小标题可以放在章节里面。点击标题旁的箭头可以折叠这一节。");
    heading(&mut doc, 3, "随身带什么");
    let mut bag = doc.new_block(K::Bullet, "帆布袋");
    bag.children.push(doc.new_block(K::Bullet, "装书，再留一点位置给面包"));
    doc.blocks.push(bag);
    paragraph(&mut doc, K::Bullet, "水壶和一支铅笔");
    paragraph(&mut doc, K::Text, "点进最后一项按回车，添一样你会带的东西。Tab 可缩进一层，Shift + Tab 可退回。选中几个字，还可以加粗、高亮或加上链接。");
    container(&mut doc, K::Quote, "朋友说：“书店可以晚点关门，肉桂卷可不会等人。”");
    container(&mut doc, K::Callout { emoji: "💡".into(), palette: "orange".into() }, "把别人说的话放进引用，把自己临出门要记得的事放进提示块。点左边的图标可以换颜色。");
    paragraph(&mut doc, K::Divider, "");
    heading(&mut doc, 2, "把路线放进正文");
    paragraph(&mut doc, K::Text, "这是给这次散步画的草图。单击图片，拖动角上的控制点等比缩放，再试试左对齐、居中和右对齐。要换成自己的照片，在空行输入 / 并选择图片。");
    route_image(&mut doc, 440.);
    paragraph(&mut doc, K::Text, "下面三站刚好一小时。点进表格改一个停留时间，按 Tab 走到下一格。");
    itinerary(&mut doc);
    heading(&mut doc, 3, "留一份能复制的清单");
    paragraph(&mut doc, K::Text, "代码块会保留原来的换行。下面这段 JSON 是路线的另一种写法，可以点「复制」带走。写技术笔记时，/ 菜单里也有 Mermaid 图表和数学公式。");
    paragraph(&mut doc, K::Code { lang: "JSON".into(), wrap: true }, r#"{
  "路线": ["旧书店", "河边", "面包房"],
  "分钟": 60
}"#);
    heading(&mut doc, 2, "建一页自己的笔记");
    for step in ["点左侧空间列表下方的「+」，新建一个叫「周末」的空间。散步计划和路线画布都可以放在这里。",
        "点右上角的「新建页面」，选择结构化文档，写下标题。如果素材还很零散，就选自由画布，把每条线索放在一张卡片里。",
        "已有 Markdown 文件时，点「新建页面」旁的导入按钮，或把 .md 文件拖进窗口。要把笔记带走，打开页面菜单，选择「导出 Markdown」。"] {
        paragraph(&mut doc, K::Ordered { start: None }, step);
    }
    paragraph(&mut doc, K::Text, "找回一条笔记，可以点左上角的放大镜搜标题或正文。误删的页面可以去「设置 → 回收站」恢复；要备份所有空间，在「设置 → 本地数据」打开数据目录，退出 Echo 后复制整个目录，图片和画布布局也在里面。");
    heading(&mut doc, 2, "接下来，去画布上试试");
    paragraph(&mut doc, K::Text, "在左侧页面列表打开《周末散步 · 自由画布入门》。试着把路线图拖到行程表旁边，你就有了第一张按自己习惯摆放的计划板。");
    doc
}

fn renumber(doc: &mut DocDocument) {
    fn walk(blocks: &mut [crate::doc_core::DocBlock], next: &mut u64) {
        for b in blocks { b.id = *next; *next += 1; walk(&mut b.children, next); }
    }
    let mut next = 1;
    walk(&mut doc.blocks, &mut next);
    doc.next_id = next;
}

fn note(id: &str, title: &str, preview: &str, format: NoteFormat) -> Note {
    Note { id: id.into(), notebook: "工作".into(), original_book: "工作".into(), parent_id: None,
        title: title.into(), preview: preview.into(),
        stamp: "刚刚".into(), updated: crate::model::now(), blocks: vec![], zoom: 1., pan: [0., 0.],
        editor_version: 7, format, doc: None }
}

pub fn notes() -> Result<Vec<Note>, String> {
    let doc = guide();
    let mut document = note("welcome-document", &doc.title.text, &doc.preview_text(), NoteFormat::Document);
    document.doc = Some(doc);

    let mut canvas = note("welcome-canvas", "周末散步 · 自由画布入门", "挪动路线图，勾掉行前待办，给自己排一条散步路线。", NoteFormat::Canvas);
    let mut intro = section("给周末留一小时");
    paragraph(&mut intro, K::Text, "欢迎来到 Echo 的自由画布。这是一块可以动手改的散步计划板。试着把路线图挪近行程表，或点一处空白，写下你想去的第四站。");

    let mut route = section("一张路线草图");
    route_image(&mut route, 325.);
    paragraph(&mut route, K::Text, "拖住卡片上沿的把手移动整张草图。要放大图本身，单击图片再拖动角点。");

    let mut bag = section("出门之前");
    paragraph(&mut bag, K::Todo { done: true }, "定好周六下午出门");
    paragraph(&mut bag, K::Todo { done: false }, "把水壶放进帆布袋");
    paragraph(&mut bag, K::Todo { done: false }, "问问朋友想吃哪种面包");
    paragraph(&mut bag, K::Text, "直接勾选，或在最后一项按回车，再加一件自己的事。");

    let mut plan = section("一小时怎么分");
    itinerary(&mut plan);
    paragraph(&mut plan, K::Text, "把你最想久留的一站多分十分钟。拖卡片右边缘能调宽整张表所在的卡片。");

    let mut clipping = section("路上听来的话");
    container(&mut clipping, K::Quote, "“沿河走，过第二张长椅就能闻到面包香。”");
    paragraph(&mut clipping, K::Bullet, "如果下雨，就在书店多待一会儿。");
    paragraph(&mut clipping, K::Divider, "");
    container(&mut clipping, K::Callout { emoji: "📌".into(), palette: "orange".into() }, "这里把引用、列表和提示放在了同一张卡片里。另起一行输入 /，可以继续加别的内容。");

    let mut log = section("散步回来");
    paragraph(&mut log, K::Ordered { start: None }, "点空白处写下一件沿途发现的小事。");
    paragraph(&mut log, K::Ordered { start: None }, "把它拖到想一起看的卡片旁边。右下角可以缩放视图，点百分比回到原始比例。");
    paragraph(&mut log, K::Text, "也可以复制下面这段日志，留给下一次散步。");
    paragraph(&mut log, K::Code { lang: "Plain Text".into(), wrap: true }, "最想再去的一站：
沿途记住的一句话：");
    for (i, (mut section, x, y, w, h)) in [
        (intro, 32., 84., 640., 142.), (route, 32., 260., 394., 310.),
        (bag, 450., 274., 338., 260.), (plan, 32., 624., 480., 310.),
        (clipping, 538., 624., 315., 330.), (log, 92., 1010., 540., 320.),
    ].into_iter().enumerate() {
        renumber(&mut section);
        let mut block = Block::new(Kind::Text, x, y, w, h, "", &crate::file_format::write_document_body(&section)?);
        block.id = format!("welcome-card-{i}");
        block.structured = true;
        block.min_h = 40.;
        canvas.blocks.push(block);
    }
    Ok(vec![document, canvas])
}

pub fn initialize(store: &mut Store) -> Result<Vec<Note>, String> {
    let notes = notes()?;
    std::fs::write(store.root.join(IMAGE), include_bytes!("../assets/welcome-walk.png")).map_err(|e| e.to_string())?;
    store.save_with_preferences(&notes, &notes[0].id, &[("book_order", NOTEBOOKS)])?;
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn welcome_examples_roundtrip_all_formats_in_both_editors() {
        let root = std::env::temp_dir().join(format!("echo-welcome-{}", crate::model::uid()));
        let mut store = Store::open(root.clone()).unwrap();
        let notes = initialize(&mut store).unwrap();
        assert_eq!(store.load().unwrap(), notes);
        assert_eq!(store.preference("book_order").as_deref(), Some(NOTEBOOKS));
        assert!(root.join(IMAGE).exists());
        let types = |doc: &DocDocument| {
            let mut types = std::collections::BTreeSet::new();
            fn walk(blocks: &[crate::doc_core::DocBlock], types: &mut std::collections::BTreeSet<String>) {
                for b in blocks { types.insert(format!("{:?}", std::mem::discriminant(&b.kind))); walk(&b.children, types); }
            }
            walk(&doc.blocks, &mut types); types
        };
        let expected = types(notes[0].doc.as_ref().unwrap());
        assert_eq!(expected.len(), 11);
        let mut actual = std::collections::BTreeSet::new();
        for b in &notes[1].blocks {
            assert!(b.structured);
            let mut doc = DocDocument::empty(); doc.blocks.clear();
            crate::file_format::read_document_body(&b.body, &mut doc).unwrap();
            actual.extend(types(&doc));
        }
        assert_eq!(actual, expected);
        std::fs::remove_dir_all(root).unwrap();
    }
}
