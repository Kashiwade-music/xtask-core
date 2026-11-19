//! Role: 図の文字の大きさ・行の高さ・字幅の規則と、折り返しを定義する。

pub const NODE_FONT_SIZE: f64 = 14.0;
pub const EDGE_FONT_SIZE: f64 = 14.0;
pub const REGION_FONT_SIZE: f64 = 12.0;
pub const TITLE_FONT_SIZE: f64 = 18.0;
pub const LINE_HEIGHT: f64 = 18.0;
pub const REGION_LINE_HEIGHT: f64 = 16.0;
/// 描画の font。SVG の font-family と viewer の TextFormat.family に使う。
/// 総称の一つだけにする（固定の Kivact の SVG 出力は `,` の候補の列を一つの名前として扱う）。
pub const FONT_FAMILY: &str = "monospace";

/// 字幅の見積り。一般の等幅 font（0.6em 前後）より広めにして、はみ出しにくくする（保証ではない）。
pub fn advance(character: char, size: f64) -> f64 {
    if character.is_ascii() {
        size * 0.62
    } else {
        size * 1.05
    }
}

/// 折り返した文字の塊。
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<String>,
    pub width: f64,
    pub height: f64,
}

/// `\n` で行を分け、各行を `max_width` で折り返す。
pub fn measure(text: &str, size: f64, line_height: f64, max_width: f64) -> TextBlock {
    let mut lines = Vec::new();
    let mut widest = 0.0_f64;
    for input in text.split('\n') {
        let mut line = String::new();
        let mut width = 0.0;
        for character in input.chars() {
            let next = advance(character, size);
            if width + next > max_width && !line.is_empty() {
                widest = widest.max(width);
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            line.push(character);
            width += next;
        }
        widest = widest.max(width);
        lines.push(line);
    }
    TextBlock {
        height: lines.len() as f64 * line_height,
        lines,
        width: widest,
    }
}
