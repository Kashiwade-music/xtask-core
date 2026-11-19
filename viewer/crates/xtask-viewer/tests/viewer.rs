//! Role: viewer の GUI の試験。文書を開く結線と、実際の大きさ・gesture が成り立つことを確かめる。
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use kivact::draw::Offscreen;
use kivact::prelude::*;
use kivact::test::Screen;
use kivact::view::MouseButton;
use xtask_archive::{
    Archive, Content, Diagram, DiagramKind, Edge, EdgeStyle, Item, Mode, Node, Point, Rect, Region,
    Scene,
};
use xtask_viewer::app;
use xtask_viewer::paint;
use xtask_viewer::state::{AppState, Camera};

const CRATE_TITLE: &str = "Workspace crate dependencies (consumer -> provider)";
const RELOADED_TITLE: &str = "Workspace crate dependencies, reloaded (consumer -> provider)";

fn item(name: &str, open: Option<u32>) -> Item {
    Item {
        name: name.into(),
        role: None,
        open,
    }
}

fn node(item: u32, x: i32, y: i32, width: i32, height: i32) -> Node {
    Node {
        item,
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        lines: Vec::new(),
    }
}

fn region(item: u32, parent: Option<u32>, x: i32, y: i32, width: i32, height: i32) -> Region {
    Region {
        item,
        parent,
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        lines: Vec::new(),
    }
}

fn point(x: i32, y: i32) -> Point {
    Point { x, y }
}

/// crate 図。item a（open = 1 で module 図の index 1 を開く）と b。a→b の一本の辺。
fn crate_scene() -> Scene {
    Scene {
        width: 400,
        height: 200,
        items: vec![item("a", Some(1)), item("b", None)],
        nodes: vec![node(0, 20, 60, 120, 60), node(1, 240, 60, 120, 60)],
        regions: Vec::new(),
        edges: vec![Edge {
            from: 0,
            to: 1,
            style: EdgeStyle::Normal,
            points: vec![point(140, 90), point(240, 90)],
            label: None,
        }],
    }
}

/// module 図（package a の lib）。region は crate（外側）と crate::x（内側）の二つ。crate::x→crate の辺。
fn module_scene() -> Scene {
    Scene {
        width: 400,
        height: 300,
        items: vec![item("crate", None), item("crate::x", None)],
        nodes: vec![node(0, 250, 200, 120, 60), node(1, 60, 100, 120, 60)],
        regions: vec![
            region(0, None, 0, 0, 400, 300),
            region(1, Some(0), 40, 60, 160, 160),
        ],
        edges: vec![Edge {
            from: 1,
            to: 0,
            style: EdgeStyle::Normal,
            points: vec![
                point(180, 130),
                point(220, 130),
                point(220, 230),
                point(250, 230),
            ],
            label: None,
        }],
    }
}

/// 試験用の図のファイル。crate 図の題だけを `title` にする。
fn archive(title: &str) -> Archive {
    Archive {
        root_name: "repo".into(),
        diagrams: vec![
            Diagram {
                kind: DiagramKind::Crates,
                title: title.into(),
                content: Content::Drawn(crate_scene()),
            },
            Diagram {
                kind: DiagramKind::Modules {
                    package: "a".into(),
                    target: "lib".into(),
                    kind: "lib".into(),
                    mode: Mode::Normal,
                },
                title: "a lib".into(),
                content: Content::Drawn(module_scene()),
            },
        ],
        notices: Vec::new(),
    }
}

fn encoded(title: &str) -> Vec<u8> {
    xtask_archive::encode(&archive(title)).expect("the test archive encodes")
}

/// 題 `title` の図のファイルを `dir` に書き、その path を返す。
fn write_archive(dir: &Path, title: &str) -> PathBuf {
    let path = dir.join("workspace.xtaskmap");
    std::fs::write(&path, encoded(title)).expect("the test archive is written");
    path
}

/// 文書を開いた画面と、その画面の状態。
struct Opened {
    screen: Screen,
    state: AppState,
}

/// `path` を引数として開いた画面を作り、読込みが終わるまで進める。
fn open(path: PathBuf) -> Opened {
    let slot = Rc::new(Cell::new(None));
    let handle = Rc::clone(&slot);
    let screen = kivact::test::render(move || {
        let state = AppState::new();
        handle.set(Some(state));
        app::view(state, Some(path))
    });
    screen.run_until_idle();
    let state = slot.get().expect("the setup function ran");
    Opened { screen, state }
}

#[test]
fn opens_file_from_argument() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));

    opened.screen.get_by_text(CRATE_TITLE);
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn canvas_has_positive_size() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));

    let rect = opened
        .screen
        .get_by_role(Role::Canvas, "Diagram")
        .rect()
        .expect("the canvas is laid out");
    assert!(
        rect.width > 100.0 && rect.height > 100.0,
        "the canvas is {} by {}",
        rect.width,
        rect.height
    );
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn click_pins_and_escape_clears() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));

    let a = opened.screen.get_by_role(Role::Button, "a");
    a.click();
    assert!(a.is_selected());

    opened.screen.press_key(Key::Escape);
    assert!(!a.is_selected());
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn double_click_opens_module_diagram_and_renames_regions() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));

    opened.screen.get_by_role(Role::Button, "a").double_click();

    opened.screen.get_by_role(Role::Button, "crate::x");
    assert!(opened.screen.query_by_role(Role::Button, "a").is_none());
    assert!(opened.screen.query_by_role(Role::Button, "b").is_none());
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn drag_on_region_pans() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));
    let screen = &opened.screen;

    let rect = screen
        .get_by_role(Role::Button, "a")
        .rect()
        .expect("the region is laid out");
    let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    screen.mouse_move(x, y);
    screen.mouse_down(MouseButton::Primary);
    screen.mouse_move(x + 40.0, y);
    screen.mouse_up(MouseButton::Primary);

    assert!(matches!(
        opened.state.camera.get_untracked(),
        Camera::Manual { .. }
    ));
    assert_eq!(opened.state.nav.get_untracked().pinned, None);
    assert!(screen.errors().is_empty());
}

#[test]
fn search_filters_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let opened = open(write_archive(dir.path(), CRATE_TITLE));

    opened
        .screen
        .get_by_role(Role::TextBox, "Search")
        .type_text("x");

    opened.screen.get_by_text("crate::x");
    assert!(opened.screen.query_by_text("b").is_none());
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn reload_after_change() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = write_archive(dir.path(), CRATE_TITLE);
    let opened = open(path.clone());
    opened.screen.get_by_text(CRATE_TITLE);

    std::fs::write(&path, encoded(RELOADED_TITLE)).expect("the changed archive is written");
    opened.screen.advance(Duration::from_millis(600));
    opened.screen.run_until_idle();

    opened.screen.find_by_text(RELOADED_TITLE);
    assert!(opened.screen.errors().is_empty());
}

#[test]
fn export_svg_matches_screen_paint() {
    let picture = xtask_render::picture(CRATE_TITLE, &crate_scene());

    let svg = Offscreen::new()
        .render_svg(picture.width, picture.height, |draw, _| {
            paint::picture(draw, &picture, paint::Mode::Export)
        })
        .expect("the SVG renders");

    assert!(svg.contains("<svg"), "no root element in the SVG");
    assert!(
        svg.contains("Workspace crate dependencies (consumer -&gt; provider)"),
        "the title is missing or not escaped"
    );
    assert!(
        svg.contains("font-family=\"monospace\""),
        "the font family is missing"
    );
}
