//! Role: 視点の計算（全体に合わせる、拡大・縮小、移動、中央へ寄せる）。純粋な関数。
use xtask_render::{Picture, R};

use crate::state::{Camera, View};

/// 全体に合わせるときの拡大率の上限と、canvas の縁の余白の割合。
const FIT_MARGIN: f64 = 0.95;
/// 拡大・縮小の範囲。
const MIN_SCALE: f64 = 0.05;
const MAX_SCALE: f64 = 8.0;
/// wheel の一目盛りあたりの拡大率の変化（指数）。
const WHEEL_RATE: f64 = 0.0015;

/// 今の視点の実際の値。Fit は canvas の大きさに合わせて解く。
pub fn view(camera: Camera, picture: &Picture, size: (f64, f64)) -> View {
    match camera {
        Camera::Manual {
            offset_x,
            offset_y,
            scale,
        } => View {
            offset_x,
            offset_y,
            scale,
        },
        Camera::Fit => {
            let fit = (size.0 / picture.width).min(size.1 / picture.height) * FIT_MARGIN;
            let scale = fit.min(1.0);
            View {
                offset_x: (size.0 - picture.width * scale) / 2.0,
                offset_y: (size.1 - picture.height * scale) / 2.0,
                scale,
            }
        }
    }
}

/// anchor（canvas の座標）の下の Picture の点を動かさずに拡大・縮小する。
pub fn zoom(view: View, anchor: (f64, f64), delta_y: f64) -> Camera {
    let scale = (view.scale * (-delta_y * WHEEL_RATE).exp()).clamp(MIN_SCALE, MAX_SCALE);
    let ratio = scale / view.scale;
    Camera::Manual {
        offset_x: anchor.0 - (anchor.0 - view.offset_x) * ratio,
        offset_y: anchor.1 - (anchor.1 - view.offset_y) * ratio,
        scale,
    }
}

/// 押した点から pointer が動いた分だけ視点を動かす。
pub fn pan(view: View, start: (f64, f64), pos: (f64, f64)) -> Camera {
    Camera::Manual {
        offset_x: view.offset_x + (pos.0 - start.0),
        offset_y: view.offset_y + (pos.1 - start.1),
        scale: view.scale,
    }
}

/// rect の中心が canvas の中央に来る視点。
pub fn center(rect: R, scale: f64, size: (f64, f64)) -> Camera {
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    Camera::Manual {
        offset_x: size.0 / 2.0 - cx * scale,
        offset_y: size.1 / 2.0 - cy * scale,
        scale,
    }
}

#[cfg(test)]
mod tests {
    //! Role: カメラ（表示の範囲・拡大・移動）の試験。
    use super::*;
    use xtask_render::Text;

    fn picture(width: f64, height: f64) -> Picture {
        Picture {
            width,
            height,
            title: Text {
                center_x: 0.0,
                baseline: 0.0,
                size: 0.0,
                text: String::new(),
            },
            regions: Vec::new(),
            edges: Vec::new(),
            bridges: Vec::new(),
            nodes: Vec::new(),
        }
    }

    fn manual(camera: Camera) -> (f64, f64, f64) {
        match camera {
            Camera::Manual {
                offset_x,
                offset_y,
                scale,
            } => (offset_x, offset_y, scale),
            Camera::Fit => panic!("expected a manual camera"),
        }
    }

    fn near(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn fit_centers_picture() {
        let fitted = view(Camera::Fit, &picture(200.0, 100.0), (400.0, 400.0));
        near(fitted.scale, 1.0);
        near(fitted.offset_x, 100.0);
        near(fitted.offset_y, 150.0);
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let start = View {
            offset_x: 10.0,
            offset_y: 20.0,
            scale: 1.0,
        };
        let (x, y, scale) = manual(zoom(start, (110.0, 120.0), -100.0));
        // The Picture point under the anchor, (100, 100), stays under the anchor.
        near(x + 100.0 * scale, 110.0);
        near(y + 100.0 * scale, 120.0);
    }

    #[test]
    fn pan_moves_by_pointer_delta() {
        let start = View {
            offset_x: 5.0,
            offset_y: 7.0,
            scale: 2.0,
        };
        let (x, y, scale) = manual(pan(start, (0.0, 0.0), (30.0, -10.0)));
        near(x, 35.0);
        near(y, -3.0);
        near(scale, 2.0);
    }

    #[test]
    fn center_puts_rect_in_middle() {
        let rect = R {
            x: 16.0,
            y: 42.0,
            width: 10.0,
            height: 10.0,
        };
        let (x, y, scale) = manual(center(rect, 2.0, (400.0, 300.0)));
        near(scale, 2.0);
        // The center of the rect, (21, 47), lands in the middle of the canvas.
        near(x + 21.0 * scale, 200.0);
        near(y + 47.0 * scale, 150.0);
    }
}
