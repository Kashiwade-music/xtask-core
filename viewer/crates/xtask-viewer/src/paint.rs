//! Role: 図の描画（静的な図、強調、画面と書出しで共通の描き方）。
use kivact::draw::{Draw, LineCap, Path, RegionId, Stroke, TextBaseline, TextFormat};
use kivact::geometry::{Point, Rect};
use kivact::style::{Color, TextAlign, rgb};
use xtask_archive::style::FONT_FAMILY;
use xtask_render::palette;
use xtask_render::{Bridge, Cmd, Highlight, P, Picture, R, Rgb, Text};

/// 画面の描画か、書出しの描画か。書出しは region の当たり判定を描かない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Export,
}

/// 強調の矩形の輪郭の太さ。
const OUTLINE_WIDTH: f64 = 2.0;

/// 図全体を描く。描く順は、背景、題、region の枠、edge と矢印、跳び、node、region の帯、edge の label。
pub fn picture(draw: &mut Draw, picture: &Picture, mode: Mode) {
    draw.fill_rect(
        rect(R {
            x: 0.0,
            y: 0.0,
            width: picture.width,
            height: picture.height,
        }),
        color(palette::BACKGROUND),
    );
    texts(draw, std::slice::from_ref(&picture.title));
    regions_frames(draw, picture);
    edges(draw, picture);
    bridges(draw, picture);
    nodes(draw, picture);
    region_bands(draw, picture);
    edge_labels(draw, picture);
    if mode == Mode::Screen {
        hit_regions(draw, picture);
    }
}

/// 強調する item の edge・跳び・矩形を、強調の色で描き直す。
pub fn highlight(draw: &mut Draw, picture: &Picture, highlight: &Highlight) {
    for &index in &highlight.edges {
        let Some(edge) = picture.edges.get(index as usize) else {
            continue;
        };
        let stroke = edge_stroke(edge.dotted, palette::ACCENT_WIDTH);
        draw.stroke_path(&edge_path(&edge.path), &stroke, color(palette::ACCENT));
        for bridge in picture.bridges.iter().filter(|bridge| bridge.edge == index) {
            draw.stroke_path(&arc(bridge), &stroke, color(palette::ACCENT));
        }
    }
    let outline = Stroke::new(OUTLINE_WIDTH);
    for region in picture
        .regions
        .iter()
        .filter(|region| highlight.items.contains(&region.item))
    {
        let path = rounded(region.rect, palette::REGION_RADIUS);
        draw.stroke_path(&path, &outline, color(palette::ACCENT));
    }
    for node in picture
        .nodes
        .iter()
        .filter(|node| highlight.items.contains(&node.item))
    {
        let path = rounded(node.rect, palette::NODE_RADIUS);
        draw.stroke_path(&path, &outline, color(palette::ACCENT));
    }
}

fn regions_frames(draw: &mut Draw, picture: &Picture) {
    for region in &picture.regions {
        let frame = rounded(region.rect, palette::REGION_RADIUS);
        if let Some(fill) = region.fill {
            draw.fill_path(&frame, color(fill));
        }
        let stroke = Stroke::new(palette::REGION_STROKE_WIDTH);
        draw.stroke_path(&frame, &stroke, color(palette::REGION_STROKE));
    }
}

fn edges(draw: &mut Draw, picture: &Picture) {
    for edge in &picture.edges {
        let stroke = edge_stroke(edge.dotted, palette::EDGE_WIDTH);
        draw.stroke_path(&edge_path(&edge.path), &stroke, color(palette::STROKE));
        draw.fill_path(&triangle(edge.arrow), color(palette::STROKE));
    }
}

/// 交差の跳び。白い halo で下の線を隠し、その上に edge の線の形式で半円を描く。
fn bridges(draw: &mut Draw, picture: &Picture) {
    for bridge in &picture.bridges {
        let Some(edge) = picture.edges.get(bridge.edge as usize) else {
            continue;
        };
        let halo = Stroke {
            cap: LineCap::Round,
            ..Stroke::new(palette::EDGE_WIDTH + 3.0)
        };
        draw.stroke_path(&span(bridge), &halo, color(palette::BACKGROUND));
        draw.stroke_path(&arc(bridge), &halo, color(palette::BACKGROUND));
        let stroke = edge_stroke(edge.dotted, palette::EDGE_WIDTH);
        draw.stroke_path(&arc(bridge), &stroke, color(palette::STROKE));
    }
}

fn nodes(draw: &mut Draw, picture: &Picture) {
    for node in &picture.nodes {
        let frame = rounded(node.rect, palette::NODE_RADIUS);
        draw.fill_path(&frame, color(palette::NODE_FILL));
        let stroke = Stroke::new(palette::NODE_STROKE_WIDTH);
        draw.stroke_path(&frame, &stroke, color(palette::NODE_STROKE));
        texts(draw, &node.texts);
    }
}

fn region_bands(draw: &mut Draw, picture: &Picture) {
    for region in &picture.regions {
        draw.fill_rect(rect(region.band), color(region.band_fill));
        texts(draw, &region.texts);
    }
}

fn edge_labels(draw: &mut Draw, picture: &Picture) {
    for edge in &picture.edges {
        // The backdrop is already the label box grown by (-2, -1), as xtask-render makes it.
        let Some((backdrop, label_texts)) = &edge.label else {
            continue;
        };
        draw.fill_rect(rect(*backdrop), color(palette::BACKGROUND));
        texts(draw, label_texts);
    }
}

/// 画面の当たり判定。region があれば region、無ければ node を、index を id にして宣言する。
fn hit_regions(draw: &mut Draw, picture: &Picture) {
    if picture.regions.is_empty() {
        for (index, node) in picture.nodes.iter().enumerate() {
            draw.region(RegionId(index as u64), rect(node.rect));
        }
    } else {
        for (index, region) in picture.regions.iter().enumerate() {
            draw.region(RegionId(index as u64), rect(region.rect));
        }
    }
}

fn texts(draw: &mut Draw, texts: &[Text]) {
    for text in texts {
        let format = TextFormat {
            size: text.size,
            family: Some(FONT_FAMILY.into()),
            align: TextAlign::Center,
            baseline: TextBaseline::Alphabetic,
            color: color(palette::TEXT),
            ..TextFormat::default()
        };
        let at = Point {
            x: text.center_x,
            y: text.baseline,
        };
        draw.text(&text.text, at, format);
    }
}

fn edge_stroke(dotted: bool, width: f64) -> Stroke {
    if dotted {
        Stroke {
            cap: LineCap::Round,
            dash: vec![0.0, 3.5],
            ..Stroke::new(width)
        }
    } else {
        Stroke::new(width)
    }
}

fn edge_path(commands: &[Cmd]) -> Path {
    commands
        .iter()
        .fold(Path::new(), |path, command| match *command {
            Cmd::Move(to) => path.move_to(point(to)),
            Cmd::Line(to) => path.line_to(point(to)),
            Cmd::Quad { control, to } => path.quad_to(point(control), point(to)),
        })
}

fn triangle(points: [P; 3]) -> Path {
    Path::new()
        .move_to(point(points[0]))
        .line_to(point(points[1]))
        .line_to(point(points[2]))
        .close()
}

/// 跳びの両端（中心の左右 radius）。
fn bridge_ends(bridge: &Bridge) -> (Point, Point) {
    let P { x, y } = bridge.center;
    (
        Point {
            x: x - bridge.radius,
            y,
        },
        Point {
            x: x + bridge.radius,
            y,
        },
    )
}

/// 跳びの水平の範囲。
fn span(bridge: &Bridge) -> Path {
    let (left, right) = bridge_ends(bridge);
    Path::new().move_to(left).line_to(right)
}

/// 跳びの半円（上へ膨らむ）。
fn arc(bridge: &Bridge) -> Path {
    let (left, right) = bridge_ends(bridge);
    Path::new()
        .move_to(left)
        .svg_arc_to((bridge.radius, bridge.radius), 0.0, false, true, right)
}

/// 角を半径 `radius` で丸めた矩形（角は二次の曲線で近似する）。
fn rounded(rect: R, radius: f64) -> Path {
    let r = radius.min(rect.width / 2.0).min(rect.height / 2.0);
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
    Path::new()
        .move_to(point(P { x: x0 + r, y: y0 }))
        .line_to(point(P { x: x1 - r, y: y0 }))
        .quad_to(point(P { x: x1, y: y0 }), point(P { x: x1, y: y0 + r }))
        .line_to(point(P { x: x1, y: y1 - r }))
        .quad_to(point(P { x: x1, y: y1 }), point(P { x: x1 - r, y: y1 }))
        .line_to(point(P { x: x0 + r, y: y1 }))
        .quad_to(point(P { x: x0, y: y1 }), point(P { x: x0, y: y1 - r }))
        .line_to(point(P { x: x0, y: y0 + r }))
        .quad_to(point(P { x: x0, y: y0 }), point(P { x: x0 + r, y: y0 }))
        .close()
}

fn point(p: P) -> Point {
    Point { x: p.x, y: p.y }
}

fn rect(r: R) -> Rect {
    Rect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn color(c: Rgb) -> Color {
    rgb((u32::from(c.0) << 16) | (u32::from(c.1) << 8) | u32::from(c.2))
}
