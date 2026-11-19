//! Role: 図の文字を折り返し、配置に使う整数の大きさ（px、切り上げ）を作る。
use xtask_archive::style::{
    self, LINE_HEIGHT, NODE_FONT_SIZE, REGION_FONT_SIZE, REGION_LINE_HEIGHT,
};
use xtask_layout::Size;

/// node と辺のラベルの最大の幅。
pub(crate) const LABEL_WIDTH: f64 = 320.0;
/// region の見出しの最大の幅。
pub(crate) const REGION_LABEL_WIDTH: f64 = 200.0;
/// Role が無いときの文字。
pub(crate) const NO_ROLE: &str = "[役割未記載]";

/// 折り返した文字と、その塊の大きさ。
pub(crate) struct Text {
    pub(crate) lines: Vec<String>,
    pub(crate) size: Size,
}

/// 文字を折り返して測る（node・辺のラベルの font）。
pub(crate) fn node_text(value: &str) -> Text {
    measured(value, NODE_FONT_SIZE, LINE_HEIGHT, LABEL_WIDTH)
}

/// region の見出しを折り返して測る。
pub(crate) fn region_text(value: &str) -> Text {
    measured(
        value,
        REGION_FONT_SIZE,
        REGION_LINE_HEIGHT,
        REGION_LABEL_WIDTH,
    )
}

fn measured(value: &str, font: f64, line_height: f64, max_width: f64) -> Text {
    let block = style::measure(value, font, line_height, max_width);
    Text {
        lines: block.lines,
        size: Size {
            width: ceil(block.width),
            height: ceil(block.height),
        },
    }
}

/// 文字の塊の周りの node の大きさ（余白を足し、最小の大きさを守る）。
pub(crate) fn node_box(text: &Text) -> Size {
    Size {
        width: (text.size.width + 28).max(96),
        height: (text.size.height + 24).max(42),
    }
}

fn ceil(value: f64) -> i64 {
    value.ceil() as i64
}
