//! Each golden file in `react-blitz/tests/golden/` is a list of commits and the tree they must
//! make. `react-blitz/js`'s encoder test writes the `.bin` files from React trees; this test reads
//! the same files, so the encoder and the decoder cannot drift.
//!
//! A `.bin` file is its commits one after another, each `[ops bytes: u32][strings bytes: u32]`, the
//! ops, then the strings. JS id 1 is the container: the document's `<body>`.

use std::fmt::Write;
use std::path::PathBuf;

use blitz_commit::Tree;
use blitz_dom::{BaseDocument, NodeData, NodeId};
use blitz_test_harness::Harness;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/golden")
}

fn commits(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let (ops_len, strings_len) = (word(at), word(at + 4));
        let ops = &bytes[at + 8..at + 8 + ops_len];
        let strings = &bytes[at + 8 + ops_len..at + 8 + ops_len + strings_len];
        out.push((ops, strings));
        at += 8 + ops_len + strings_len;
    }
    out
}

/// One line per node under `node`: an element with its attributes sorted, its inline style and its
/// listeners; a text node quoted.
fn outline(doc: &BaseDocument, tree: &Tree, node: NodeId, depth: usize, out: &mut String) {
    let n = doc.get_node(node).unwrap();
    let pad = "  ".repeat(depth);
    match &n.data {
        NodeData::Text(text) => writeln!(out, "{pad}{:?}", text.content).unwrap(),
        NodeData::Element(el) => {
            let ns = match &*el.name.ns {
                "http://www.w3.org/2000/svg" => "svg:",
                "http://www.w3.org/1998/Math/MathML" => "math:",
                _ => "",
            };
            write!(out, "{pad}<{ns}{}", el.name.local).unwrap();
            let mut attrs: Vec<_> = el
                .attrs()
                .iter()
                .filter(|a| &*a.name.local != "style")
                .map(|a| (a.name.local.to_string(), a.value.clone()))
                .collect();
            attrs.sort();
            for (name, value) in attrs {
                write!(out, " {name}={value:?}").unwrap();
            }
            if let Some(style) = &el.style_attribute {
                let guard = doc.guard().read();
                let mut css = String::new();
                style.read_with(&guard).to_css(&mut css).unwrap();
                if !css.is_empty() {
                    write!(out, " style={css:?}").unwrap();
                }
            }
            for event in tree.listeners(node) {
                write!(out, " on:{event}").unwrap();
            }
            writeln!(out, ">").unwrap();
        }
        _ => {}
    }
    for &child in &n.children {
        outline(doc, tree, child, depth + 1, out);
    }
}

fn run(family: &str) {
    let dir = golden_dir();
    let bytes = std::fs::read(dir.join(format!("{family}.bin")))
        .unwrap_or_else(|e| panic!("{family}.bin: {e}"));
    let want = std::fs::read_to_string(dir.join(format!("{family}.tree")))
        .unwrap_or_else(|e| panic!("{family}.tree: {e}"));

    let mut harness = Harness::from_html("<html><body></body></html>");
    let mut tree = Tree::new();
    let body = harness.query("body").unwrap();
    tree.bind(1, body);
    for (i, (ops, strings)) in commits(&bytes).into_iter().enumerate() {
        let mut doc = harness.base_mut();
        tree.apply(&mut doc, ops, strings)
            .unwrap_or_else(|e| panic!("{family}.bin commit {i}: {e}"));
    }
    harness.pump();

    let doc = harness.base();
    let mut got = String::new();
    outline(&doc, &tree, body, 0, &mut got);
    assert_eq!(got.trim_end(), want.trim_end(), "{family}: the tree differs");
}

#[test]
fn create() {
    run("create");
}

#[test]
fn attrs() {
    run("attrs");
}

#[test]
fn style() {
    run("style");
}

#[test]
fn text() {
    run("text");
}

#[test]
fn tree() {
    run("tree");
}

#[test]
fn listen() {
    run("listen");
}
