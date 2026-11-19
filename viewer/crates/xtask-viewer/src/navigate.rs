//! Role: 図の移動と固定の遷移（履歴、固定、視点の変更）。
use crate::camera;
use crate::state::{AppState, Camera, Gesture, Loaded, Nav};

/// 図 `diagram`（archive の index）を開く。今の図と同じ kind なら移動せず、固定だけを解く。
/// 違えば、今の図を履歴に積む。index が無ければ何もしない。
pub fn open(nav: &Nav, loaded: &Loaded, diagram: usize) -> Nav {
    let Some(target) = loaded.archive.diagrams.get(diagram) else {
        return nav.clone();
    };
    if target.kind == nav.diagram {
        return Nav {
            pinned: None,
            ..nav.clone()
        };
    }
    let mut back = nav.back.clone();
    back.push(nav.diagram.clone());
    Nav {
        diagram: target.kind.clone(),
        back,
        pinned: None,
    }
}

/// 履歴の最後の図へ戻る。履歴が空なら何もしない。
pub fn back(nav: &Nav) -> Nav {
    let mut back = nav.back.clone();
    match back.pop() {
        Some(diagram) => Nav {
            diagram,
            back,
            pinned: None,
        },
        None => nav.clone(),
    }
}

/// 固定の切替え。同じ item なら解き、違えば固定し直す。
pub fn toggle_pin(nav: &Nav, item: u32) -> Nav {
    let pinned = if nav.pinned == Some(item) {
        None
    } else {
        Some(item)
    };
    Nav {
        pinned,
        ..nav.clone()
    }
}

pub fn unpin(nav: &Nav) -> Nav {
    Nav {
        pinned: None,
        ..nav.clone()
    }
}

/// 図を開き、視点を全体に合わせる。
pub fn go(state: AppState, diagram: usize) {
    let Some((loaded, _)) = state.current() else {
        return;
    };
    let nav = state.nav.get_untracked();
    state.nav.set(open(&nav, &loaded, diagram));
    state.camera.set(Camera::Fit);
    state.hover.set(None);
    state.gesture.set(Gesture::Idle);
}

pub fn go_back(state: AppState) {
    let nav = state.nav.get_untracked();
    state.nav.set(back(&nav));
    state.camera.set(Camera::Fit);
    state.hover.set(None);
    state.gesture.set(Gesture::Idle);
}

pub fn fit(state: AppState) {
    state.camera.set(Camera::Fit);
}

/// 図 `diagram` を開き、item を固定して、その要素を中央に寄せる。
/// 拡大率は今の視点のものを保つ（今の図が無ければ、開く図を全体に合わせた拡大率）。
pub fn reveal(state: AppState, diagram: usize, item: u32) {
    let Some((loaded, index)) = state.current() else {
        return;
    };
    let nav = state.nav.get_untracked();
    let mut next = open(&nav, &loaded, diagram);
    next.pinned = Some(item);
    state.nav.set(next);
    state.hover.set(None);
    state.gesture.set(Gesture::Idle);

    let viewport = state.viewport.get();
    let now = state.camera.get_untracked();
    let shown = loaded.pictures.get(index).and_then(Option::as_ref);
    let target = loaded.pictures.get(diagram).and_then(Option::as_ref);
    let (Some(picture), Some(basis)) = (target, shown.or(target)) else {
        return;
    };
    let scale = camera::view(now, basis, viewport).scale;
    if let Some(rect) = xtask_render::item_rect(picture, item) {
        state.camera.set(camera::center(rect, scale, viewport));
    }
}

#[cfg(test)]
mod tests {
    //! Role: 図の間の移動（履歴・pin）の試験。
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use xtask_archive::{Archive, Content, Diagram, DiagramKind, Mode};

    fn module(target: &str) -> DiagramKind {
        DiagramKind::Modules {
            package: "a".into(),
            target: target.into(),
            kind: "lib".into(),
            mode: Mode::Normal,
        }
    }

    fn diagram(kind: DiagramKind) -> Diagram {
        Diagram {
            kind,
            title: String::new(),
            content: Content::Failed(String::new()),
        }
    }

    fn loaded() -> Loaded {
        Loaded {
            path: PathBuf::new(),
            archive: Archive {
                root_name: String::new(),
                diagrams: vec![diagram(DiagramKind::Crates), diagram(module("a"))],
                notices: Vec::new(),
            },
            pictures: vec![None, None],
            kinds: BTreeMap::new(),
        }
    }

    #[test]
    fn open_pushes_history_and_clears_pin() {
        let nav = Nav {
            diagram: DiagramKind::Crates,
            back: Vec::new(),
            pinned: Some(3),
        };
        let opened = open(&nav, &loaded(), 1);
        assert_eq!(
            opened,
            Nav {
                diagram: module("a"),
                back: vec![DiagramKind::Crates],
                pinned: None,
            }
        );
    }

    #[test]
    fn back_pops_and_clears_pin() {
        let nav = Nav {
            diagram: module("a"),
            back: vec![DiagramKind::Crates],
            pinned: Some(1),
        };
        assert_eq!(
            back(&nav),
            Nav {
                diagram: DiagramKind::Crates,
                back: Vec::new(),
                pinned: None,
            }
        );
    }

    #[test]
    fn back_on_empty_history_is_unchanged() {
        let nav = Nav {
            diagram: DiagramKind::Crates,
            back: Vec::new(),
            pinned: Some(2),
        };
        assert_eq!(back(&nav), nav);
    }

    #[test]
    fn toggle_pin_twice_unpins() {
        let nav = Nav::start();
        let pinned = toggle_pin(&nav, 4);
        assert_eq!(pinned.pinned, Some(4));
        assert_eq!(toggle_pin(&pinned, 4).pinned, None);
    }
}
