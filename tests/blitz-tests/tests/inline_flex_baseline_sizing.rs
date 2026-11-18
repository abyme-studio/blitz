//! A line holding an inline-flex box must be measured as tall as it is laid out. A flex
//! container computes its baseline in its layout pass, so a line sized without it sat the
//! box on the baseline and came out taller: a flex item's text was sized taller than a
//! plain block's with the same content.

use blitz_dom::{BaseDocument, DocumentConfig, NodeId};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use std::sync::Arc;

const HTML: &str = r#"<!DOCTYPE html>
<html><head><style>
    body { margin: 0; font-size: 14px; line-height: 20px; }
    .row { display: flex; align-items: flex-start; }
    .text { flex: 1 1 0%; }
    .badge { display: inline-flex; align-items: center; height: 18px; font-size: 10px; line-height: 14px; }
</style></head>
<body>
<div id="plain">On close <span class="badge">Recommended</span></div>
<div class="row"><div id="item" class="text">On close <span class="badge">Recommended</span></div></div>
</body></html>
"#;

fn node(doc: &BaseDocument, selector: &str) -> NodeId {
    doc.query_selector(selector).unwrap().unwrap()
}

#[test]
fn a_line_with_an_inline_flex_box_is_sized_as_it_is_laid_out() {
    let mut doc = HtmlDocument::from_html(
        HTML,
        DocumentConfig {
            viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    let plain = doc.get_client_bounding_rect(node(&doc, "#plain")).unwrap();
    let item = doc.get_client_bounding_rect(node(&doc, "#item")).unwrap();
    assert!(plain.height > 0.0);
    assert_eq!(
        item.height, plain.height,
        "a flex item's line is as tall as a plain block's"
    );
}
