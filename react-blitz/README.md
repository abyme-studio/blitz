# react-blitz

React drawn by Blitz. This folder is everything the fork `abyme-studio/blitz` adds to upstream
Blitz, as its own Cargo workspace: upstream's root workspace never lists these crates, so a rebase
on upstream touches none of them.

- `blitz-commit/`: the wire format. It decodes one React commit (ops and a string table) and applies
  it to a `BaseDocument` through one `DocumentMutator`. It is the one owner of the format.
- `js/`: the npm package `react-blitz`: the React host config, the encoder, event dispatch, and the
  DOM subset its nodes answer.
- `react-blitz/`: `ReactDocument` (Blitz's `Document` over a `BaseDocument` and a JS engine) and the
  `ScriptEngine` trait.
- `react-blitz-v8/`: V8 through `deno_core`, behind `ScriptEngine`.
- `react-blitz-shot/`: paints a built page to a PNG, offscreen.
- `tests/golden/`: one commit file per op family, written by `js/`'s encoder test and read by
  `blitz-commit`'s test.
- `vendor/`: patched copies of crates Blitz takes from crates.io (Stylo, Parley), until a release
  has the fix. `vendor/README.md` names each one's upstream pull request.

## The rebase rule

The fork's `main` is upstream's `main` plus this folder, plus one commit per fix to Blitz's own
crates (`packages/*`), each one feature, as upstream asks. Rebase on upstream weekly, never merge. A
carried fix drops out once upstream has it. Nothing fork-only lives outside `react-blitz/`.

## Build

```sh
cd react-blitz && cargo test
```

`cargo build` at the fork's root does not build this folder.
