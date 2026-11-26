//! Editable examples for a new library, written through the normal Markdown store.
use crate::{
    doc_core::{DocBlockKind as K, DocDocument, NoteFormat, RichText, TextMarks},
    model::{Block, Kind, Note},
    storage::Store,
    table_data::TableData,
};

pub const NOTEBOOKS: &str = "Work,Product,Design,Study,Life,Ideas";
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
        caption: "A weekend walk from the bookshop to the bakery by way of the river".into() }, "");
}

fn itinerary(doc: &mut DocDocument) {
    let rows = [["Stop", "Time", "Plan"], ["Bookshop", "15 min", "Pick a short book"],
        ["Riverside", "30 min", "Read on a bench"], ["Bakery", "15 min", "Buy two cinnamon rolls"]];
    paragraph(doc, K::Table(TableData {
        rows: rows.into_iter().map(|row| row.into_iter().map(String::from).collect()).collect(),
        widths: vec![1., 1., 1.6], align: vec![0, 0, 0],
    }), "");
}

fn guide() -> DocDocument {
    let mut doc = DocDocument::empty();
    doc.title = RichText::plain("Welcome to Echo");
    doc.blocks.clear();
    paragraph(&mut doc, K::Text, "This is your Work space. Documents and canvases live on your computer, so you can edit them offline. Try writing a plan on this page, then open Weekend Walk to arrange the same ideas on a canvas. Both pages are yours to change.");
    heading(&mut doc, 2, "Start with a small plan");
    paragraph(&mut doc, K::Text, "If you had an hour to spare on Saturday, where would you go? Click the sentence below and change the destination. Echo saves as you type.");
    let mut sentence = doc.new_block(K::Text, "");
    sentence.text.push("On Saturday I’ll stop at ", TextMarks::default());
    sentence.text.push("the bookshop by the river", TextMarks { bold: true, ..Default::default() });
    sentence.text.push(" and pick a book I can finish on a park bench.", TextMarks::default());
    doc.blocks.push(sentence);
    paragraph(&mut doc, K::Todo { done: false }, "Check this box when you’ve changed the destination");
    heading(&mut doc, 2, "Give your document an outline");
    paragraph(&mut doc, K::Text, "Type / on a new line and choose a heading. Use headings for sections and smaller headings for topics within them. Click the arrow beside a heading to fold its section.");
    heading(&mut doc, 3, "What to bring");
    let mut bag = doc.new_block(K::Bullet, "A canvas tote");
    bag.children.push(doc.new_block(K::Bullet, "Leave room for a book and a couple of rolls"));
    doc.blocks.push(bag);
    paragraph(&mut doc, K::Bullet, "A water bottle and a pencil");
    paragraph(&mut doc, K::Text, "Press Enter at the end of the list to add something you’d bring. Tab indents an item and Shift + Tab moves it back. Select a few words to make them bold, highlight them, or add a link.");
    container(&mut doc, K::Quote, "“The bookshop stays open late,” my friend said. “The cinnamon rolls go fast.”");
    container(&mut doc, K::Callout { emoji: "💡".into(), palette: "orange".into() }, "Use a quote for someone else’s words and a callout for something you want to remember. Click the callout icon to change its color.");
    paragraph(&mut doc, K::Divider, "");
    heading(&mut doc, 2, "Add a map and an itinerary");
    paragraph(&mut doc, K::Text, "Click the map and drag a corner to resize it. Try left, center, and right alignment. To add your own photo, type / on a blank line and choose Image.");
    route_image(&mut doc, 440.);
    paragraph(&mut doc, K::Text, "These three stops add up to an hour. Change a time in the table, then press Tab to move to the next cell.");
    itinerary(&mut doc);
    heading(&mut doc, 3, "Keep a list you can copy");
    paragraph(&mut doc, K::Text, "A code block keeps your line breaks. Click Copy to take this JSON itinerary with you. The / menu also has Mermaid diagrams and math blocks for technical notes.");
    paragraph(&mut doc, K::Code { lang: "JSON".into(), wrap: true }, r#"{
  "stops": ["Bookshop", "Riverside", "Bakery"],
  "minutes": 60
}"#);
    heading(&mut doc, 2, "Make a page of your own");
    for step in ["Click + below the spaces in the sidebar. Name your new space Weekend and keep your walking plans there.",
        "Choose New page in the upper right and start a document. For ideas you want to move around, choose a canvas and put each idea on its own card.",
        "Use the import button beside New page or drag a Markdown file into the window. To export a note, open its page menu and choose Export Markdown."] {
        paragraph(&mut doc, K::Ordered { start: None }, step);
    }
    paragraph(&mut doc, K::Text, "Click the magnifying glass to search titles and text. Restore deleted pages in Settings → Trash. To back up your spaces, open the data folder from Settings → Local data, quit Echo, and copy the folder. It includes your pictures and canvas layouts.");
    heading(&mut doc, 2, "Try the canvas");
    paragraph(&mut doc, K::Text, "Open Weekend Walk from the sidebar. Drag the map next to the itinerary and arrange the rest of the cards in the order you’d like to read them.");
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
    Note { id: id.into(), notebook: "Work".into(), original_book: "Work".into(), parent_id: None,
        title: title.into(), preview: preview.into(),
        stamp: "Just now".into(), updated: crate::model::now(), blocks: vec![], zoom: 1., pan: [0., 0.],
        editor_version: 7, format, doc: None }
}

pub fn notes() -> Result<Vec<Note>, String> {
    let doc = guide();
    let mut document = note("welcome-document", &doc.title.text, &doc.preview_text(), NoteFormat::Document);
    document.doc = Some(doc);

    let mut canvas = note("welcome-canvas", "Weekend Walk", "Plan a walk by moving the map, editing the itinerary, and checking off what to bring.", NoteFormat::Canvas);
    let mut intro = section("An hour on Saturday");
    paragraph(&mut intro, K::Text, "Try moving the map beside the itinerary. Then click a blank spot on this canvas and write down a fourth stop you’d like to visit.");

    let mut route = section("The route");
    route_image(&mut route, 325.);
    paragraph(&mut route, K::Text, "Drag the handle at the top to move this card. To resize the map itself, click the picture and drag a corner.");

    let mut bag = section("Before you leave");
    paragraph(&mut bag, K::Todo { done: true }, "Pick a time on Saturday afternoon");
    paragraph(&mut bag, K::Todo { done: false }, "Pack a water bottle");
    paragraph(&mut bag, K::Todo { done: false }, "Ask what to bring back from the bakery");
    paragraph(&mut bag, K::Text, "Check off an item, or press Enter at the end of the list to add one.");

    let mut plan = section("An hour in three stops");
    itinerary(&mut plan);
    paragraph(&mut plan, K::Text, "Give your favorite stop ten extra minutes. Drag the right edge of this card to make more room for the table.");

    let mut clipping = section("Overheard on the way");
    container(&mut clipping, K::Quote, "“Follow the river. By the second bench you can smell the bread.”");
    paragraph(&mut clipping, K::Bullet, "If it rains, stay a little longer at the bookshop.");
    paragraph(&mut clipping, K::Divider, "");
    container(&mut clipping, K::Callout { emoji: "📌".into(), palette: "orange".into() }, "This card has a quote, a list, and a callout. Type / on a new line to add something else.");

    let mut log = section("After the walk");
    paragraph(&mut log, K::Ordered { start: None }, "Click a blank spot and jot down one thing you noticed.");
    paragraph(&mut log, K::Ordered { start: None }, "Drag your note beside a related card. Use the zoom controls at the bottom right or click the percentage to return to 100%.");
    paragraph(&mut log, K::Text, "Copy these prompts into your journal for the next walk.");
    paragraph(&mut log, K::Code { lang: "Plain Text".into(), wrap: true }, "A stop worth revisiting:
A line to remember:");
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
        assert_eq!(store.selected().as_deref(), Some("welcome-document"));
        assert_eq!(notes[0].title, "Welcome to Echo");
        assert_eq!(notes[1].title, "Weekend Walk");
        assert_eq!(notes[1].blocks.len(), 6);
        for note in &notes {
            assert_eq!(note.notebook, "Work");
            let saved = serde_json::to_string(note).unwrap();
            assert!(!saved.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)));
        }
        assert_eq!(std::fs::read(root.join(IMAGE)).unwrap(), include_bytes!("../assets/welcome-walk.png").as_slice());
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
