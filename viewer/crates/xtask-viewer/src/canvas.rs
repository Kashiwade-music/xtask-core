//! Role: 図の canvas（重ねた二つの canvas、region の宣言、pointer・wheel・tooltip）。
use kivact::prelude::{IntoView, View};

use crate::state::AppState;

pub fn diagram_view(state: AppState) -> impl IntoView {
    fn unbuilt() -> View {
        todo!("V2d")
    }
    let _ = state;
    unbuilt()
}
