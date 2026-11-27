//! The format registry: every category, canonical format and extension
//! alias the path catalog knows, in one table (issue #629). A new format is
//! one [`Format`] line; a suffix whose meaning depends on the file's bytes
//! also gets an arm in `classify::resolve_conflict`.

/// What a catalogued file is for, as stored in `path_catalog.category`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Image,
    Vector,
    Video,
    Audio,
    Subtitle,
    Document,
    Spreadsheet,
    Presentation,
    Ebook,
    Font,
    Design,
    Model3d,
    Data,
    Database,
    Geospatial,
    Archive,
    Binary,
    Email,
    /// A file only a language parser claims (`.rs`, `.py`): its format is
    /// the parser's language id.
    Source,
    /// Walked but not classifiable with enough evidence; still findable by
    /// path and name.
    Other,
}

impl Category {
    /// Every category, in the order summaries list them.
    pub const ALL: [Category; 20] = [
        Category::Image,
        Category::Vector,
        Category::Video,
        Category::Audio,
        Category::Subtitle,
        Category::Document,
        Category::Spreadsheet,
        Category::Presentation,
        Category::Ebook,
        Category::Font,
        Category::Design,
        Category::Model3d,
        Category::Data,
        Category::Database,
        Category::Geospatial,
        Category::Archive,
        Category::Binary,
        Category::Email,
        Category::Source,
        Category::Other,
    ];

    /// The stored and queried name.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Image => "image",
            Category::Vector => "vector",
            Category::Video => "video",
            Category::Audio => "audio",
            Category::Subtitle => "subtitle",
            Category::Document => "document",
            Category::Spreadsheet => "spreadsheet",
            Category::Presentation => "presentation",
            Category::Ebook => "ebook",
            Category::Font => "font",
            Category::Design => "design",
            Category::Model3d => "model3d",
            Category::Data => "data",
            Category::Database => "database",
            Category::Geospatial => "geospatial",
            Category::Archive => "archive",
            Category::Binary => "binary",
            Category::Email => "email",
            Category::Source => "source",
            Category::Other => "other",
        }
    }

    /// The category named `name`, case-insensitively.
    pub fn parse(name: &str) -> Option<Category> {
        let name = name.trim();
        Category::ALL
            .into_iter()
            .find(|c| c.as_str().eq_ignore_ascii_case(name))
    }
}

/// One canonical format: its id (stored in `path_catalog.format`), its
/// category, and every file suffix that names it (lowercase, no leading
/// dot; a compound suffix such as `tar.gz` is matched before `gz`).
#[derive(Debug, Clone, Copy)]
pub struct Format {
    pub id: &'static str,
    pub category: Category,
    pub suffixes: &'static [&'static str],
}

const fn f(id: &'static str, category: Category, suffixes: &'static [&'static str]) -> Format {
    Format {
        id,
        category,
        suffixes,
    }
}

use Category as C;

/// Every registered format. A suffix appears in at most one entry (a unit
/// test checks it); a suffix with two meanings is in [`CONFLICTING_SUFFIXES`]
/// instead and resolved from a bounded signature read.
pub const FORMATS: &[Format] = &[
    // Raster images.
    f("png", C::Image, &["png", "apng"]),
    f(
        "jpeg",
        C::Image,
        &["jpeg", "jpg", "jpe", "jfif", "pjpeg", "pjp"],
    ),
    f("gif", C::Image, &["gif"]),
    f("webp", C::Image, &["webp"]),
    f("avif", C::Image, &["avif"]),
    f("heif", C::Image, &["heic", "heif", "heics", "heifs", "hif"]),
    f("bmp", C::Image, &["bmp", "dib"]),
    f("ico", C::Image, &["ico"]),
    f("cur", C::Image, &["cur", "ani"]),
    f(
        "jpeg2000",
        C::Image,
        &["jp2", "j2k", "jpf", "jpx", "jpm", "j2c"],
    ),
    f("jpegxl", C::Image, &["jxl"]),
    f("tga", C::Image, &["tga", "icb", "vda"]),
    f("dds", C::Image, &["dds"]),
    f("ktx", C::Image, &["ktx", "ktx2"]),
    f("openexr", C::Image, &["exr"]),
    f("radiance-hdr", C::Image, &["hdr", "rgbe"]),
    f("qoi", C::Image, &["qoi"]),
    f("pnm", C::Image, &["pbm", "pgm", "ppm", "pnm", "pam"]),
    f("dng", C::Image, &["dng"]),
    f("canon-raw", C::Image, &["cr2", "cr3", "crw"]),
    f("nikon-raw", C::Image, &["nef", "nrw"]),
    f("sony-raw", C::Image, &["arw", "srf", "sr2"]),
    f("fujifilm-raw", C::Image, &["raf"]),
    f("olympus-raw", C::Image, &["orf"]),
    f("panasonic-raw", C::Image, &["rw2"]),
    f("pentax-raw", C::Image, &["pef"]),
    // Vector images and diagrams.
    f("svg", C::Vector, &["svg"]),
    f("svgz", C::Vector, &["svgz"]),
    f("eps", C::Vector, &["eps", "epsf", "epsi"]),
    f("postscript", C::Vector, &["ps"]),
    f("illustrator", C::Vector, &["ai"]),
    f("coreldraw", C::Vector, &["cdr"]),
    f(
        "visio",
        C::Vector,
        &["vsd", "vsdx", "vsdm", "vss", "vssx", "vst", "vstx"],
    ),
    f(
        "drawio",
        C::Vector,
        &["drawio", "dio", "drawio.svg", "drawio.png"],
    ),
    f("emf", C::Vector, &["emf", "wmf"]),
    // Video.
    f("mp4", C::Video, &["mp4", "m4p"]),
    f("m4v", C::Video, &["m4v"]),
    f("quicktime", C::Video, &["mov", "qt"]),
    f("matroska", C::Video, &["mkv", "mk3d"]),
    f("webm", C::Video, &["webm"]),
    f("avi", C::Video, &["avi"]),
    f(
        "mpeg",
        C::Video,
        &["mpeg", "mpg", "mpe", "m1v", "m2v", "vob"],
    ),
    f("wmv", C::Video, &["wmv", "asf"]),
    f("flv", C::Video, &["flv", "f4v"]),
    f("ogv", C::Video, &["ogv"]),
    f("3gp", C::Video, &["3gp", "3g2"]),
    f("m2ts", C::Video, &["m2ts", "m2t"]),
    // Audio.
    f("mp3", C::Audio, &["mp3"]),
    f("wav", C::Audio, &["wav", "wave"]),
    f("flac", C::Audio, &["flac"]),
    f("aac", C::Audio, &["aac"]),
    f("m4a", C::Audio, &["m4a", "m4b"]),
    f("alac", C::Audio, &["alac"]),
    f("caf", C::Audio, &["caf"]),
    f("ogg", C::Audio, &["ogg", "oga"]),
    f("opus", C::Audio, &["opus"]),
    f("aiff", C::Audio, &["aiff", "aif", "aifc"]),
    f("wma", C::Audio, &["wma"]),
    f("amr", C::Audio, &["amr"]),
    f("ac3", C::Audio, &["ac3", "eac3"]),
    f("midi", C::Audio, &["mid", "midi", "kar"]),
    f("ape", C::Audio, &["ape"]),
    f("dsd", C::Audio, &["dsf", "dff"]),
    // Subtitles and transcripts.
    f("srt", C::Subtitle, &["srt"]),
    f("webvtt", C::Subtitle, &["vtt"]),
    f("ass", C::Subtitle, &["ass"]),
    f("ssa", C::Subtitle, &["ssa"]),
    f("ttml", C::Subtitle, &["ttml", "dfxp"]),
    f("lrc", C::Subtitle, &["lrc"]),
    f("sbv", C::Subtitle, &["sbv"]),
    f("microdvd", C::Subtitle, &["sub"]),
    // PDF and text documents.
    f("pdf", C::Document, &["pdf"]),
    f("doc", C::Document, &["doc"]),
    f("docx", C::Document, &["docx"]),
    f("docm", C::Document, &["docm"]),
    f("dotx", C::Document, &["dotx", "dotm"]),
    f("odt", C::Document, &["odt", "ott", "fodt"]),
    f("rtf", C::Document, &["rtf"]),
    f("pages", C::Document, &["pages"]),
    f("wordperfect", C::Document, &["wpd"]),
    f("plain-text", C::Document, &["txt", "text"]),
    // Spreadsheets.
    f("xls", C::Spreadsheet, &["xls"]),
    f("xlsx", C::Spreadsheet, &["xlsx"]),
    f("xlsm", C::Spreadsheet, &["xlsm"]),
    f("xlsb", C::Spreadsheet, &["xlsb"]),
    f("xlt", C::Spreadsheet, &["xlt"]),
    f("xltx", C::Spreadsheet, &["xltx", "xltm"]),
    f("ods", C::Spreadsheet, &["ods", "ots", "fods"]),
    f("numbers", C::Spreadsheet, &["numbers"]),
    f("csv", C::Spreadsheet, &["csv"]),
    f("tsv", C::Spreadsheet, &["tsv", "tab"]),
    // Presentations.
    f("ppt", C::Presentation, &["ppt"]),
    f("pptx", C::Presentation, &["pptx"]),
    f("pptm", C::Presentation, &["pptm"]),
    f("pps", C::Presentation, &["pps"]),
    f("ppsx", C::Presentation, &["ppsx", "ppsm"]),
    f("pot", C::Presentation, &["pot"]),
    f("potx", C::Presentation, &["potx", "potm"]),
    f("odp", C::Presentation, &["odp", "otp", "fodp"]),
    f("keynote", C::Presentation, &["keynote"]),
    // Publications and comics.
    f("epub", C::Ebook, &["epub"]),
    f("mobi", C::Ebook, &["mobi", "prc"]),
    f("kindle", C::Ebook, &["azw", "azw3", "kfx"]),
    f("fictionbook", C::Ebook, &["fb2", "fb2.zip"]),
    f("djvu", C::Ebook, &["djvu", "djv"]),
    f("comic-zip", C::Ebook, &["cbz"]),
    f("comic-rar", C::Ebook, &["cbr"]),
    f("comic-7z", C::Ebook, &["cb7"]),
    // Fonts.
    f("truetype", C::Font, &["ttf"]),
    f("opentype", C::Font, &["otf"]),
    f("opentype-collection", C::Font, &["otc"]),
    f("woff", C::Font, &["woff"]),
    f("woff2", C::Font, &["woff2"]),
    f("eot", C::Font, &["eot"]),
    f("type1", C::Font, &["pfb", "pfm", "afm"]),
    // Graphic and creative design.
    f("photoshop", C::Design, &["psd", "psb"]),
    f("sketch", C::Design, &["sketch"]),
    f("adobe-xd", C::Design, &["xd"]),
    f("gimp", C::Design, &["xcf"]),
    f("krita", C::Design, &["kra"]),
    f("after-effects", C::Design, &["aep", "aet"]),
    f("indesign", C::Design, &["indd", "indt", "idml"]),
    f("affinity", C::Design, &["afdesign", "afphoto", "afpub"]),
    f("premiere", C::Design, &["prproj"]),
    // 3D models and CAD.
    f("blender", C::Model3d, &["blend", "blend1"]),
    f("gltf", C::Model3d, &["gltf"]),
    f("glb", C::Model3d, &["glb", "vrm"]),
    f("fbx", C::Model3d, &["fbx"]),
    f("stl", C::Model3d, &["stl"]),
    f("usd", C::Model3d, &["usd", "usda", "usdc", "usdz"]),
    f("3mf", C::Model3d, &["3mf"]),
    f("collada", C::Model3d, &["dae"]),
    f("3ds", C::Model3d, &["3ds", "max"]),
    f("ply", C::Model3d, &["ply"]),
    f("alembic", C::Model3d, &["abc"]),
    f("wavefront-mtl", C::Model3d, &["mtl"]),
    f("step", C::Model3d, &["step", "stp", "p21"]),
    f("iges", C::Model3d, &["iges", "igs"]),
    f("dwg", C::Model3d, &["dwg"]),
    f("dxf", C::Model3d, &["dxf"]),
    f("solidworks", C::Model3d, &["sldprt", "sldasm", "slddrw"]),
    f("ifc", C::Model3d, &["ifc"]),
    // Structured data and notebooks.
    f("json", C::Data, &["json", "jsonc", "json5"]),
    f("json-lines", C::Data, &["jsonl", "ndjson"]),
    f(
        "xml",
        C::Data,
        &["xml", "xsd", "xsl", "xslt", "rss", "atom", "plist"],
    ),
    f("yaml", C::Data, &["yaml", "yml"]),
    f("toml", C::Data, &["toml"]),
    f("parquet", C::Data, &["parquet"]),
    f("arrow", C::Data, &["arrow", "feather", "ipc"]),
    f("avro", C::Data, &["avro"]),
    f("orc", C::Data, &["orc"]),
    f("jupyter-notebook", C::Data, &["ipynb"]),
    f("protobuf-binary", C::Data, &["pb", "binpb"]),
    f("hdf5", C::Data, &["h5", "hdf5", "hdf"]),
    f("numpy", C::Data, &["npy", "npz"]),
    f("ini", C::Data, &["ini", "cfg", "conf"]),
    // Databases and dumps.
    f(
        "sqlite",
        C::Database,
        &["sqlite", "sqlite3", "db3", "s3db", "sl3"],
    ),
    f("access", C::Database, &["mdb", "accdb"]),
    f("duckdb", C::Database, &["duckdb", "ddb"]),
    f("sql", C::Database, &["sql", "dump"]),
    // Geospatial data.
    f("geojson", C::Geospatial, &["geojson"]),
    f("kml", C::Geospatial, &["kml"]),
    f("kmz", C::Geospatial, &["kmz"]),
    f("gpx", C::Geospatial, &["gpx"]),
    f(
        "shapefile",
        C::Geospatial,
        &["shp", "shx", "dbf", "prj", "cpg"],
    ),
    f("geopackage", C::Geospatial, &["gpkg"]),
    f("topojson", C::Geospatial, &["topojson"]),
    f("mbtiles", C::Geospatial, &["mbtiles", "pmtiles"]),
    // Archives and disk images.
    f("zip", C::Archive, &["zip", "zipx"]),
    f("7z", C::Archive, &["7z"]),
    f("rar", C::Archive, &["rar"]),
    f("tar", C::Archive, &["tar"]),
    f("tar.gz", C::Archive, &["tar.gz", "tgz", "taz"]),
    f("tar.bz2", C::Archive, &["tar.bz2", "tbz", "tbz2", "tb2"]),
    f("tar.xz", C::Archive, &["tar.xz", "txz"]),
    f("tar.zst", C::Archive, &["tar.zst", "tzst"]),
    f("tar.lz", C::Archive, &["tar.lz", "tlz"]),
    f("gzip", C::Archive, &["gz"]),
    f("bzip2", C::Archive, &["bz2"]),
    f("xz", C::Archive, &["xz"]),
    f("zstd", C::Archive, &["zst"]),
    f("lz4", C::Archive, &["lz4"]),
    f("cab", C::Archive, &["cab"]),
    f("iso", C::Archive, &["iso"]),
    f("dmg", C::Archive, &["dmg"]),
    f(
        "virtual-disk",
        C::Archive,
        &["vhd", "vhdx", "vmdk", "qcow2", "img"],
    ),
    // Binaries, libraries and installers.
    f("windows-executable", C::Binary, &["exe", "com", "scr"]),
    f("windows-library", C::Binary, &["dll", "sys"]),
    f(
        "windows-installer",
        C::Binary,
        &["msi", "msix", "appx", "msp"],
    ),
    f("shared-library", C::Binary, &["so"]),
    f("macos-library", C::Binary, &["dylib", "bundle"]),
    f("static-library", C::Binary, &["a", "lib"]),
    f("debian-package", C::Binary, &["deb", "udeb"]),
    f("rpm-package", C::Binary, &["rpm"]),
    f("android-package", C::Binary, &["apk", "aab", "xapk"]),
    f("ios-package", C::Binary, &["ipa"]),
    f("macos-package", C::Binary, &["pkg"]),
    f("java-archive", C::Binary, &["jar", "war", "ear", "aar"]),
    f("java-class", C::Binary, &["class"]),
    f("webassembly", C::Binary, &["wasm"]),
    f("python-bytecode", C::Binary, &["pyc", "pyo"]),
    f("python-wheel", C::Binary, &["whl", "egg"]),
    f("appimage", C::Binary, &["appimage"]),
    f("snap", C::Binary, &["snap", "flatpak"]),
    // Mail and calendar.
    f("eml", C::Email, &["eml"]),
    f("outlook-msg", C::Email, &["msg"]),
    f("mbox", C::Email, &["mbox", "mbx"]),
    f("icalendar", C::Email, &["ics", "ical", "icalendar", "ifb"]),
    f("vcard", C::Email, &["vcf", "vcard"]),
];

/// Suffixes with two common meanings, resolved by `classify` from the parser
/// that claims the file and a bounded signature read, never by extension
/// alone. Each keeps its own formats (ids not in [`FORMATS`]).
pub const CONFLICTING_SUFFIXES: &[&str] = &[
    "ts",   // TypeScript or an MPEG transport stream.
    "mts",  // TypeScript module or an AVCHD video.
    "dot",  // Word 97-2003 template or a Graphviz graph.
    "ttc",  // TrueType font collection or any other `.ttc` text.
    "obj",  // Wavefront 3D mesh (text) or a compiled COFF object file.
    "key",  // Keynote presentation (a ZIP package) or a PEM key.
    "db",   // A SQLite database or some other `.db` file.
    "tif",  // A plain TIFF or a GeoTIFF raster.
    "tiff", // A plain TIFF or a GeoTIFF raster.
    "fig",  // A Figma file or an XFig drawing.
];

/// Formats only [`CONFLICTING_SUFFIXES`] resolve to: `(id, category)`, so a
/// query can name them too.
pub const CONFLICT_FORMATS: &[(&str, Category)] = &[
    ("mpeg-ts", Category::Video),
    ("dot", Category::Document),
    ("graphviz", Category::Vector),
    ("ttc", Category::Font),
    ("wavefront-obj", Category::Model3d),
    ("coff-object", Category::Binary),
    ("keynote", Category::Presentation),
    ("pem", Category::Other),
    ("sqlite", Category::Database),
    ("db", Category::Database),
    ("tiff", Category::Image),
    ("geotiff", Category::Geospatial),
    ("figma", Category::Design),
    ("xfig", Category::Vector),
];

/// File names without an extension that are documents by convention.
pub const NAMED_DOCUMENTS: &[&str] = &[
    "readme",
    "license",
    "licence",
    "copying",
    "authors",
    "contributors",
    "notice",
    "changelog",
    "changes",
    "history",
    "news",
    "todo",
];

/// The format registered for `suffix` (lowercase, no dot), if any.
pub fn by_suffix(suffix: &str) -> Option<&'static Format> {
    FORMATS.iter().find(|f| f.suffixes.contains(&suffix))
}

/// The canonical format id a query's `alias` names: a format id or any of
/// its suffixes, case-insensitively, with an optional leading dot
/// (`JPG`, `.jpg` and `jpeg` all give `jpeg`).
pub fn canonical_format(alias: &str) -> Option<&'static str> {
    let alias = alias.trim().trim_start_matches('.').to_ascii_lowercase();
    if let Some(format) = FORMATS
        .iter()
        .find(|f| f.id == alias || f.suffixes.contains(&alias.as_str()))
    {
        return Some(format.id);
    }
    // A conflicting suffix as a query alias names its non-source reading.
    let suffix_reading = match alias.as_str() {
        "ts" | "mts" => Some("mpeg-ts"),
        "tif" => Some("tiff"),
        "obj" => Some("wavefront-obj"),
        "fig" => Some("figma"),
        _ => None,
    };
    CONFLICT_FORMATS
        .iter()
        .map(|(id, _)| *id)
        .find(|id| *id == alias)
        .or(suffix_reading)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn each_suffix_belongs_to_one_format_and_never_to_a_conflict() {
        let mut seen = HashSet::new();
        for format in FORMATS {
            for suffix in format.suffixes {
                assert_eq!(*suffix, suffix.to_ascii_lowercase(), "{suffix}");
                assert!(!suffix.starts_with('.'), "{suffix}");
                assert!(seen.insert(*suffix), "suffix `{suffix}` registered twice");
                assert!(
                    !CONFLICTING_SUFFIXES.contains(suffix),
                    "`{suffix}` is both a plain and a conflicting suffix"
                );
            }
        }
    }

    #[test]
    fn format_ids_are_unique_across_both_tables() {
        let mut ids = HashSet::new();
        for id in FORMATS.iter().map(|f| f.id) {
            assert!(ids.insert(id), "format id `{id}` registered twice");
        }
        for (id, category) in CONFLICT_FORMATS {
            if let Some(plain) = FORMATS.iter().find(|f| f.id == *id) {
                assert_eq!(plain.category, *category, "{id}");
            } else {
                assert!(ids.insert(id), "conflict format id `{id}` twice");
            }
        }
    }

    #[test]
    fn every_category_round_trips_through_its_name() {
        for category in Category::ALL {
            assert_eq!(Category::parse(category.as_str()), Some(category));
        }
        assert_eq!(Category::parse(" AUDIO "), Some(Category::Audio));
        assert_eq!(Category::parse("audios"), None);
    }

    #[test]
    fn aliases_normalize_to_one_canonical_format() {
        for alias in ["jpg", "JPG", ".jpeg", "jfif", "jpeg"] {
            assert_eq!(canonical_format(alias), Some("jpeg"), "{alias}");
        }
        assert_eq!(canonical_format("tgz"), Some("tar.gz"));
        assert_eq!(canonical_format("TAR.GZ"), Some("tar.gz"));
        assert_eq!(canonical_format("ts"), Some("mpeg-ts"));
        assert_eq!(canonical_format("geotiff"), Some("geotiff"));
        assert_eq!(canonical_format("nope"), None);
    }
}
