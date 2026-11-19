//! Role: 図の移動と固定の遷移（履歴、固定、視点の変更）。
use crate::state::{AppState, Loaded, Nav};

pub fn open(nav: &Nav, loaded: &Loaded, diagram: usize) -> Nav {
    let _ = (nav, loaded, diagram);
    todo!("V2b")
}

pub fn back(nav: &Nav) -> Nav {
    let _ = nav;
    todo!("V2b")
}

pub fn toggle_pin(nav: &Nav, item: u32) -> Nav {
    let _ = (nav, item);
    todo!("V2b")
}

pub fn unpin(nav: &Nav) -> Nav {
    let _ = nav;
    todo!("V2b")
}

pub fn go(state: AppState, diagram: usize) {
    let _ = (state, diagram);
    todo!("V2b")
}

pub fn go_back(state: AppState) {
    let _ = state;
    todo!("V2b")
}

pub fn fit(state: AppState) {
    let _ = state;
    todo!("V2b")
}

pub fn reveal(state: AppState, diagram: usize, item: u32) {
    let _ = (state, diagram, item);
    todo!("V2b")
}
