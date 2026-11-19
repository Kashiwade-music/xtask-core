//! Role: 図のファイルの読込み（純粋な読込み、結果を状態に写す規則、要求の受理、開く・監視）。
use std::path::{Path, PathBuf};

use crate::state::{AppState, Document, Loaded, Nav, Reason, Request, Stamp};

/// 読込みの結果。`blocking` の戻り値として UI の thread の外から返すので Send。
pub struct LoadResult {
    pub path: PathBuf,
    pub stamp: Option<Stamp>,
    pub loaded: Result<Loaded, String>,
}

/// 読込みの結果を写した後の状態（`apply` の戻り値）。
pub struct Applied {
    pub document: Document,
    pub nav: Nav,
    pub fit: bool,
    pub note: Option<String>,
}

pub fn load(path: &Path) -> LoadResult {
    let _ = path;
    todo!("V2a")
}

pub fn apply(previous: &Document, nav: &Nav, reason: Reason, result: LoadResult) -> Applied {
    let _ = (previous, nav, reason, result);
    todo!("V2a")
}

pub fn accepts(request: &Request, id: u64) -> bool {
    let _ = (request, id);
    todo!("V2a")
}

pub fn open(state: AppState, path: PathBuf, reason: Reason) {
    let _ = (state, path, reason);
    todo!("V2a")
}

pub fn watch(state: AppState) {
    let _ = state;
    todo!("V2a")
}
