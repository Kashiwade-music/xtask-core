# xtask-core

[日本語](README_JP.md)

Repository rules and architecture diagrams for Rust workspaces, run through a
`cargo xtask` entry point.

`cargo xtask check` runs every repository rule and writes nothing:

- enforces a per-file code line limit,
- restricts which packages may depend on which (`visible_to`),
- requires a one-line `Role:` declaration on every crate root and module,
- rejects reference cycles between modules,
- optionally applies a mechanically checkable subset of the
  [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html).

`cargo xtask check --write` also lays out the crate and module dependency graphs
and writes them to one compressed file, which the `xtask-viewer` GUI opens. See
[Output](#output).

## Requirements

- Rust 1.85 or later. Node.js is not needed.
- A `Cargo.lock` and a populated dependency cache. Cargo is invoked with
  `--locked --offline`.
- `rustfmt` and `clippy` components, if style checks are enabled.
- To build the viewer: Rust 1.98.1 (selected by `viewer/rust-toolchain.toml`)
  and network access to fetch Kivact.

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
cargo xtask check              # run every check; writes nothing
cargo xtask check --write      # run every check, then write the diagram file
cargo xtask check --root PATH  # check another repository
cargo xtask check --help
```

All violations are collected and reported together. The command exits with 0 on
success, 1 on violations or execution errors, and 2 on invalid arguments. Any
other option, including the removed `--no-write` and `--prune`, is an invalid
argument.

`--write` writes every diagram that can be built, even when checks fail. A
target whose analysis failed records the reason in the file in place of its
diagram. If the file cannot be written, the error is reported with the other
violations.

When style checks are enabled, rustfmt and Clippy run in every mode. They
produce Cargo build output under `target/`, and they execute build scripts and
procedural macros.

Both the normal and the `cfg(test)` configuration of each target are analyzed.

## Configuration

The policy is read from `dev/xtask/policy/workspace.toml`. If the file does not
exist, every section uses its default. Unknown keys are rejected, and every
section must be a TOML table, such as `[limits]`.

```toml
[limits]
max_code_lines = 1000     # per file; at most 1000
max_role_chars = 80       # Role description length in Unicode characters

[visible_to]
internal = ["facade", "app-*"]
app = []

[diagrams]
targets = "all"          # "all", "lib-bin" or "lib"
output = "target/xtask/architecture.xtaskmap"   # default; relative to the root, or absolute

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
and patterns must match workspace members. Normal, dev, build, optional and
target-specific dependencies are all checked.

### Role declarations

Every crate root (library, binary, test, bench, example and build script) and
every module has a one-line inner documentation comment whose first line is
`Role:`:

```rust
//! Role: Coordinate the application.

mod config {
    //! Role: Hold runtime settings.
}
```

The `Role:` line must be the first line of the first inner documentation
comment of its module. A `Role:` line anywhere else, such as the second line of
the first inner documentation comment, is an error. For a module in its own
file, the declaration goes at the top of that file; an outer comment on
`mod name;` does not count. `/*! Role: ... */` and `#![doc = "Role: ..."]` have
the same meaning. The description must be nonempty and no longer than
`max_role_chars`.

### Diagrams

`[diagrams] targets` selects the targets whose module diagrams are written:

| Value | Targets |
| --- | --- |
| `"all"` (default) | every target |
| `"lib-bin"` | library kinds and binaries |
| `"lib"` | `lib`, `rlib`, `staticlib`, `cdylib`, `dylib`, `proc-macro` |

Unselected targets are still checked; only their diagrams are omitted.

`[diagrams] output` is the path of the diagram file. It is relative to the
repository root, or absolute. The default is `target/xtask/architecture.xtaskmap`.

### Analysis

By default, analysis uses the host target, Cargo's default features and no extra
`cfg`. `[analysis]` changes these settings for both source analysis and Clippy.

- `features` lists the features to enable. Each entry is nonempty, unique and
  has no surrounding spaces. `default_features` (default `true`) controls
  Cargo's default features.
- `all_features = true` analyzes with every feature. It cannot be combined with
  `features`, and `default_features` must not be written with it. Otherwise the
  policy is rejected with `all_features cannot be combined with features or
  default_features`.
- `cfg(true)` is evaluated as enabled.

`trusted_attributes`, `trusted_derives` and `trusted_macros` name items that are
assumed not to affect module structure. A call to a trusted function-like macro
is matched by the exact path written at the call site, such as
`serde_json::json`. Such a call is not expanded, so the references inside it
are not analyzed and do not become dependency edges.

`json!` and `Token!` are not special-cased. A repository that uses them lists
the paths it writes in `trusted_macros`, such as `serde_json::json` or
`syn::Token` for `Token![...]`.

A glob import from another crate could provide any macro, derive or attribute,
so the analyzer cannot tell where `vec!` or `#[derive(Clone)]` comes from in a
module that has one. `trusted_globs` names such an import by its path, such as
`framework::prelude`, so that the analyzer does not treat it as an unknown
provider. If the glob provides an attribute or macro that the code uses, that
name must also be listed in `trusted_attributes` or `trusted_macros`.

`trusted_globs` and `allowed_globs` in `[style]` are separate settings.
`allowed_globs` only lets the `glob-imports` style rule accept the glob; it does
not change the analysis. A glob that needs both must be listed in both.

## Output

### Diagram file

`--write` writes one file to `[diagrams] output`. Each `--write` replaces that
file as a whole; no other file is written or deleted. The file is a
zlib-compressed binary (format version 1) that holds every diagram of the run
and the run's notices. It is under `target/`, which is a build output directory
normally excluded from version control.

`check` without `--write` leaves the file alone.

### Viewer

`xtask-viewer` is a GUI that opens the diagram file. It is built with Kivact
from the [`viewer/`](viewer) workspace.

- Open a file by passing its path as an argument, with the Open dialog, or by
  dropping the file on the window. The viewer opens the crate graph.
- The viewer checks the file every 500 ms and reloads it when it changes. F5
  reloads it at once.
- Hover an item to highlight what it depends on and what depends on it. Click
  or press Space to pin the highlight; press Esc or click the background to clear
  it.
- Double-click a crate or module, or press Enter, to open its module diagram.
  Back (Backspace) returns to the previous diagram, and Fit (0) fits the view.
- Drag to pan and use the wheel to zoom around the pointer.
- The tree on the left lists crates, targets and modules, and the search box
  narrows it. Press Enter or double-click an item to open it; a single click only
  selects it.
- The current diagram can be exported as SVG.

Build (Windows):

```sh
cd viewer
cargo build --release
```

The executable is `viewer/target/release/xtask-viewer.exe`. The build needs
Rust 1.98.1 (selected by `viewer/rust-toolchain.toml`) and network access to
fetch Kivact at its pinned revision. macOS is not supported yet, because Kivact
does not support it.

A push to `main` runs `.github/workflows/viewer-windows.yml`. It builds the
viewer in release mode on Windows and keeps `xtask-viewer.exe` as the artifact
`xtask-viewer-windows`.

## Migration

To update a repository that uses the earlier version:

- Node.js and `XTASK_NODE` are no longer used. Remove any Node.js or elkjs setup
  from CI.
- `--no-write` is now the default, and the flag is removed. Remove it from
  scripts. `--write` writes the diagram file.
- `--prune` is removed. Each `--write` replaces the diagram file, so no stale
  diagrams remain. Delete an old `architecture/` directory by hand, together with
  its SVG and HTML files; the tool no longer removes them.
- The top-level policy key `output` moved to `[diagrams] output`. A policy that
  still has `output` is rejected, and the error names the new key.
- Role declarations must be on the first line of the first inner documentation
  comment of their module.
- An exception `path` of `*` alone is rejected. Use a directory prefix such as
  `src/*`.
- A repository that uses `json!` or `Token!` lists them in `trusted_macros`.
- Library users:
  - `Config::no_write` is removed. To write the diagram file, set
    `config.output = Output::Policy`.
  - `Config::max_role_chars` is removed. The Role limit is `[limits] max_role_chars`.
  - `TargetGraph` is replaced by `TargetAnalysis`, and its graph is
    `TargetOutcome::Analyzed(graph)`.
  - `Report::artifacts` is replaced by `Report::archive`, and
    `Report::style_diagnostics` by `Report::style`.
  - `Report::errors` no longer contains style violations. Read `Report::style` as
    well.
- Add this to the root `Cargo.toml` of the repository, so that the layout runs
  fast in debug builds:

  ```toml
  [profile.dev.package.xtask-layout]
  opt-level = 3
  ```

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
may end in `*` for a prefix match, such as `src/*`; a `path` of `*` alone is an
error. `item` optionally restricts the exception to one item path within its
module hierarchy. `reason` is required. Exceptions never waive execution errors
such as a missing tool or a failed compilation.

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
  The repository's manifests and lockfile are not modified. Selectors must name
  public, non-generic types.

## Library API

The checks are also available as a library.

```rust
use xtask_core::{Config, run};

let config = Config::new("path/to/repository"); // writes nothing by default
let report = run(&config)?;
for diagnostic in &report.style {
    println!("{diagnostic}");
}
```

- `Config` has the fields `root`, `policy`, `output` and `analysis`. `output` is
  `Output::None` (the default, which writes nothing), `Output::Policy` (the
  `[diagrams] output` path) or `Output::Path(PathBuf)` (a path, relative to the
  root or absolute). When `analysis` is `Some`, it replaces the policy's
  `[analysis]` settings.
- `run` returns an error only when the policy or the Cargo metadata cannot be
  read. Other failures are recorded in `Report::errors`.
- `Report` has the fields `errors`, `notes`, `style` and `archive`. `style` holds
  the structured style violations, which are not repeated in `errors`.
- `analyze(&config)` returns one `TargetAnalysis` per target, with an outcome for
  each configuration (`Mode::Normal` or `Mode::Test`): `Analyzed(ModuleGraph)`,
  `FeatureGated`, `UnsupportedEdition` or `Failed`. It writes nothing and does
  not run the style checks.

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
cargo xtask check
cargo xtask check --write
```

The viewer is a separate workspace (`exclude = ["viewer"]` in the root
`Cargo.toml`), so run its checks from `viewer/`:

```sh
cd viewer
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

To time a run against another repository, use the `benchmark` example. It times
`check` and `check --write` for ROOT. The `--write` output goes to ROOT's
`target/`.

```sh
cargo run --release -p xtask-core --example benchmark -- ROOT
```

Workspace crates:

| Crate | Responsibility |
| --- | --- |
| `xtask-core` | CLI, the line and visibility checks, and the diagram file |
| `xtask-source` | source discovery, path normalization and line counting |
| `xtask-policy` | policy reading and validation |
| `xtask-workspace` | Cargo metadata (structure and features) |
| `xtask-modules` | module and reference analysis |
| `xtask-style` | Google Rust Style Guide checks |
| `xtask-layout` | layout engine (Rust only) |
| `xtask-archive` | the diagram file: types, validation, encoding and compression |
| `xtask-diagram` | builds diagrams from the analysis and lays them out |

Viewer crates (`viewer/`):

| Crate | Responsibility |
| --- | --- |
| `xtask-render` | geometry of the drawing (corners, crossings, arrows, text positions); independent of Kivact |
| `xtask-viewer` | the Kivact GUI, which draws what `xtask-render` computes |

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
