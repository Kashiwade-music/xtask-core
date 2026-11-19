//! Role: viewer の画面の組立て（root、view、読込みの開始と監視、toolbar、通知）。
use std::path::PathBuf;

use kivact::prelude::{IntoView, View};

use crate::state::AppState;

// `todo!()` の型は `!` で、`impl IntoView` の戻り値にはできないので、`View` を返す内側の関数に置く。
pub fn root(initial: Option<PathBuf>) -> impl IntoView {
    fn unbuilt() -> View {
        todo!("V2a")
    }
    let _ = initial;
    unbuilt()
}

pub fn view(state: AppState, initial: Option<PathBuf>) -> impl IntoView {
    fn unbuilt() -> View {
        todo!("V2a")
    }
    let _ = (state, initial);
    unbuilt()
}
