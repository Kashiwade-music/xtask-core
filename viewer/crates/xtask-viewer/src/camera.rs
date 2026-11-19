//! Role: 視点の計算（全体に合わせる、拡大・縮小、移動、中央へ寄せる）。純粋な関数。
use xtask_render::{Picture, R};

use crate::state::{Camera, View};

pub fn view(camera: Camera, picture: &Picture, size: (f64, f64)) -> View {
    let _ = (camera, picture, size);
    todo!("V2b")
}

pub fn zoom(view: View, anchor: (f64, f64), delta_y: f64) -> Camera {
    let _ = (view, anchor, delta_y);
    todo!("V2b")
}

pub fn pan(view: View, start: (f64, f64), pos: (f64, f64)) -> Camera {
    let _ = (view, start, pos);
    todo!("V2b")
}

pub fn center(rect: R, scale: f64, size: (f64, f64)) -> Camera {
    let _ = (rect, scale, size);
    todo!("V2b")
}
