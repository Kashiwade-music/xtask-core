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
mod symbols;
mod visitor;

pub use model::{AnalysisOptions, Module, ModuleGraph, Reference};
use std::path::Path;

pub fn analyze(root_file: &Path, options: &AnalysisOptions) -> Result<ModuleGraph, String> {
    let sources = loader::load(root_file, options)?;
    resolve::analyze(sources, options)
}
