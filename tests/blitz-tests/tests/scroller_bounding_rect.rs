//! `BaseDocument::get_client_bounding_rect` of a scroll container must not move with
//! the container's own scroll offset: scrolling moves its content, not its box.

use blitz_dom::{BaseDocument, DocumentConfig, NodeId, ScrollBehavior};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use std::sync::Arc;

const HTML: &str = r#"<!DOCTYPE html>
<html><head><style>
    body { margin: 0; }
    #scroller { margin-top: 20px; width: 100px; height: 50px; overflow-y: scroll; }
    #content { height: 200px; }
</style></head>
<body>
<div id="scroller"><div id="content"></div></div>
</body></html>
"#;

fn node(doc: &BaseDocument, selector: &str) -> NodeId {
    doc.query_selector(selector).unwrap().unwrap()
}

#[test]
fn scrolling_a_container_moves_its_content_not_its_box() {
    let mut doc = HtmlDocument::from_html(
        HTML,
        DocumentConfig {
            viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    let scroller = node(&doc, "#scroller");
    let content = node(&doc, "#content");

    doc.scroll_to(scroller, 0.0, 30.0, ScrollBehavior::Instant);
    doc.resolve(0.0);

    let scroller_rect = doc.get_client_bounding_rect(scroller).unwrap();
    assert_eq!(
        (scroller_rect.y, scroller_rect.height),
        (20.0, 50.0),
        "the scroller's box stays where it is laid out"
    );
    let content_rect = doc.get_client_bounding_rect(content).unwrap();
    assert_eq!(content_rect.y, -10.0, "its content moves up by the offset");
}
