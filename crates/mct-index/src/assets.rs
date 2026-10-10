//! Asset pre-pass (ADR-003 D1): binary and data files code refers to
//! (`.glb`, `.png`, …) get a `files` row with language `asset` and one
//! `asset` symbol, so a relation can resolve to them. Like `manifests.rs`,
//! this runs in `mct-index` before the `LanguageParser` registry and is not
//! a language. Images are only stat'ed; glTF JSON is read under a cap, and a
//! `.glb`'s BIN chunk is never read.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// File extensions (lowercase, no dot) indexed as assets.
pub const ASSET_EXTENSIONS: &[&str] = &[
    "glb", "gltf", "png", "jpg", "jpeg", "webp", "ktx2", "hdr", "exr",
];

/// Language id stored in `files.language` for an asset.
pub const ASSET_LANGUAGE: &str = "asset";

/// Largest glTF JSON read, as a whole `.gltf` file or a `.glb` JSON chunk.
pub const MAX_GLTF_JSON_BYTES: u64 = 8 * 1024 * 1024;

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_JSON_CHUNK: &[u8; 4] = b"JSON";

/// Why an asset's glTF JSON could not be read.
#[derive(Debug)]
pub enum AssetError {
    /// The file could not be opened or read: a `read_failure`.
    Io(io::Error),
    /// The content is malformed, truncated or over a cap: a `syntax_error`
    /// that keeps the asset symbol.
    Invalid(String),
}

impl From<io::Error> for AssetError {
    fn from(err: io::Error) -> Self {
        AssetError::Io(err)
    }
}

/// The asset extension of `file_name`, lowercased, if it is one.
pub fn asset_extension(file_name: &str) -> Option<String> {
    let (_, ext) = file_name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    ASSET_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// The glTF JSON text of the asset at `path` (`len` bytes long), or `None`
/// for a format with no JSON (images, HDR/EXR), which is never opened.
pub fn read_gltf_json(
    path: &Path,
    extension: &str,
    len: u64,
) -> Result<Option<String>, AssetError> {
    let bytes = match extension {
        "gltf" => {
            if len > MAX_GLTF_JSON_BYTES {
                return Err(AssetError::Invalid(format!(
                    "gltf: {len} B exceeds {MAX_GLTF_JSON_BYTES} B cap"
                )));
            }
            crate::read_repository_file(path)?
        }
        "glb" => match read_glb_json_chunk(&mut File::open(path)?, len) {
            Err(AssetError::Io(err)) if err.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(AssetError::Invalid(format!("glb: truncated ({len} B)")));
            }
            other => other?,
        },
        _ => return Ok(None),
    };
    let prefix = if extension == "glb" { "glb" } else { "gltf" };
    let text = String::from_utf8(bytes)
        .map_err(|_| AssetError::Invalid(format!("{prefix}: JSON is not UTF-8")))?;
    if let Err(err) = serde_json::from_str::<serde_json::Value>(&text) {
        return Err(AssetError::Invalid(format!("{prefix}: {err}")));
    }
    Ok(Some(text))
}

/// Reads a binary glTF's JSON chunk from `reader`, positioned at the start
/// of a file `file_len` bytes long: the 12-byte header (magic `glTF`,
/// version 2), then the first chunk header, which must be a `JSON` chunk
/// of at most [`MAX_GLTF_JSON_BYTES`] that fits the file. Reads exactly
/// that chunk, never the BIN chunk after it, and trims trailing space
/// padding. A short read is an `UnexpectedEof` I/O error.
pub fn read_glb_json_chunk(reader: &mut impl Read, file_len: u64) -> Result<Vec<u8>, AssetError> {
    let invalid = |detail: String| Err(AssetError::Invalid(format!("glb: {detail}")));
    let mut header = [0u8; 20];
    reader.read_exact(&mut header)?;
    let word = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    if &header[0..4] != GLB_MAGIC {
        return invalid("bad magic".to_string());
    }
    if word(4) != 2 {
        return invalid(format!("unsupported version {}", word(4)));
    }
    if &header[16..20] != GLB_JSON_CHUNK {
        return invalid("first chunk is not JSON".to_string());
    }
    let chunk_len = u64::from(word(12));
    if chunk_len > MAX_GLTF_JSON_BYTES {
        return invalid(format!(
            "JSON chunk {chunk_len} B exceeds {MAX_GLTF_JSON_BYTES} B cap"
        ));
    }
    if chunk_len > file_len.saturating_sub(20) {
        return invalid(format!(
            "JSON chunk {chunk_len} B exceeds the {file_len} B file"
        ));
    }
    let mut chunk = Vec::new();
    reader.take(chunk_len).read_to_end(&mut chunk)?;
    if (chunk.len() as u64) < chunk_len {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
    }
    while chunk.last() == Some(&b' ') {
        chunk.pop();
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn glb(version: u32, chunk_type: &[u8; 4], declared: u32, json: &[u8]) -> Vec<u8> {
        let mut out = b"glTF".to_vec();
        out.extend(version.to_le_bytes());
        out.extend((20 + json.len() as u32).to_le_bytes());
        out.extend(declared.to_le_bytes());
        out.extend(chunk_type);
        out.extend(json);
        out
    }

    fn read(bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
        read_glb_json_chunk(&mut &bytes[..], bytes.len() as u64)
    }

    fn detail(result: Result<Vec<u8>, AssetError>) -> String {
        match result {
            Err(AssetError::Invalid(d)) => d,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn reads_the_json_chunk_and_trims_padding() {
        let bytes = glb(2, b"JSON", 12, b"{\"a\":1}     BIN-garbage");
        assert_eq!(read(&bytes).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn rejects_bad_headers() {
        assert_eq!(
            detail(read(&glb(1, b"JSON", 2, b"{}"))),
            "glb: unsupported version 1"
        );
        assert_eq!(
            detail(read(&glb(2, b"BIN\0", 2, b"{}"))),
            "glb: first chunk is not JSON"
        );
        let mut bad = glb(2, b"JSON", 2, b"{}");
        bad[0] = b'x';
        assert_eq!(detail(read(&bad)), "glb: bad magic");
    }

    #[test]
    fn enforces_the_cap_and_the_file_length() {
        let over = MAX_GLTF_JSON_BYTES as u32 + 1;
        assert_eq!(
            detail(read(&glb(2, b"JSON", over, b"{}"))),
            format!("glb: JSON chunk {over} B exceeds 8388608 B cap")
        );
        assert_eq!(
            detail(read(&glb(2, b"JSON", 100, b"{}"))),
            "glb: JSON chunk 100 B exceeds the 22 B file"
        );
    }

    #[test]
    fn a_short_read_is_unexpected_eof() {
        let bytes = glb(2, b"JSON", 2, b"{}");
        for len in [0, 5, 19] {
            match read(&bytes[..len]) {
                Err(AssetError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof),
                other => panic!("{len}: {other:?}"),
            }
        }
        // The stated file length is larger than what the reader yields.
        match read_glb_json_chunk(&mut &bytes[..21], bytes.len() as u64) {
            Err(AssetError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn asset_extensions_are_case_insensitive() {
        assert_eq!(asset_extension("robot.GLB").as_deref(), Some("glb"));
        assert_eq!(asset_extension("a.tar.png").as_deref(), Some("png"));
        assert_eq!(asset_extension("glb"), None);
        assert_eq!(asset_extension("a.json"), None);
    }
}
