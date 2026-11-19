//! Role: 図のファイルの本体の書手と読手、および zlib の圧縮と復号を定義する。

use crate::model::{
    Archive, Content, Diagram, DiagramKind, Edge, EdgeStyle, Item, Label, Mode, Node, Point, Rect,
    Region, Scene,
};
use crate::model::{DecodeError, FORMAT_VERSION, MAX_BODY_LEN};

pub(crate) const MAGIC: &[u8] = b"XTASKMAP";
/// 見出し（magic、版、本体の長さ）の大きさ。
pub(crate) const HEADER_LEN: usize = 20;
const COMPRESSION_LEVEL: u8 = 6;
const TOO_LARGE: &str = "archive is too large";

type ReadResult<T> = Result<T, DecodeError>;

/// 見出しを付けて本体を圧縮する。
pub(crate) fn wrap(body: &[u8]) -> Vec<u8> {
    let mut file = Vec::with_capacity(HEADER_LEN);
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    file.extend_from_slice(&(body.len() as u64).to_le_bytes());
    file.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(
        body,
        COMPRESSION_LEVEL,
    ));
    file
}

/// zlib の stream を一つだけ、ちょうど `len` バイトに戻す。
pub(crate) fn inflate_exact(input: &[u8], len: usize) -> Result<Vec<u8>, DecodeError> {
    use miniz_oxide::inflate::TINFLStatus;
    use miniz_oxide::inflate::core::{DecompressorOxide, decompress, inflate_flags};

    let mut out = vec![0u8; len];
    let mut state = Box::<DecompressorOxide>::default();
    let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER
        | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF
        | inflate_flags::TINFL_FLAG_COMPUTE_ADLER32;
    let (status, read, written) = decompress(&mut state, input, &mut out, 0, flags);
    match status {
        TINFLStatus::Done if read == input.len() && written == len => Ok(out),
        _ => Err(DecodeError::Corrupt("bad compression".into())),
    }
}

/// 本体を書く。検証は呼出し側（`encode`）が行う。
pub(crate) fn write_body(archive: &Archive) -> Result<Vec<u8>, String> {
    let mut writer = Writer { out: Vec::new() };
    write_archive(&mut writer, archive)?;
    if writer.out.len() > MAX_BODY_LEN {
        return Err(TOO_LARGE.into());
    }
    Ok(writer.out)
}

/// 本体を読み、最後まで消費したことを確かめる。検証は呼出し側（`decode`）が行う。
pub(crate) fn read_body(body: &[u8]) -> Result<Archive, DecodeError> {
    let mut reader = Reader::new(body);
    let archive = read_archive(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(corrupt("trailing bytes"));
    }
    Ok(archive)
}

fn corrupt(reason: &str) -> DecodeError {
    DecodeError::Corrupt(reason.to_string())
}

fn unknown_tag(tag: u8) -> DecodeError {
    corrupt(&format!("unknown tag {tag}"))
}

// ---- 書手 ----

pub(crate) struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, value: u8) {
        self.out.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// 要素数（または文字のバイト数）を書く。
    fn vec(&mut self, len: usize) -> Result<(), String> {
        let len = u32::try_from(len).map_err(|_| TOO_LARGE.to_string())?;
        self.u32(len);
        Ok(())
    }

    fn str(&mut self, text: &str) -> Result<(), String> {
        self.vec(text.len())?;
        self.out.extend_from_slice(text.as_bytes());
        Ok(())
    }

    fn opt(&mut self, present: bool) {
        self.u8(u8::from(present));
    }
}

fn write_list<T>(
    writer: &mut Writer,
    items: &[T],
    write: impl Fn(&mut Writer, &T) -> Result<(), String>,
) -> Result<(), String> {
    writer.vec(items.len())?;
    items.iter().try_for_each(|item| write(writer, item))
}

fn write_option<T>(
    writer: &mut Writer,
    value: &Option<T>,
    write: impl Fn(&mut Writer, &T) -> Result<(), String>,
) -> Result<(), String> {
    writer.opt(value.is_some());
    match value {
        Some(inner) => write(writer, inner),
        None => Ok(()),
    }
}

fn write_archive(writer: &mut Writer, archive: &Archive) -> Result<(), String> {
    writer.str(&archive.root_name)?;
    write_list(writer, &archive.diagrams, write_diagram)?;
    write_list(writer, &archive.notices, |writer, notice| {
        writer.str(notice)
    })
}

fn write_diagram(writer: &mut Writer, diagram: &Diagram) -> Result<(), String> {
    write_kind(writer, &diagram.kind)?;
    writer.str(&diagram.title)?;
    write_content(writer, &diagram.content)
}

fn write_kind(writer: &mut Writer, kind: &DiagramKind) -> Result<(), String> {
    match kind {
        DiagramKind::Crates => {
            writer.u8(0);
            Ok(())
        }
        DiagramKind::Modules {
            package,
            target,
            kind,
            mode,
        } => {
            writer.u8(1);
            writer.str(package)?;
            writer.str(target)?;
            writer.str(kind)?;
            write_mode(writer, *mode);
            Ok(())
        }
    }
}

fn write_mode(writer: &mut Writer, mode: Mode) {
    writer.u8(match mode {
        Mode::Normal => 0,
        Mode::Test => 1,
    });
}

fn write_content(writer: &mut Writer, content: &Content) -> Result<(), String> {
    match content {
        Content::Drawn(scene) => {
            writer.u8(0);
            write_scene(writer, scene)
        }
        Content::Failed(reason) => {
            writer.u8(1);
            writer.str(reason)
        }
        Content::Skipped(reason) => {
            writer.u8(2);
            writer.str(reason)
        }
    }
}

fn write_scene(writer: &mut Writer, scene: &Scene) -> Result<(), String> {
    writer.i32(scene.width);
    writer.i32(scene.height);
    write_list(writer, &scene.items, write_item)?;
    write_list(writer, &scene.nodes, write_node)?;
    write_list(writer, &scene.regions, write_region)?;
    write_list(writer, &scene.edges, write_edge)
}

fn write_item(writer: &mut Writer, item: &Item) -> Result<(), String> {
    writer.str(&item.name)?;
    write_option(writer, &item.role, |writer, role| writer.str(role))?;
    write_option(writer, &item.open, |writer, open| {
        writer.u32(*open);
        Ok(())
    })
}

fn write_node(writer: &mut Writer, node: &Node) -> Result<(), String> {
    writer.u32(node.item);
    write_rect(writer, &node.rect);
    write_list(writer, &node.lines, |writer, line| writer.str(line))
}

fn write_region(writer: &mut Writer, region: &Region) -> Result<(), String> {
    writer.u32(region.item);
    write_option(writer, &region.parent, |writer, parent| {
        writer.u32(*parent);
        Ok(())
    })?;
    write_rect(writer, &region.rect);
    write_list(writer, &region.lines, |writer, line| writer.str(line))
}

fn write_edge(writer: &mut Writer, edge: &Edge) -> Result<(), String> {
    writer.u32(edge.from);
    writer.u32(edge.to);
    writer.u8(match edge.style {
        EdgeStyle::Normal => 0,
        EdgeStyle::Dev => 1,
        EdgeStyle::Build => 2,
    });
    write_list(writer, &edge.points, |writer, point| {
        write_point(writer, point);
        Ok(())
    })?;
    write_option(writer, &edge.label, write_label)
}

fn write_label(writer: &mut Writer, label: &Label) -> Result<(), String> {
    write_rect(writer, &label.rect);
    write_list(writer, &label.lines, |writer, line| writer.str(line))
}

fn write_rect(writer: &mut Writer, rect: &Rect) {
    writer.i32(rect.x);
    writer.i32(rect.y);
    writer.i32(rect.width);
    writer.i32(rect.height);
}

fn write_point(writer: &mut Writer, point: &Point) {
    writer.i32(point.x);
    writer.i32(point.y);
}

// ---- 読手 ----

/// 確保の予算つきの読手。
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    budget: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            budget: 8 * bytes.len() + 4096,
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn take(&mut self, count: usize) -> ReadResult<&'a [u8]> {
        let bytes = self.bytes;
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| corrupt("unexpected end"))?;
        let slice = &bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> ReadResult<[u8; N]> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| corrupt("unexpected end"))
    }

    fn u8(&mut self) -> ReadResult<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> ReadResult<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> ReadResult<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn flag(&mut self) -> ReadResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(unknown_tag(tag)),
        }
    }

    /// 要素数（または文字のバイト数）を読み、残りのバイト数と予算を確かめてから予算を引く。
    fn length(&mut self, element: usize) -> ReadResult<usize> {
        let count = self.u32()? as usize;
        if count > self.remaining() {
            return Err(corrupt("length out of range"));
        }
        let bytes = count
            .checked_mul(element)
            .ok_or_else(|| corrupt("allocation budget exceeded"))?;
        self.budget = self
            .budget
            .checked_sub(bytes)
            .ok_or_else(|| corrupt("allocation budget exceeded"))?;
        Ok(count)
    }

    fn vec_len<T>(&mut self) -> ReadResult<usize> {
        self.length(std::mem::size_of::<T>())
    }

    fn str(&mut self) -> ReadResult<String> {
        let count = self.length(1)?;
        let bytes = self.take(count)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("invalid UTF-8"))
    }
}

fn read_list<'a, T>(
    reader: &mut Reader<'a>,
    mut read: impl FnMut(&mut Reader<'a>) -> ReadResult<T>,
) -> ReadResult<Vec<T>> {
    let count = reader.vec_len::<T>()?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(read(reader)?);
    }
    Ok(items)
}

fn read_option<'a, T>(
    reader: &mut Reader<'a>,
    read: impl FnOnce(&mut Reader<'a>) -> ReadResult<T>,
) -> ReadResult<Option<T>> {
    if reader.flag()? {
        Ok(Some(read(reader)?))
    } else {
        Ok(None)
    }
}

fn read_archive(reader: &mut Reader<'_>) -> ReadResult<Archive> {
    let root_name = reader.str()?;
    let diagrams = read_list(reader, read_diagram)?;
    let notices = read_list(reader, |reader| reader.str())?;
    Ok(Archive {
        root_name,
        diagrams,
        notices,
    })
}

fn read_diagram(reader: &mut Reader<'_>) -> ReadResult<Diagram> {
    let kind = read_kind(reader)?;
    let title = reader.str()?;
    let content = read_content(reader)?;
    Ok(Diagram {
        kind,
        title,
        content,
    })
}

fn read_kind(reader: &mut Reader<'_>) -> ReadResult<DiagramKind> {
    match reader.u8()? {
        0 => Ok(DiagramKind::Crates),
        1 => {
            let package = reader.str()?;
            let target = reader.str()?;
            let kind = reader.str()?;
            let mode = read_mode(reader)?;
            Ok(DiagramKind::Modules {
                package,
                target,
                kind,
                mode,
            })
        }
        tag => Err(unknown_tag(tag)),
    }
}

fn read_mode(reader: &mut Reader<'_>) -> ReadResult<Mode> {
    match reader.u8()? {
        0 => Ok(Mode::Normal),
        1 => Ok(Mode::Test),
        tag => Err(unknown_tag(tag)),
    }
}

fn read_content(reader: &mut Reader<'_>) -> ReadResult<Content> {
    match reader.u8()? {
        0 => Ok(Content::Drawn(read_scene(reader)?)),
        1 => Ok(Content::Failed(reader.str()?)),
        2 => Ok(Content::Skipped(reader.str()?)),
        tag => Err(unknown_tag(tag)),
    }
}

fn read_scene(reader: &mut Reader<'_>) -> ReadResult<Scene> {
    let width = reader.i32()?;
    let height = reader.i32()?;
    let items = read_list(reader, read_item)?;
    let nodes = read_list(reader, read_node)?;
    let regions = read_list(reader, read_region)?;
    let edges = read_list(reader, read_edge)?;
    Ok(Scene {
        width,
        height,
        items,
        nodes,
        regions,
        edges,
    })
}

fn read_item(reader: &mut Reader<'_>) -> ReadResult<Item> {
    let name = reader.str()?;
    let role = read_option(reader, |reader| reader.str())?;
    let open = read_option(reader, |reader| reader.u32())?;
    Ok(Item { name, role, open })
}

fn read_node(reader: &mut Reader<'_>) -> ReadResult<Node> {
    let item = reader.u32()?;
    let rect = read_rect(reader)?;
    let lines = read_list(reader, |reader| reader.str())?;
    Ok(Node { item, rect, lines })
}

fn read_region(reader: &mut Reader<'_>) -> ReadResult<Region> {
    let item = reader.u32()?;
    let parent = read_option(reader, |reader| reader.u32())?;
    let rect = read_rect(reader)?;
    let lines = read_list(reader, |reader| reader.str())?;
    Ok(Region {
        item,
        parent,
        rect,
        lines,
    })
}

fn read_edge(reader: &mut Reader<'_>) -> ReadResult<Edge> {
    let from = reader.u32()?;
    let to = reader.u32()?;
    let style = read_style(reader)?;
    let points = read_list(reader, read_point)?;
    let label = read_option(reader, read_label)?;
    Ok(Edge {
        from,
        to,
        style,
        points,
        label,
    })
}

fn read_style(reader: &mut Reader<'_>) -> ReadResult<EdgeStyle> {
    match reader.u8()? {
        0 => Ok(EdgeStyle::Normal),
        1 => Ok(EdgeStyle::Dev),
        2 => Ok(EdgeStyle::Build),
        tag => Err(unknown_tag(tag)),
    }
}

fn read_label(reader: &mut Reader<'_>) -> ReadResult<Label> {
    let rect = read_rect(reader)?;
    let lines = read_list(reader, |reader| reader.str())?;
    Ok(Label { rect, lines })
}

fn read_rect(reader: &mut Reader<'_>) -> ReadResult<Rect> {
    let x = reader.i32()?;
    let y = reader.i32()?;
    let width = reader.i32()?;
    let height = reader.i32()?;
    Ok(Rect {
        x,
        y,
        width,
        height,
    })
}

fn read_point(reader: &mut Reader<'_>) -> ReadResult<Point> {
    let x = reader.i32()?;
    let y = reader.i32()?;
    Ok(Point { x, y })
}
