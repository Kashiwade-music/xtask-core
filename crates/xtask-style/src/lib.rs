//! Role: スタイル検査の入力と報告の型を公開し、検査の本体を呼び出せるようにする。
//! Style checks over a prepared workspace: source syntax, rustfmt, Clippy, and
//! Default probes. The caller supplies normalized paths and analysis settings.

mod model;
mod style;
mod style_source;
mod style_tools;

pub use model::{StyleDiagnostic, StyleReport};
pub use style::{StyleInput, check};
