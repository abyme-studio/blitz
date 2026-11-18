# vendor

Crates Blitz takes from crates.io that the fork has to change. Each is a copy of the published
crate with the fixes below, taken by `[patch.crates-io]` in `../Cargo.toml`. A fix upstream has
taken comes off; a folder with no fixes left is deleted with its patch line.

## stylo 0.24.0

- `values/specified/length.rs`: the container units (`cqw`, `cqh`, `cqi`, `cqb`, `cqmin`,
  `cqmax`) parse in the servo build too. Upstream parses them only with `feature = "gecko"`.
  They resolve against `TElement::query_container_size`, which blitz-dom answers
  (`fix/container-units`).
- `stylesheets/rule_parser.rs`: `@container` parses under the pref
  `layout.css.container-queries.enabled`, which blitz-dom sets. Upstream parses it only with
  `feature = "gecko"`.
- `properties/shorthands.toml`, `properties/shorthands.rs`: the `container` shorthand
  (`container: page / inline-size`) parses under the same pref. Upstream has it for Gecko alone,
  so a container named by the shorthand was no container, and its `@container page` rules never
  applied.
- `properties/longhands.toml`: `scrollbar-gutter` is a property of the servo build too. Upstream
  has it for Gecko alone. stylo_taffy reads it under its `scrollbar-gutter` feature
  (`fix/scrollbar-gutter`), which `react-blitz` turns on.

To see the fixes: `diff -r ~/.cargo/registry/src/*/stylo-0.24.0 vendor/stylo`.

## anyrender_vello 0.16.0

- `window_renderer.rs`: `VelloWindowRenderer::warm` builds the device and Vello's compute
  pipelines on a thread of their own, from the renderer's making, and the first `resume` takes
  them. Upstream builds them inside `resume`, on the main thread, where Metal takes about half a
  second for them before the window's first frame.
- `window_renderer.rs`: the renderer builds the pipelines of its own antialiasing method only.
  Upstream builds all three (`AaSupport::all()`), two of which a window never draws with.
- `frame_times.rs`, `scene.rs`, `window_renderer.rs`: the renderer times a frame's stages on its
  own thread (`take_frame_times`): encoding the scene, acquiring the surface, the render, the
  present and the wait for the GPU, and counts the glyph runs and glyphs drawn. Upstream times
  none of them. `react-blitz-window` logs them per frame.
- `scene.rs`, `image_renderer.rs`, `window_renderer.rs`: the painter keeps fragments
  (`PaintScene::begin_fragment`, below), each a `vello::Scene` in the renderer, drawn again by
  Vello's own `append`. Upstream has none.

## anyrender 0.15.0

- `lib.rs`: `PaintScene` has `begin_fragment`, `end_fragment` and `append_fragment`: what is
  drawn between the first two is set aside in the back end's own form under a `ResourceId`, and
  the third draws it again under a transform, in a later frame too. Each is a provided method
  that answers no, so a back end without them draws as before. blitz-paint keeps a scroll
  container's content with them (`fix/scene-fragments`, `keep_fragments`).

To see the fixes: `diff -r ~/.cargo/registry/src/*/anyrender-0.15.0 vendor/anyrender`.

## vello_encoding 0.12.0

- `glyph_cache.rs`, `lib.rs`: `take_glyph_outlines_encoded` counts the glyph outlines the
  resolver's cache did not hold and so encoded. Upstream counts none, and the cache is private.
  anyrender_vello's `take_frame_times` reads it, and `react-blitz-window` logs it per frame.

To see the fixes: `diff -r ~/.cargo/registry/src/*/vello_encoding-0.12.0 vendor/vello_encoding`.
