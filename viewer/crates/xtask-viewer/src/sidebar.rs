//! Role: 木（crate・target・module・要素）の行の組立て、絞込み、開く操作の決定、sidebar の画面。
use kivact::prelude::{IntoView, View};

use crate::state::{AppState, Loaded};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TreeKey {
    Package(u32),
    Diagram(u32),
    Item { diagram: u32, item: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: TreeKey,
    pub parent: Option<TreeKey>,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open(usize),
    Reveal(usize, u32),
}

pub fn sidebar(state: AppState) -> impl IntoView {
    fn unbuilt() -> View {
        todo!("V2c")
    }
    let _ = state;
    unbuilt()
}

pub fn rows(loaded: &Loaded, search: &str) -> Vec<Row> {
    let _ = (loaded, search);
    todo!("V2c")
}

pub fn action(loaded: &Loaded, key: TreeKey) -> Option<Action> {
    let _ = (loaded, key);
    todo!("V2c")
}
