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
mod visitor;

pub use model::{AnalysisOptions, DEFAULT_MAX_ROLE_CHARS, Module, ModuleGraph, Reference};
use std::path::Path;

pub fn analyze(root_file: &Path, options: &AnalysisOptions) -> Result<ModuleGraph, String> {
    role::validate_limit(root_file, options.max_role_chars)?;
    let sources = loader::load(root_file, options)?;
    resolve::analyze(sources, options)
}

/// Read the crate root's static inner Role doc without loading child modules.
pub fn crate_role(root_file: &Path) -> Result<String, String> {
    crate_role_with_limit(root_file, DEFAULT_MAX_ROLE_CHARS)
}

/// Read the root Role using a positive, configurable Unicode character limit.
pub fn crate_role_with_limit(root_file: &Path, max_role_chars: usize) -> Result<String, String> {
    role::root(root_file, max_role_chars)
}
