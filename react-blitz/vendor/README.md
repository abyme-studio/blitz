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

To see the fixes: `diff -r ~/.cargo/registry/src/*/stylo-0.24.0 vendor/stylo`.
