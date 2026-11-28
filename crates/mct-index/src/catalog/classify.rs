//! The path catalog's classifier: a file name, the parser that claims it and
//! (only for a conflicting suffix) a bounded signature prefix in; a
//! category, a canonical format and the evidence behind them out. Pure: the
//! caller does the one bounded read ([`read_signature`]).

use std::io::{self, Read};

use super::formats::{self, Category, CONFLICTING_SUFFIXES, NAMED_DOCUMENTS};

/// Most bytes ever read to classify a file: enough for every signature
/// below and for a TIFF's first directory in the usual layout.
pub const SIGNATURE_BYTES: u64 = 4096;

/// Why a file got its category and format, as stored in
/// `path_catalog.evidence`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// The registered suffix alone.
    Extension,
    /// A signature in the file's first bytes confirmed one reading of a
    /// conflicting suffix.
    Signature,
    /// A language parser claims the file (category `source`).
    Parser,
    /// A conventional extensionless name (`README`, `LICENSE`).
    Name,
    /// A conflicting suffix the signals could not settle; the note says
    /// which readings remain.
    Ambiguous,
    /// No registered suffix and no parser.
    Unknown,
}

impl Evidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Evidence::Extension => "extension",
            Evidence::Signature => "signature",
            Evidence::Parser => "parser",
            Evidence::Name => "name",
            Evidence::Ambiguous => "ambiguous",
            Evidence::Unknown => "unknown",
        }
    }
}

/// One file's classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub category: Category,
    /// Canonical format id, the parser's language id for `source`, or empty
    /// when unknown.
    pub format: String,
    pub evidence: Evidence,
    /// Why a doubtful classification is what it is; `None` when plain.
    pub note: Option<String>,
}

impl Classification {
    fn new(category: Category, format: &str, evidence: Evidence) -> Self {
        Classification {
            category,
            format: format.to_string(),
            evidence,
            note: None,
        }
    }

    fn noted(mut self, note: String) -> Self {
        self.note = Some(note);
        self
    }
}

/// The candidate suffixes of `file_name`, lowercase, most specific first:
/// `bundle.tar.gz` gives `tar.gz`, `gz`; `libz.so.1.2` gives `so`. Leading
/// dots belong to the name, so `.gitignore` has none.
pub fn suffixes(file_name: &str) -> Vec<String> {
    let lower = file_name.trim_start_matches('.').to_lowercase();
    let parts: Vec<&str> = lower.split('.').collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    // A versioned shared library: `.so` followed only by numeric parts.
    if let Some(at) = parts.iter().rposition(|p| *p == "so") {
        let tail = &parts[at + 1..];
        if at > 0 && !tail.is_empty() && tail.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        {
            return vec!["so".to_string()];
        }
    }
    let mut out = Vec::new();
    if parts.len() >= 3 {
        out.push(parts[parts.len() - 2..].join("."));
    }
    out.push(parts[parts.len() - 1].to_string());
    out
}

/// Whether classifying `file_name` needs [`read_signature`]: only a
/// conflicting suffix does, so most files are never opened.
pub fn needs_signature(file_name: &str) -> bool {
    suffixes(file_name)
        .iter()
        .any(|s| CONFLICTING_SUFFIXES.contains(&s.as_str()))
}

/// Up to [`SIGNATURE_BYTES`] from `reader`, never more, whatever its size.
pub fn read_signature(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut prefix = Vec::new();
    reader.take(SIGNATURE_BYTES).read_to_end(&mut prefix)?;
    Ok(prefix)
}

/// Classifies `file_name`. `parser` is the language id of the parser the
/// indexer would run on it; `signature` the file's first bytes, needed only
/// when [`needs_signature`] says so (`None` reads as "no evidence").
pub fn classify(file_name: &str, parser: Option<&str>, signature: Option<&[u8]>) -> Classification {
    let candidates = suffixes(file_name);
    for suffix in &candidates {
        if CONFLICTING_SUFFIXES.contains(&suffix.as_str()) {
            return resolve_conflict(suffix, parser, signature.unwrap_or_default());
        }
        if let Some(format) = formats::by_suffix(suffix) {
            // SQL keeps the treatment it already has: source when a parser
            // indexes it, a dump otherwise.
            if format.id == "sql" {
                if let Some(language) = parser {
                    return Classification::new(Category::Source, language, Evidence::Parser)
                        .noted("SQL indexed as source code".to_string());
                }
            }
            return Classification::new(format.category, format.id, Evidence::Extension);
        }
    }
    if let Some(language) = parser {
        return Classification::new(Category::Source, language, Evidence::Parser);
    }
    match candidates.last() {
        Some(extension) => Classification::new(Category::Other, "", Evidence::Unknown)
            .noted(format!("unregistered extension .{extension}")),
        None if NAMED_DOCUMENTS.contains(&file_name.to_lowercase().as_str()) => {
            Classification::new(Category::Document, "plain-text", Evidence::Name)
        }
        None => Classification::new(Category::Other, "", Evidence::Unknown)
            .noted("no extension".to_string()),
    }
}

const OLE2_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

fn ambiguous(note: String) -> Classification {
    Classification::new(Category::Other, "", Evidence::Ambiguous).noted(note)
}

fn resolve_conflict(suffix: &str, parser: Option<&str>, bytes: &[u8]) -> Classification {
    use Category as C;
    use Evidence::{Extension, Signature};
    match suffix {
        "ts" | "mts" => {
            if is_mpeg_ts(bytes) {
                Classification::new(C::Video, "mpeg-ts", Signature)
            } else if let Some(language) = parser {
                Classification::new(C::Source, language, Evidence::Parser)
            } else {
                ambiguous(format!(
                    ".{suffix} is TypeScript or an MPEG-TS video: no parser claims it and no MPEG-TS sync bytes"
                ))
            }
        }
        "dot" => {
            if bytes.starts_with(OLE2_MAGIC) {
                Classification::new(C::Document, "dot", Signature)
            } else if is_graphviz(bytes) {
                Classification::new(C::Vector, "graphviz", Signature)
            } else {
                ambiguous(
                    ".dot is a Word template or a Graphviz graph: neither signature found".into(),
                )
            }
        }
        "ttc" => {
            if bytes.starts_with(b"ttcf") {
                Classification::new(C::Font, "ttc", Signature)
            } else {
                ambiguous(".ttc without the ttcf font-collection signature".into())
            }
        }
        "obj" => {
            if bytes.is_empty() {
                ambiguous(".obj is a Wavefront mesh or a COFF object: empty file".into())
            } else if bytes.contains(&0) {
                Classification::new(C::Binary, "coff-object", Signature)
            } else {
                Classification::new(C::Model3d, "wavefront-obj", Signature)
            }
        }
        "key" => {
            if bytes.starts_with(ZIP_MAGIC) {
                Classification::new(C::Presentation, "keynote", Signature)
            } else if bytes.starts_with(b"-----BEGIN") {
                Classification::new(C::Other, "pem", Signature)
            } else {
                ambiguous(".key is a Keynote deck or a key file: neither signature found".into())
            }
        }
        "db" => {
            if bytes.starts_with(b"SQLite format 3\0") {
                Classification::new(C::Database, "sqlite", Signature)
            } else {
                Classification::new(C::Database, "db", Evidence::Ambiguous)
                    .noted(".db without a SQLite signature: engine unknown".into())
            }
        }
        "tif" | "tiff" => match tiff_kind(bytes) {
            Some(true) => Classification::new(C::Geospatial, "geotiff", Signature),
            Some(false) => Classification::new(C::Image, "tiff", Signature),
            None => Classification::new(C::Image, "tiff", Extension)
                .noted("no TIFF header: GeoTIFF tags not checked".into()),
        },
        "fig" => {
            if bytes.starts_with(b"#FIG") {
                Classification::new(C::Vector, "xfig", Signature)
            } else if bytes.starts_with(b"fig-kiwi") {
                Classification::new(C::Design, "figma", Signature)
            } else {
                Classification::new(C::Design, "figma", Extension)
                    .noted("no Figma or XFig signature".into())
            }
        }
        _ => ambiguous(format!("conflicting suffix .{suffix} has no resolver")),
    }
}

/// MPEG-TS (188-byte packets) or AVCHD/M2TS (4-byte timestamp + 188): two
/// sync bytes one packet apart.
fn is_mpeg_ts(bytes: &[u8]) -> bool {
    let synced = |first: usize, stride: usize| {
        bytes.get(first) == Some(&0x47) && bytes.get(first + stride) == Some(&0x47)
    };
    synced(0, 188) || synced(4, 192)
}

fn is_graphviz(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let text = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    ["digraph", "graph", "strict"]
        .iter()
        .any(|keyword| text.starts_with(keyword))
}

/// `Some(true)` for a TIFF whose first directory has a GeoTIFF tag,
/// `Some(false)` for any other TIFF, `None` without a TIFF header. Reads
/// only `bytes`, so a directory past the prefix counts as plain TIFF.
fn tiff_kind(bytes: &[u8]) -> Option<bool> {
    let little = match bytes.get(..4)? {
        [b'I', b'I', 42, 0] => true,
        [b'M', b'M', 0, 42] => false,
        [b'I', b'I', 43, 0] | [b'M', b'M', 0, 43] => return Some(false), // BigTIFF
        _ => return None,
    };
    let u16_at = |at: usize| {
        let b: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let u32_at = |at: usize| {
        let b: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    let Some(directory) = u32_at(4).and_then(|o| usize::try_from(o).ok()) else {
        return Some(false);
    };
    let count = usize::from(u16_at(directory).unwrap_or(0));
    // ModelPixelScale, ModelTiepoint, ModelTransformation, GeoKeyDirectory.
    const GEO_TAGS: [u16; 4] = [33550, 33922, 34264, 34735];
    let geo = (0..count)
        .map_while(|i| u16_at(directory + 2 + 12 * i))
        .any(|tag| GEO_TAGS.contains(&tag));
    Some(geo)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn class(
        name: &str,
        parser: Option<&str>,
        bytes: &[u8],
    ) -> (&'static str, String, &'static str) {
        let c = classify(name, parser, Some(bytes));
        (c.category.as_str(), c.format, c.evidence.as_str())
    }

    #[test]
    fn extension_case_and_aliases_give_one_canonical_format() {
        assert_eq!(
            class("photo.JPG", None, b""),
            ("image", "jpeg".into(), "extension")
        );
        assert_eq!(
            class("photo.jpeg", None, b""),
            ("image", "jpeg".into(), "extension")
        );
        assert_eq!(
            class("scan.JfIf", None, b""),
            ("image", "jpeg".into(), "extension")
        );
    }

    #[test]
    fn a_compound_suffix_wins_over_its_last_part() {
        assert_eq!(class("dist.tar.gz", None, b"").1, "tar.gz");
        assert_eq!(class("dist.TGZ", None, b"").1, "tar.gz");
        assert_eq!(class("notes.gz", None, b"").1, "gzip");
        assert_eq!(class("flow.drawio.svg", None, b"").1, "drawio");
        assert_eq!(class("libfoo.so.1.2", None, b"").1, "shared-library");
        assert_eq!(class("my.photo.png", None, b"").1, "png");
    }

    #[test]
    fn typescript_is_not_mpeg_ts_without_sync_bytes() {
        let ts = b"export const x = 1;\n";
        assert_eq!(
            class("app.ts", Some("typescript"), ts),
            ("source", "typescript".into(), "parser")
        );
        let packets: Vec<u8> = (0..2)
            .flat_map(|_| {
                let mut p = vec![0x47, 0x40, 0, 0x10];
                p.resize(188, 0xff);
                p
            })
            .collect();
        assert_eq!(
            class("broadcast.ts", Some("typescript"), &packets),
            ("video", "mpeg-ts".into(), "signature")
        );
        let mut avchd = vec![0, 0, 0, 0];
        avchd.extend(&packets[..188]);
        avchd.extend([0, 0, 0, 0, 0x47]);
        assert_eq!(class("00001.MTS", None, &avchd).1, "mpeg-ts");
        // A single leading `G` is not a stream.
        assert_eq!(class("g.ts", Some("typescript"), b"G = 1").0, "source");
    }

    #[test]
    fn an_ambiguous_suffix_without_evidence_says_so() {
        let c = classify("x.ts", None, Some(b"plain"));
        assert_eq!(c.evidence, Evidence::Ambiguous);
        assert!(c.note.unwrap().contains("TypeScript or an MPEG-TS"));
        let c = classify("notes.ttc", None, Some(b"TOOL find_files"));
        assert_eq!(
            (c.category, c.evidence),
            (Category::Other, Evidence::Ambiguous)
        );
        let c = classify("other.db", None, Some(b"\0\x01"));
        assert_eq!((c.category, c.format.as_str()), (Category::Database, "db"));
        assert_eq!(c.evidence, Evidence::Ambiguous);
        assert_eq!(classify("x.dot", None, None).evidence, Evidence::Ambiguous);
    }

    #[test]
    fn signatures_settle_each_conflicting_suffix() {
        assert_eq!(class("t.dot", None, OLE2_MAGIC).1, "dot");
        assert_eq!(class("g.dot", None, b"  digraph G {}").1, "graphviz");
        assert_eq!(class("f.ttc", None, b"ttcf\0\x01").1, "ttc");
        assert_eq!(class("m.obj", None, b"v 0 0 0\n").1, "wavefront-obj");
        assert_eq!(class("m.obj", None, b"\x64\x86\x01\0").1, "coff-object");
        assert_eq!(class("deck.key", None, ZIP_MAGIC).1, "keynote");
        assert_eq!(class("tls.key", None, b"-----BEGIN PRIVATE KEY").1, "pem");
        assert_eq!(class("a.db", None, b"SQLite format 3\0").1, "sqlite");
        assert_eq!(class("u.fig", None, b"#FIG 3.2").1, "xfig");
        assert_eq!(class("u.fig", None, b"fig-kiwi").1, "figma");
    }

    fn tiff(tags: &[u16], little: bool) -> Vec<u8> {
        let mut out = if little {
            b"II*\0".to_vec()
        } else {
            b"MM\0*".to_vec()
        };
        let u16b = |v: u16| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let u32b = |v: u32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        out.extend(u32b(8));
        out.extend(u16b(tags.len() as u16));
        for tag in tags {
            out.extend(u16b(*tag));
            out.extend([0u8; 10]);
        }
        out
    }

    #[test]
    fn geotiff_is_told_from_tiff_by_its_tags() {
        assert_eq!(
            class("e.tif", None, &tiff(&[256, 33550], true)).1,
            "geotiff"
        );
        assert_eq!(
            class("e.TIFF", None, &tiff(&[256, 34735], false)).1,
            "geotiff"
        );
        assert_eq!(class("p.tif", None, &tiff(&[256, 257], true)).1, "tiff");
        assert_eq!(class("p.tif", None, b"").2, "extension");
        // A directory offset past the prefix is a plain TIFF, not a panic.
        assert_eq!(class("p.tif", None, b"II*\0\xff\xff\xff\xff").1, "tiff");
    }

    #[test]
    fn sql_follows_its_parser() {
        assert_eq!(class("dump.sql", None, b"").0, "database");
        assert_eq!(class("q.sql", Some("sql"), b"").0, "source");
    }

    #[test]
    fn unregistered_and_extensionless_files_stay_classified_as_other() {
        let c = classify("blob.xyz", None, None);
        assert_eq!(
            (c.category, c.evidence),
            (Category::Other, Evidence::Unknown)
        );
        assert_eq!(c.note.as_deref(), Some("unregistered extension .xyz"));
        assert_eq!(
            class("README", None, b""),
            ("document", "plain-text".into(), "name")
        );
        assert_eq!(class("Makefile.bak", None, b"").0, "other");
        assert_eq!(
            classify(".gitignore", None, None).note.as_deref(),
            Some("no extension")
        );
        assert_eq!(
            class("main.rs", Some("rust"), b""),
            ("source", "rust".into(), "parser")
        );
        // A registered format wins over a parser that also claims it.
        assert_eq!(class("logo.svg", Some("html"), b"").0, "vector");
    }

    #[test]
    fn only_conflicting_suffixes_need_a_signature() {
        assert!(needs_signature("a.ts"));
        assert!(needs_signature("A.TIF"));
        assert!(!needs_signature("a.png"));
        assert!(!needs_signature("README"));
    }

    /// A reader that counts what is read from it.
    struct Counting<'a>(&'a mut u64);

    impl Read for Counting<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(0x47);
            *self.0 += buf.len() as u64;
            Ok(buf.len())
        }
    }

    #[test]
    fn the_signature_read_is_bounded_for_an_endless_file() {
        let mut read = 0;
        let prefix = read_signature(Counting(&mut read)).unwrap_or_default();
        assert_eq!(prefix.len() as u64, SIGNATURE_BYTES);
        assert!(read <= SIGNATURE_BYTES, "read {read} bytes");
    }
}
