//! Role: edge の折れ線を角を丸めた path にし、矢印の三角形を作る。
use crate::model::{Cmd, P};

/// 角の丸めの半径の上限。
const CORNER_RADIUS: f64 = 4.0;
/// 矢印の長さの上限。
const ARROW_LENGTH: f64 = 8.0;

/// 折れ線の各角を、半径 min(4, 前の辺/2, 次の辺/2) の二次曲線で丸めた path。
pub(crate) fn path(points: &[P]) -> Vec<Cmd> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut commands = vec![Cmd::Move(*first)];
    for window in points.windows(3) {
        let (before, corner, after) = (window[0], window[1], window[2]);
        let radius = CORNER_RADIUS
            .min(distance(before, corner) / 2.0)
            .min(distance(corner, after) / 2.0);
        commands.push(Cmd::Line(towards(corner, before, radius)));
        if radius > 0.0 {
            commands.push(Cmd::Quad {
                control: corner,
                to: towards(corner, after, radius),
            });
        }
    }
    if let (true, Some(last)) = (points.len() >= 2, points.last()) {
        commands.push(Cmd::Line(*last));
    }
    commands
}

/// 先端（最後の点）と、その手前の基部の二点。先端は矢印の [0]。最後の線分は右向きとする。
pub(crate) fn arrow(points: &[P]) -> [P; 3] {
    let Some(&tip) = points.last() else {
        let origin = P { x: 0.0, y: 0.0 };
        return [origin; 3];
    };
    let previous = points
        .iter()
        .rev()
        .find(|point| **point != tip)
        .copied()
        .unwrap_or(tip);
    let length = distance(previous, tip).min(ARROW_LENGTH);
    let half_width = length * 0.4;
    [
        tip,
        P {
            x: tip.x - length,
            y: tip.y - half_width,
        },
        P {
            x: tip.x - length,
            y: tip.y + half_width,
        },
    ]
}

fn distance(from: P, to: P) -> f64 {
    (to.x - from.x).hypot(to.y - from.y)
}

/// `from` から `to` の向きへ `along` だけ進んだ点。`from` と `to` が同じなら `from`。
fn towards(from: P, to: P, along: f64) -> P {
    let length = distance(from, to);
    if length == 0.0 {
        return from;
    }
    P {
        x: from.x + (to.x - from.x) * along / length,
        y: from.y + (to.y - from.y) * along / length,
    }
}
