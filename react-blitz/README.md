# react-blitz

React drawn by Blitz. This folder is everything the fork `abyme-studio/blitz` adds to upstream
Blitz, as its own Cargo workspace: upstream's root workspace never lists these crates, so a rebase
on upstream touches none of them.

- `anyrender_quads/`: a drawing back end of instanced quads on wgpu, as Zed's GPUI draws (its shaders
  ported, Apache-2.0), behind `--renderer quads`; Vello stays the default. What it cannot draw yet
  is left out and counted.
- `blitz-commit/`: the wire format. It decodes one React commit (ops and a string table) and applies
  it to a `BaseDocument` through one `DocumentMutator`. It is the one owner of the format.
- `js/`: the npm package `react-blitz`: the React host config, the encoder, event dispatch, and the
  DOM subset its nodes answer.
- `react-blitz/`: `ReactDocument` (Blitz's `Document` over a `BaseDocument` and a JS engine) and the
  `ScriptEngine` trait.
- `react-blitz-v8/`: V8 through `deno_core`, behind `ScriptEngine`.
- `react-blitz-shot/`: paints a built page to a PNG, offscreen; and `react-blitz-window`, the page in a
  window, or with `--control` on a stepped 60 Hz clock that drive steers over a socket (`src/control.rs`).
- `tests/golden/`: one commit file per op family, written by `js/`'s encoder test and read by
  `blitz-commit`'s test.
- `vendor/`: patched copies of crates Blitz takes from crates.io (Stylo, Parley), until a release
  has the fix. `vendor/README.md` names each one's upstream pull request.

## Branches

The fork keeps each fix to Blitz on its own branch, so each can go upstream as it stands.

- `upstream/main`: Blitz's own main. Every branch below is cut from it.
- `fix/<topic>`: one fix to Blitz's own crates (`packages/*`), one commit, standing alone, ready to
  be a pull request. A fix to a vendored crate is `fix/stylo-<topic>` or `fix/parley-<topic>`, one
  commit on top of the vendor copy.
- `react-blitz`: this folder, never for upstream. Nothing fork-only lives outside `react-blitz/`.
- `abyme`: `upstream/main`, then each fix in the order `fixes` lists, then `react-blitz`, rebuilt by
  `recompose` (cherry-picks, never a merge) and force-pushed. abyme pins a sha on it, and each
  pinned sha has a tag `pin/<short-sha>` so it stays reachable after the next rebuild.
- `main`: frozen where it stood before this layout. Nothing is pushed to it.

Rebuild `abyme` on each new fix and whenever upstream moves, weekly at least:

```sh
react-blitz/recompose --push   # rebase each branch on upstream/main, rebuild and push abyme
react-blitz/recompose --pin    # the same, and tag the tip pin/<short-sha> for abyme to pin
```

A new fix: branch from `upstream/main`, commit once, push, add its name to `fixes`, recompose. A
fix upstream has taken comes off `fixes`. A test under `react-blitz/` that needs a fix passes on
`abyme`, not on `react-blitz` alone.

## Build

```sh
cd react-blitz && cargo test
```

`cargo build` at the fork's root does not build this folder.
