# xtask-core

[日本語](README_JP.md)

Repository rules and architecture diagrams for Rust workspaces, run through a
`cargo xtask` entry point.

A single `cargo xtask check` command:

- enforces a per-file code line limit,
- restricts which packages may depend on which (`visible_to`),
- requires a one-line `Role:` declaration on every crate root and module,
- rejects reference cycles between modules,
- optionally applies a mechanically checkable subset of the
  [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html),
- renders crate and module dependency graphs to SVG and a self-contained
  interactive HTML viewer.

## Requirements

- Rust 1.85 or later.
- A `Cargo.lock` and a populated dependency cache. Cargo is invoked with
  `--locked --offline`.
- Node.js, for writing diagrams. Set `XTASK_NODE` to use a binary other than
  `node` on `PATH`. A vendored copy of `elkjs` 0.12.0 is used, so no npm install
  or network access is required. `--no-write` does not need Node.js.
- `rustfmt` and `clippy` components, if style checks are enabled.

## Installation

Add this repository to the target repository, for example as a submodule at
`vendor/xtask-core`, then create an xtask package.

`dev/xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
xtask-core = { path = "../../vendor/xtask-core/crates/xtask-core" }
```

`dev/xtask/src/main.rs`:

```rust
//! Role: Run the shared repository checks.

fn main() -> std::process::ExitCode {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside dev")
        .parent()
        .expect("dev must be inside the repository");
    xtask_core::main(root)
}
```

`.cargo/config.toml`:

```toml
[alias]
xtask = "run --package xtask --"
```

Add `dev/xtask` to the workspace `members`, and create the policy file
described below.

## Usage

```sh
cargo xtask check              # check, then write SVG and HTML output
cargo xtask check --no-write   # check only; suitable for CI
cargo xtask check --root PATH  # check another repository
cargo xtask check --help
```

All violations are collected and reported together. Diagrams that can be
generated are written even when checks fail. The command exits with 0 on
success, 1 on violations or execution errors, and 2 on invalid arguments.

`--no-write` runs every check and validates diagram structure without writing
SVG or HTML files. When style checks are enabled, rustfmt and Clippy still run,
which produces Cargo build output under `target/` and executes build scripts
and procedural macros.

Both the normal and the `cfg(test)` configuration of each target are analyzed.

## Configuration

The policy is read from `dev/xtask/policy/workspace.toml`. Unknown keys are
rejected.

```toml
output = "architecture"   # SVG and HTML destination, relative to the root

[limits]
max_code_lines = 1000     # per file; at most 1000
max_role_chars = 80       # Role description length in Unicode characters

[visible_to]
internal = ["facade", "app-*"]
app = []

[diagrams]
targets = "all"           # "all", "lib-bin" or "lib"

[analysis]
target = "x86_64-unknown-linux-gnu"
features = ["app/feature"]
all_features = false
default_features = true
cfg = ["custom_cfg"]
trusted_attributes = ["serde"]
trusted_derives = ["Deserialize", "Serialize"]
trusted_macros = ["proptest", "prop_assert"]
trusted_globs = ["framework::prelude"]

[style]
enabled = true
```

### Line limit

Lines are counted excluding blank lines, comments and the contents of string
literals.

### Dependency visibility

Each key of `visible_to` names a package that is depended on; its value lists
the packages allowed to depend on it. A trailing `*` matches a prefix. `[]`
forbids all dependents, and packages without an entry are unrestricted. Keys
and patterns must match workspace members. Normal,
dev, build, optional and target-specific dependencies are all checked.

### Role declarations

Every crate root (library, binary, test, bench, example and build script) and
every module begins with a one-line inner documentation comment:

```rust
//! Role: Coordinate the application.

mod config {
    //! Role: Hold runtime settings.
}
```

For a module in its own file, the declaration goes at the top of that file; an
outer comment on `mod name;` does not count. `/*! Role: ... */` and
`#![doc = "Role: ..."]` are also accepted. The description must be nonempty and
no longer than `max_role_chars`.

### Diagrams

`[diagrams] targets` selects the targets that get module diagrams:

| Value | Targets |
| --- | --- |
| `"all"` (default) | every target |
| `"lib-bin"` | library kinds and binaries |
| `"lib"` | `lib`, `rlib`, `staticlib`, `cdylib`, `dylib`, `proc-macro` |

Unselected targets are still checked; only their diagrams are omitted.

### Analysis

By default, analysis uses the host target, Cargo's default features and no
extra `cfg`. `[analysis]` changes these settings for both source analysis and
Clippy.

`trusted_attributes`, `trusted_derives` and `trusted_macros` name items that
are assumed not to affect module structure. Calls to a trusted function-like
macro, matched by the exact path written at the call site, are not expanded,
so references inside them are not analyzed.

A glob import from another crate could provide any macro, derive or attribute,
so the analyzer cannot tell where `vec!` or `#[derive(Clone)]` come from in a
module that has one. `trusted_globs` names such imports by their `crate::module`
path when they are known to provide no macros, derives or attributes, such as a
framework's prelude of types and traits.

## Output

```text
architecture/
├── index.html
├── crates.svg
└── modules/<package>/
    ├── <target>-<kind>-normal.svg
    └── <target>-<kind>-test.svg
```

`index.html` is a single file with all SVG, CSS and JavaScript embedded. It
opens on the crate graph and supports:

- hovering a crate or module to highlight what it depends on and what depends
  on it, and clicking to pin the highlight (Esc or a click on empty space
  clears it);
- double-clicking a crate to open its module graph, with a selector for other
  targets and the test configuration;
- a tree of all crates, targets and modules in the side pane;
- panning by dragging and zooming with the wheel;
- keyboard navigation: Tab to move, Space to pin, Enter to open.

On each writing run, module SVGs under `<output>/modules` that were not
generated this run (for removed or unselected targets) are deleted. When
analysis fails, diagrams of targets that still exist are kept. `--no-write`
only reports what would be deleted.

## Style checks

`[style]` applies rules from the Google Rust Style Guide that can be checked
mechanically. It does not certify compliance with the whole guide: for
documentation and safety comments, only presence is checked.

`enabled = true` selects rules 1–13. Rules 14–20 need targets or reviewed
exceptions and are added through `extra_rules`.

| # | Rule | Checks | Implementation |
| --- | --- | --- | --- |
| 1 | `format` | Code matches the project's rustfmt configuration, whose effective `max_width` is at most 100. | rustfmt |
| 2 | `hex-literals` | Hexadecimal integer literals use lowercase digits, including inside macro calls. | syntax |
| 3 | `import-granularity` | One `use` imports from one module; no nested paths inside braces. | syntax |
| 4 | `unstable-features` | No `#![feature(...)]`. | rustc `unstable_features` |
| 5 | `unwrap` | No `unwrap` / `unwrap_err` on `Option` or `Result` outside test code. | Clippy `unwrap_used` |
| 6 | `checked-unwrap` | No unwrap after an explicit check. | Clippy `unnecessary_unwrap` |
| 7 | `flexible-references` | No `&Box<T>`, `&Vec<T>`, `&String` or `&Option<T>` parameters. | Clippy `borrowed_box`, `ptr_arg`, `ref_option` |
| 8 | `derivable-impls` | No manual impls that `derive` can replace. | Clippy `derivable_impls` |
| 9 | `new-default` | Public argument-less `new` has a `Default` impl. | Clippy `new_without_default` |
| 10 | `len-is-empty` | Public `len` has a matching `is_empty`. | Clippy `len_without_is_empty` |
| 11 | `missing-docs` | Public items are documented. | rustc `missing_docs` |
| 12 | `safety-docs` | Unsafe functions document a `# Safety` section. | Clippy `missing_safety_doc` |
| 13 | `safety-comments` | Unsafe blocks and impls have a `SAFETY:` comment. | Clippy `undocumented_unsafe_blocks` |
| 14 | `glob-imports` | No glob imports, except `super::*` in inline modules, `use Enum::*` in function bodies, `pub use child::*` in non-leaf modules and the paths in `allowed_globs`. | syntax |
| 15 | `non-leaf-implementation` | Modules with child modules contain only declarations, imports and re-exports; test modules and `#[cxx::bridge]` are allowed. | syntax |
| 16 | `rustfmt-skip` | No `#[rustfmt::skip]` in any form. | syntax |
| 17 | `non-exhaustive` | Public enums and all-public structs in `api_packages` are `#[non_exhaustive]`. | Clippy `exhaustive_enums`, `exhaustive_structs` |
| 18 | `catch-unwind` | No `std::panic::catch_unwind`. | Clippy `disallowed_methods` |
| 19 | `poison-access` | No `PoisonError::into_inner`, `get_ref` or `get_mut`. | Clippy `disallowed_methods` |
| 20 | `options-default` | Types listed in `options_structs` implement `Default`. | compile probe |

```toml
[style]
enabled = true
extra_rules = [
    "glob-imports",
    "non-leaf-implementation",
    "rustfmt-skip",
    "non-exhaustive",
    "catch-unwind",
    "poison-access",
    "options-default",
]
disabled_rules = ["missing-docs"]
api_packages = ["my-library"]
options_structs = ["my-library::options::Options"]
allowed_globs = ["framework::prelude"]
toolchain = "1.85.0"      # optional rustup toolchain

[[style.exceptions]]
rule = "rustfmt-skip"
path = "crates/my-library/src/generated/*"
reason = "Preserve the generator's layout."

[[style.exceptions]]
rule = "unwrap"
path = "crates/app/src/main.rs"
item = "startup"
reason = "The configuration was validated before this call."
```

Exceptions are the only way to waive a rule: selected lints take precedence
over `#[allow(...)]` in source. `path` is relative to the repository root and
may end in `*` for a prefix match. `item` optionally restricts the exception to
one item path within its module hierarchy. `reason` is required. Exceptions
never waive execution errors such as a missing tool or a failed compilation.

Notes:

- Syntax rules read all discovered sources, including code under inactive
  `cfg`. Macro input is not parsed as Rust and generated code is not expanded.
- Clippy runs on all workspace targets with the `[analysis]` settings. It does
  not try every feature combination.
- The project's Clippy configuration is preserved, except that
  `allow-unwrap-in-tests = true`, `check-private-items = true` and
  `avoid-breaking-exported-api = false` are set.
- `ref_option` requires Clippy 1.83 or later.
- With `analysis.cfg`, Cargo `rustflags` from configuration files are not
  merged implicitly; set `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` to include
  them.
- `options-default` compiles a temporary crate against a copy of the lockfile.
  The repository's manifests and lockfile are not modified. Selectors must
  name public, non-generic types.

## Library API

The checks are also available as a library.

```rust
use xtask_core::{Config, run};

let mut config = Config::new("path/to/repository");
config.no_write = true;
let report = run(&config)?;
for diagnostic in &report.style_diagnostics {
    println!("{diagnostic}");
}
```

`Config` can override the output directory, the role length limit and the
`[analysis]` settings. `analyze(&Config)` returns the per-target module graphs
without running style tools or writing output. Style violations are available as
structured `Report::style_diagnostics` and are also included in
`Report::errors`.

## Limitations

- Analysis is static, based on `syn`. Type inference and macro expansion are
  not reproduced. Unsupported syntax is reported as an error rather than
  ignored.
- Rust editions 2018, 2021 and 2024 are supported.

## Development

```sh
cargo test --locked --offline --workspace
cargo fmt --all -- --check
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
node --test crates/xtask-core/src/arch_map/viewer.test.cjs
cargo xtask check
```

To time a run against another repository, use the `benchmark` example. Pass an
absolute OUTPUT to leave that repository's diagrams untouched.

```sh
cargo run --release -p xtask-core --example benchmark -- ROOT OUTPUT MODE
```

`MODE` is `analyze`, `no-write` or `write`.

Workspace crates:

| Crate | Responsibility |
| --- | --- |
| `xtask-core` | CLI, checks, style rules and output |
| `xtask-workspace` | Cargo metadata and policy |
| `xtask-source` | source discovery and line counting |
| `xtask-modules` | module and reference analysis |
| `xtask-graph` | graph construction and SVG writing |
| `xtask-layout` | ELK layout through Node.js |
| `xtask-svg` | SVG rendering |

## License

Copyright (c) 2026 Kashiwade.

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

`crates/xtask-layout/vendor/elkjs` is distributed under its own license; see
the files in that directory.
