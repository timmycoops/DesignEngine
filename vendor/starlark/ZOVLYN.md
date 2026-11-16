# starlark 0.14.2, patched

The `starlark` crate 0.14.2 from crates.io (https://github.com/facebook/starlark-rust), Copyright 2018 The Starlark in
Rust Authors and Facebook, Inc. and its affiliates, licensed under the Apache License, Version 2.0
(https://www.apache.org/licenses/LICENSE-2.0). Used by `crates/zd-script` through `[patch.crates-io]` in the workspace's
`Cargo.toml` (decision 0077).

Changes from the published crate:

- `Cargo.toml`: the `arbitrary_precision` feature of `serde_json` is no longer asked for. Cargo unifies a dependency's
  features across the build, and that feature breaks serde's untagged enums and flattened structs holding numbers in
  every crate of the app (attribute values, parameter values, face steps, dataflow fields). Without it Starlark's
  `json.encode` gives integers wider than 64 bits as floats.
- `src/tests/` and the crate's `testcases/` are left out: they build only for Starlark's own tests.

To move to a newer Starlark: copy the new crate's `src`, `Cargo.toml` and `build.rs` here, make the same two changes,
and run `cargo test -p zd-script` (its `serde_json_reads_numbers_as_plain_floats` test fails if the feature is back).
