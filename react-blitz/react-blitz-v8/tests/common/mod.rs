//! Open a test page in a `ReactDocument` over V8: bundled by `react-blitz/js/scripts/bundle.mjs`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_test_harness::Harness;
use blitz_traits::shell::{ColorScheme, Viewport};
use react_blitz::{DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_v8::V8Engine;

pub type Page = Harness<ReactDocument<V8Engine>>;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Bundle `tests/pages/<name>` into the target folder and open it at 800x600.
pub fn open(name: &str) -> Page {
    let page = manifest().join("tests/pages").join(name);
    let out = manifest().join("../../target/react-blitz-pages").join(name);
    let js = manifest().join("../js");
    let status = Command::new("node")
        .arg(js.join("scripts/bundle.mjs"))
        .arg(&page)
        .arg(&out)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling {name} failed");
    open_dist(&out)
}

pub fn open_dist(dist: &Path) -> Page {
    let loader = Rc::new(DistLoader::new(dist).unwrap());
    let engine = V8Engine::new(loader).unwrap();
    let doc = ReactDocument::open(
        ReactDocumentConfig {
            dist: dist.to_path_buf(),
            page: "index.html".into(),
            query: String::new(),
            document: DocumentConfig {
                viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
                ..Default::default()
            },
            clock: Default::default(),
        },
        engine,
    )
    .unwrap();
    let mut page = Harness::wrap(doc);
    page.pump();
    page
}
