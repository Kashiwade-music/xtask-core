//! Role: 有効なcfgに基づくRustモジュール解析の公開APIを提供する。
//! Source-level Rust module analysis with explicit active cfg configuration.
//! Name and absolute-path resolution assumes Rust edition 2018 or later.
//!
//! This is not rustc name resolution: procedural macros cannot be expanded.
//! Potential module-generating item macros and include! fail explicitly.
//! Unresolved external names produce no inferred dependency.
mod ast;
mod cfg;
mod components;
mod intrinsic;
mod json;
mod loader;
mod model;
mod resolve;
mod role;
mod symbols;
mod thread_local_items;
mod visitor;

use model::Env;
pub use model::{Configuration, Module, ModuleGraph, ModuleKind, Reference, Settings};
use std::path::Path;

/// Analyzes one crate root under each configuration. The root and every module file
/// are parsed once for all configurations; results are returned in the same order.
pub fn analyze(
    root_file: &Path,
    settings: &Settings,
    configurations: &[Configuration],
) -> Vec<Result<ModuleGraph, String>> {
    if let Err(error) = role::validate_limit(root_file, settings.max_role_chars) {
        return configurations.iter().map(|_| Err(error.clone())).collect();
    }
    let mut cache = loader::ParseCache::new();
    configurations
        .iter()
        .map(|configuration| {
            let env = Env {
                settings,
                cfg: &configuration.cfg,
            };
            let loaded = loader::load(root_file, env, &mut cache)?;
            resolve::analyze(loaded, env)
        })
        .collect()
}
