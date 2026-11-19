//! Role: 図のファイル（架構図の保存形式）の型・符号化・復号・検証を公開する。

mod codec;
mod model;
pub mod style;
#[cfg(test)]
mod tests;
mod validate;

pub use model::*;
pub use validate::validate;

use codec::{FORMAT_VERSION, HEADER_LEN, MAGIC};

/// 本体（圧縮前）の長さの上限。
pub const MAX_BODY_LEN: usize = 64 * 1024 * 1024;
/// ファイルの長さの上限。viewer は読む前にこれを確かめる。
pub const MAX_FILE_LEN: u64 = 65 * 1024 * 1024;

/// 検証に通らない値、上限を超える値は Err（文は validate の文、または "archive is too large"）。
pub fn encode(archive: &Archive) -> Result<Vec<u8>, String> {
    validate(archive)?;
    let body = codec::write_body(archive)?;
    Ok(codec::wrap(&body))
}

pub fn decode(bytes: &[u8]) -> Result<Archive, DecodeError> {
    if bytes.len() < HEADER_LEN || !bytes.starts_with(MAGIC) {
        return Err(DecodeError::NotArchive);
    }
    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if version != FORMAT_VERSION {
        return Err(DecodeError::Version { found: version });
    }
    let body_len = u64::from_le_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
    ]);
    if body_len > MAX_BODY_LEN as u64 {
        return Err(DecodeError::TooLarge);
    }
    let body_len = usize::try_from(body_len).map_err(|_| DecodeError::TooLarge)?;
    let body = codec::inflate_exact(&bytes[HEADER_LEN..], body_len)?;
    let archive = codec::read_body(&body)?;
    validate(&archive).map_err(DecodeError::Corrupt)?;
    Ok(archive)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// magic が違う、またはファイルが 20 バイトより短い。
    NotArchive,
    /// 読めない版。
    Version { found: u32 },
    /// 本体の長さが上限（`MAX_BODY_LEN`）を超える。
    TooLarge,
    /// 圧縮・文法・意味の誤り。文は最初の誤り。
    Corrupt(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotArchive => write!(formatter, "not an xtask architecture file"),
            Self::Version { found } => write!(
                formatter,
                "unsupported architecture file version {found}; this program reads version {FORMAT_VERSION}"
            ),
            Self::TooLarge => write!(formatter, "architecture file is too large"),
            Self::Corrupt(reason) => write!(formatter, "corrupt architecture file: {reason}"),
        }
    }
}

impl std::error::Error for DecodeError {}
