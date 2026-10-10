//! Role: workspace情報とリポジトリ固有の可視性ポリシーを公開する。
//! Cargo workspace declarations and the shared visibility policy.
//!
//! Edges include every declared normal, development, build, and target-specific
//! dependency on a workspace member, including optional dependencies. Identity
//! comes from canonical manifest paths rather than a dependency's alias or name.

mod metadata;
mod model;
mod policy;

#[cfg(test)]
mod tests;

pub use policy::{Limits, Policy};

pub use model::{Dependency, Package, Target, Workspace};
