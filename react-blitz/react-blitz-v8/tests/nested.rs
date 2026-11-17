//! A small React module renders a nested `div` tree with classes; Blitz lays it out.

mod common;

#[test]
fn nested_divs_lay_out() {
    let page = common::open("nested");
    assert!(page.doc.shared().state.borrow().errors.is_empty());
    let outer = page.layout_rect(".outer");
    let a = page.layout_rect(".a");
    let b = page.layout_rect(".b");
    assert_eq!((outer.width, outer.height), (220.0, 40.0), "{}", page.dom_string());
    assert_eq!((a.width, a.height), (50.0, 20.0), "{}", page.dom_string());
    assert_eq!((b.width, b.height), (150.0, 20.0), "{}", page.dom_string());
    assert_eq!(page.text_content(".b"), "text");
}
