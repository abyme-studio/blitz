//! `BaseDocument::text_width` answers the width the document itself lays the same text out at.

use blitz_dom::DocumentConfig;
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use std::sync::Arc;

const HTML: &str = r#"<!DOCTYPE html>
<html><head><style>
    body { margin: 0; }
    span { font: 20px sans-serif; }
    #bold { font: italic 700 13px serif; }
</style></head>
<body>
<span id="plain">South East Asia</span><br>
<span id="bold">1,250,000</span>
</body></html>
"#;

#[test]
fn a_text_measures_as_wide_as_it_is_laid_out() {
    let mut doc = HtmlDocument::from_html(
        HTML,
        DocumentConfig {
            viewport: Some(Viewport::new(800, 300, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    let laid = |selector: &str| {
        let node = doc.query_selector(selector).unwrap().unwrap();
        doc.get_client_bounding_rect(node).unwrap().width as f32
    };

    let plain = doc.text_width("South East Asia", "sans-serif", 20.0, 400.0, false);
    assert!(
        plain > 0.0 && (plain - laid("#plain")).abs() < 0.5,
        "{plain} against {}",
        laid("#plain")
    );
    let bold = doc.text_width("1,250,000", "serif", 13.0, 700.0, true);
    assert!(
        bold > 0.0 && (bold - laid("#bold")).abs() < 0.5,
        "{bold} against {}",
        laid("#bold")
    );

    assert_eq!(doc.text_width("", "sans-serif", 20.0, 400.0, false), 0.0);
    assert!(doc.text_width("South East Asia", "sans-serif", 40.0, 400.0, false) > plain * 1.9);
}
