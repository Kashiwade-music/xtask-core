//! Role: 図の描画（静的な図、強調、画面と書出しで共通の描き方）。
use kivact::draw::Draw;
use xtask_render::{Highlight, Picture};

/// 画面の描画か、書出しの描画か。書出しは region の当たり判定を描かない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Export,
}

pub fn picture(draw: &mut Draw, picture: &Picture, mode: Mode) {
    let _ = (draw, picture, mode);
    todo!("V2b")
}

pub fn highlight(draw: &mut Draw, picture: &Picture, highlight: &Highlight) {
    let _ = (draw, picture, highlight);
    todo!("V2b")
}
