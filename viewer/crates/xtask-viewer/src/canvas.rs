//! Role: 図の canvas（重ねた二つの canvas、region の宣言、pointer・wheel・tooltip）。
use std::rc::Rc;

use kivact::custom::Part;
use kivact::draw::{Draw, Layer, RegionId, Transform};
use kivact::prelude::*;
use kivact::style::Position;
use kivact::view::{ButtonPress, MouseButton, PointerPhase, WheelEvent, overlay_at};
use xtask_archive::{Content, DiagramKind, Item, Scene};
use xtask_render::{Picture, neighbors};

use crate::camera;
use crate::navigate;
use crate::paint::{self, Mode};
use crate::state::{AppState, Document, Gesture, Loaded, View};

/// この距離（px）以上動いたら、押した後の動きをドラッグとみなす。
const DRAG_SLOP: f64 = 3.0;

/// 図の canvas。静的な図（base）と強調（overlay）を重ね、region・pointer・wheel・tooltip を結ぶ。
pub fn diagram_view(state: AppState) -> impl IntoView {
    let diagram = memo(move || state.nav.with(|nav| nav.diagram.clone()));
    let entries = memo(move || {
        Shown::current(state)
            .map(|shown| shown.entries())
            .unwrap_or_default()
    });
    let base = canvas(move |draw, size| base_draw(state, diagram, draw, size))
        .style(style().grow(1).min_height(0))
        .name("Diagram")
        .regions(
            entries,
            |entry: &RegionEntry| RegionId(entry.id),
            move |_, entry| region_part(state, entry),
        )
        .region_events()
        .on_pointer(move |event: PointerEvent| pointer(state, &event))
        .on_wheel(move |event: WheelEvent| wheel(state, &event));
    let overlay = canvas(move |draw, size| overlay_draw(state, draw, size))
        .style(style().position(Position::Absolute).inset(0))
        .hit_testable(false);
    let tip = memo(move || tooltip(state));
    column((
        base,
        overlay,
        show_some(tip, move |line| {
            overlay_at(state.pointer, Placement::Below, text(move || line.get()))
                .hit_testable(false)
        }),
    ))
    .style(style().grow(1).min_height(0).position(Position::Relative))
}

/// 表示中の図の region の一つ。`id` は `RegionId` と同じ値。
#[derive(Clone, PartialEq)]
struct RegionEntry {
    id: u64,
    item: u32,
    name: String,
    open: Option<u32>,
}

/// 表示中の図（文書が Loaded のとき）。読んだ時点の snapshot を持つ。
struct Shown {
    loaded: Rc<Loaded>,
    index: usize,
}

impl Shown {
    fn current(state: AppState) -> Option<Self> {
        let (loaded, index) = state.current()?;
        Some(Self { loaded, index })
    }

    fn picture(&self) -> Option<&Picture> {
        self.loaded.pictures.get(self.index)?.as_ref()
    }

    fn scene(&self) -> Option<&Scene> {
        match &self.loaded.archive.diagrams.get(self.index)?.content {
            Content::Drawn(scene) => Some(scene),
            _ => None,
        }
    }

    fn item_at(&self, id: RegionId) -> Option<u32> {
        region_item(self.picture()?, id)
    }

    fn item_info(&self, item: u32) -> Option<&Item> {
        self.scene()?.items.get(item as usize)
    }

    /// 今の図の region ごとの一覧（region が無い図では node ごと）。
    fn entries(&self) -> Vec<RegionEntry> {
        let Some(picture) = self.picture() else {
            return Vec::new();
        };
        (0..region_count(picture))
            .filter_map(|index| {
                let id = RegionId(index as u64);
                let item = region_item(picture, id)?;
                let info = self.item_info(item);
                Some(RegionEntry {
                    id: index as u64,
                    item,
                    name: info.map(|info| info.name.clone()).unwrap_or_default(),
                    open: info.and_then(|info| info.open),
                })
            })
            .collect()
    }
}

/// region の数。region が一つも無い図では node を region とみなす（paint と同じ規則）。
fn region_count(picture: &Picture) -> usize {
    if picture.regions.is_empty() {
        picture.nodes.len()
    } else {
        picture.regions.len()
    }
}

/// region の id が指す item（paint の描いた id と同じ規則）。
fn region_item(picture: &Picture, id: RegionId) -> Option<u32> {
    let index = usize::try_from(id.0).ok()?;
    if picture.regions.is_empty() {
        picture.nodes.get(index).map(|node| node.item)
    } else {
        picture.regions.get(index).map(|region| region.item)
    }
}

/// 静的な図。読むのは document、図の kind（memo 経由）、camera だけ。固定の変化では描き直さない。
fn base_draw(state: AppState, diagram: Value<DiagramKind>, draw: &mut Draw, size: Size) {
    state.viewport.set((size.width, size.height));
    let Document::Loaded { loaded, .. } = state.document.get() else {
        return;
    };
    let Some(index) = loaded.kinds.get(&diagram.get()).copied() else {
        return;
    };
    let Some(picture) = loaded.pictures.get(index).and_then(Option::as_ref) else {
        return;
    };
    let view = camera::view(state.camera.get(), picture, (size.width, size.height));
    draw.layer(camera_layer(view), |draw| {
        paint::picture(draw, picture, Mode::Screen);
    });
}

/// 強調の重ね描き。固定があればそれ、無ければ hover の item を強調する。
fn overlay_draw(state: AppState, draw: &mut Draw, size: Size) {
    let Some(shown) = Shown::current(state) else {
        return;
    };
    let (Some(picture), Some(scene)) = (shown.picture(), shown.scene()) else {
        return;
    };
    let Some(item) = state.nav.with(|nav| nav.pinned).or(state.hover.get()) else {
        return;
    };
    let view = camera::view(state.camera.get(), picture, (size.width, size.height));
    let highlight = neighbors(scene, item);
    draw.layer(camera_layer(view), |draw| {
        paint::highlight(draw, picture, &highlight);
    });
}

fn camera_layer(view: View) -> Layer {
    Layer::new().transform(
        Transform::new()
            .translate(view.offset_x, view.offset_y)
            .scale(view.scale),
    )
}

/// region の Part。名前と handler は `entry` を読む（同じ key の region が別の item に替わっても従う）。
fn region_part(state: AppState, entry: Value<RegionEntry>) -> Part<()> {
    Part::new(().into_value(), move |host| {
        host.role(Role::Button)
            .focusable(true)
            .name(move || entry.with(|e| e.name.clone()))
            .selected(move || state.nav.with(|nav| nav.pinned) == Some(entry.with(|e| e.item)))
            .on_key(move |event: KeyEvent| region_key(state, entry, &event))
    })
}

/// region の keyboard の操作。Enter は開き、Space は固定を切り替える。
fn region_key(state: AppState, entry: Value<RegionEntry>, event: &KeyEvent) -> Handled {
    let (item, open) = entry.with(|e| (e.item, e.open));
    match event.key() {
        Key::Enter => {
            if let Some(diagram) = open {
                navigate::go(state, diagram as usize);
            }
            Handled::Yes
        }
        Key::Space => {
            let nav = state.nav.get_untracked();
            state.nav.set(navigate::toggle_pin(&nav, item));
            Handled::Yes
        }
        _ => Handled::No,
    }
}

/// pointer の一事象を、押した後の動作（gesture）、hover、視点、固定に写す。
fn pointer(state: AppState, event: &PointerEvent) -> Handled {
    match event.phase() {
        PointerPhase::Down(press) => press_down(state, event, press),
        PointerPhase::Move => move_to(state, event),
        PointerPhase::Up(press) if press.button == MouseButton::Primary => release(state),
        PointerPhase::Up(_) => {}
        PointerPhase::Leave => state.hover.set(None),
        PointerPhase::Cancel => {
            state.gesture.set(Gesture::Idle);
            state.hover.set(None);
        }
        PointerPhase::Enter => {
            if state.gesture.get() == Gesture::Idle {
                move_to(state, event);
            }
        }
    }
    Handled::No
}

fn press_down(state: AppState, event: &PointerEvent, press: ButtonPress) {
    if press.button != MouseButton::Primary {
        return;
    }
    let Some(shown) = Shown::current(state) else {
        return;
    };
    let Some(picture) = shown.picture() else {
        return;
    };
    let item = hit_item(state, event);
    if press.clicks >= 2 {
        let open = item
            .and_then(|item| shown.item_info(item))
            .and_then(|info| info.open);
        if let Some(diagram) = open {
            navigate::go(state, diagram as usize);
            state.gesture.set(Gesture::Idle);
            return;
        }
    }
    let view = camera::view(state.camera.get_untracked(), picture, state.viewport.get());
    state.gesture.set(Gesture::Pressed {
        start: point(event.pos()),
        view,
        target: item,
    });
}

fn move_to(state: AppState, event: &PointerEvent) {
    let pos = point(event.pos());
    match state.gesture.get() {
        Gesture::Pressed { start, view, .. } if distance(start, pos) >= DRAG_SLOP => {
            state.gesture.set(Gesture::Dragging { start, view });
            state.camera.set(camera::pan(view, start, pos));
        }
        Gesture::Dragging { start, view } => state.camera.set(camera::pan(view, start, pos)),
        Gesture::Idle => {
            state.hover.set(hit_item(state, event));
            state.pointer.set(event.window_pos());
        }
        Gesture::Pressed { .. } => {}
    }
}

/// 押して離した時。押した item なら固定を切り替え、item の上でなければ固定を解く。
fn release(state: AppState) {
    let nav = state.nav.get_untracked();
    match state.gesture.get() {
        Gesture::Pressed {
            target: Some(item), ..
        } => state.nav.set(navigate::toggle_pin(&nav, item)),
        Gesture::Pressed { target: None, .. } => state.nav.set(navigate::unpin(&nav)),
        Gesture::Dragging { .. } | Gesture::Idle => {}
    }
    state.gesture.set(Gesture::Idle);
}

/// 押した後でなければ、wheel は pointer の位置を中心に拡大・縮小する。
fn wheel(state: AppState, event: &WheelEvent) -> Handled {
    if state.gesture.get() != Gesture::Idle {
        return Handled::Yes;
    }
    let Some(shown) = Shown::current(state) else {
        return Handled::Yes;
    };
    let Some(picture) = shown.picture() else {
        return Handled::Yes;
    };
    let view = camera::view(state.camera.get_untracked(), picture, state.viewport.get());
    let pos = event.pos();
    state
        .camera
        .set(camera::zoom(view, (pos.x, pos.y), event.delta().y));
    Handled::Yes
}

/// pointer が当たっている item（region が無ければ node）。
fn hit_item(state: AppState, event: &PointerEvent) -> Option<u32> {
    let shown = Shown::current(state)?;
    event.region().and_then(|id| shown.item_at(id))
}

/// hover の item の tooltip の文（名前と、あれば改行して Role）。
fn tooltip(state: AppState) -> Option<String> {
    let item = state.hover.get()?;
    let shown = Shown::current(state)?;
    let info = shown.item_info(item)?;
    Some(match &info.role {
        Some(role) => format!("{}\n{role}", info.name),
        None => info.name.clone(),
    })
}

fn point(p: Point) -> (f64, f64) {
    (p.x, p.y)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}
