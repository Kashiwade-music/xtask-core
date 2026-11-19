//! Role: 図のファイル（架構図の保存形式）の符号化・復号の入口を公開する。

mod codec;
mod model;
pub mod style;
#[cfg(test)]
mod tests;
mod validate;

pub use model::*;
pub use validate::validate;

use codec::{HEADER_LEN, MAGIC};

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
