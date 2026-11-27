# Path catalog formats

The path catalog ([ADR-052](../01-architecture/adrs/052-path-catalog.md), issue #629) gives every file the index walks one row: its path, category, format, size and the evidence for the classification. `find_files` (MCP) and `mct-cli files` query it. The catalog stores metadata only: it never extracts text, decodes media or opens archives.

The registry lives in `crates/mct-index/src/catalog/formats.rs`. The classifier is `crates/mct-index/src/catalog/classify.rs`.

## How a file is classified

1. The file name is split into suffixes, longest first: `bundle.tar.gz` tries `tar.gz`, then `gz`. A versioned shared library such as `libz.so.1.2` counts as `.so`. Matching ignores case: `.JPG`, `.Jpeg` and `.jpg` give the same format, and the stored path keeps its spelling.
2. A suffix in `CONFLICTING_SUFFIXES` is resolved from the parser that claims the file and from at most 4096 bytes read from its start (`SIGNATURE_BYTES`). No other file is opened.
3. A registered suffix gives its format, with evidence `extension`. A registered format wins over a parser that also claims the suffix: `.svg` stays `vector` when a markup parser indexes it.
4. A file only a parser claims is category `source`. Its format is the parser's language id (`rust`, `javascript_typescript`) and its evidence is `parser`. SQL is `source` when a parser claims it, and a `database` dump (`sql`) otherwise.
5. A name without an extension in `NAMED_DOCUMENTS` (`README`, `LICENSE`, `CHANGELOG`, ...) is a `document` in `plain-text`, with evidence `name`.
6. Anything else is category `other` with an empty format and evidence `unknown`. Its note says why: `unregistered extension .xyz` or `no extension`. It stays findable by path and query.

## Doubtful files

| Suffix | Readings | How it is decided |
|---|---|---|
| `.ts`, `.mts` | TypeScript source, MPEG-TS / AVCHD video | Transport-stream sync bytes (`0x47` at 0 and 188, or at 4 and 196 for M2TS) give `video/mpeg-ts`. Otherwise a parser that claims it makes it `source`; with no parser it is `ambiguous`. |
| `.dot` | Word 97-2003 template, Graphviz graph | The OLE2 signature gives `document/dot`; `graph`/`digraph` text gives `vector/graphviz`; neither is `ambiguous`. |
| `.ttc` | TrueType collection, text | `ttcf` gives `font/ttc`; anything else is `ambiguous`. |
| `.obj` | Wavefront mesh, COFF object | NUL bytes give `binary/coff-object`; text gives `model3d/wavefront-obj`; an empty file is `ambiguous`. |
| `.key` | Keynote, PEM key | `PK` gives `presentation/keynote`; `-----BEGIN` gives `other/pem`; neither is `ambiguous`. The default exclusions skip `*.key`, so neither is catalogued unless `.mctignore` re-includes it. |
| `.db` | SQLite, other | `SQLite format 3` gives `database/sqlite`; anything else is `database/db` with evidence `ambiguous` (engine unknown). |
| `.tif`, `.tiff` | TIFF, GeoTIFF | A GeoTIFF tag (33550, 33922, 34264 or 34735) in the first IFD gives `geospatial/geotiff`; otherwise `image/tiff`. |
| `.fig` | Figma, XFig | `fig-kiwi` gives `design/figma`; `#FIG` gives `vector/xfig`; neither is `design/figma` by extension, with a note. |

When the signals do not settle it, the row has evidence `ambiguous` and a note naming the readings; it is category `other` unless both readings share one (`.db`). Some settled rows carry a note too (SQL indexed as source, a `.tif` without a TIFF header). `find_files` adds a `note` column, `evidence: note`, when a returned row has one. Example: `fonts/notes.ttc` without the `ttcf` signature is `other` with "`.ttc` without the ttcf font-collection signature".

## Query aliases

`find_files`' `format` accepts a format id or any of its suffixes, in any case, with or without the leading dot: `jpg`, `.JPG` and `jpe` all mean `jpeg`; `tgz` means `tar.gz`; `xls` and `.XLS` mean `xls`. A conflicting suffix names its non-source reading: `ts` and `mts` mean `mpeg-ts`, `tif` means `tiff`, `obj` means `wavefront-obj`, `fig` means `figma`. A value the registry does not know is accepted only if some catalogued row has it as its format (a parser language id). Otherwise the call is rejected and names the valid forms. `category` accepts the 20 names below, in any case.

## Categories

`image`, `vector`, `video`, `audio`, `subtitle`, `document`, `spreadsheet`, `presentation`, `ebook`, `font`, `design`, `model3d`, `data`, `database`, `geospatial`, `archive`, `binary`, `email`, `source`, `other`.

## How an existing index updates

Migration 34 adds an empty `path_catalog` table and invalidates nothing. The next ordinary `reindex`, or the watcher's first full walk, fills it without `--force`. After that a row is rewritten only when the file's size, modification time or the rules fingerprint changes. The fingerprint is `CATALOG_RULES_VERSION` plus a hash of the extensions the language registry parses, so an unchanged tree writes nothing. A full reindex drops rows for paths it did not see, except under a path it could not read. Watcher updates drop deleted paths and the old end of a rename.

## How to add a format

1. Add an `f("<id>", C::<Category>, &["<suffix>", ...])` line to `FORMATS` in `formats.rs`, in its family's block. The id is lowercase; suffixes are lowercase, without the dot. A compound suffix (`tar.zst`) is matched before its last part.
2. A suffix may belong to one format only (unit test `each_suffix_belongs_to_one_format_and_never_to_a_conflict`). If the new suffix already has another common meaning, add it to `CONFLICTING_SUFFIXES` instead, add its formats to `CONFLICT_FORMATS`, and resolve it in `resolve_conflict` in `classify.rs` from the parser and the signature bytes.
3. Bump `CATALOG_RULES_VERSION` in `store.rs`, so existing rows are reclassified once on the next reindex.
4. Add a tiny sample with real magic bytes to `crates/mct-index/tests/fixtures/path-catalog/` and an expected row to `every_family_is_classified` in `crates/mct-index/tests/path_catalog.rs`.
5. Regenerate the table below.

## Registered formats

| Category | Format id | Suffixes |
|---|---|---|
| `image` | `png` | `.png`, `.apng` |
| `image` | `jpeg` | `.jpeg`, `.jpg`, `.jpe`, `.jfif`, `.pjpeg`, `.pjp` |
| `image` | `gif` | `.gif` |
| `image` | `webp` | `.webp` |
| `image` | `avif` | `.avif` |
| `image` | `heif` | `.heic`, `.heif`, `.heics`, `.heifs`, `.hif` |
| `image` | `bmp` | `.bmp`, `.dib` |
| `image` | `ico` | `.ico` |
| `image` | `cur` | `.cur`, `.ani` |
| `image` | `jpeg2000` | `.jp2`, `.j2k`, `.jpf`, `.jpx`, `.jpm`, `.j2c` |
| `image` | `jpegxl` | `.jxl` |
| `image` | `tga` | `.tga`, `.icb`, `.vda` |
| `image` | `dds` | `.dds` |
| `image` | `ktx` | `.ktx`, `.ktx2` |
| `image` | `openexr` | `.exr` |
| `image` | `radiance-hdr` | `.hdr`, `.rgbe` |
| `image` | `qoi` | `.qoi` |
| `image` | `pnm` | `.pbm`, `.pgm`, `.ppm`, `.pnm`, `.pam` |
| `image` | `dng` | `.dng` |
| `image` | `canon-raw` | `.cr2`, `.cr3`, `.crw` |
| `image` | `nikon-raw` | `.nef`, `.nrw` |
| `image` | `sony-raw` | `.arw`, `.srf`, `.sr2` |
| `image` | `fujifilm-raw` | `.raf` |
| `image` | `olympus-raw` | `.orf` |
| `image` | `panasonic-raw` | `.rw2` |
| `image` | `pentax-raw` | `.pef` |
| `vector` | `svg` | `.svg` |
| `vector` | `svgz` | `.svgz` |
| `vector` | `eps` | `.eps`, `.epsf`, `.epsi` |
| `vector` | `postscript` | `.ps` |
| `vector` | `illustrator` | `.ai` |
| `vector` | `coreldraw` | `.cdr` |
| `vector` | `visio` | `.vsd`, `.vsdx`, `.vsdm`, `.vss`, `.vssx`, `.vst`, `.vstx` |
| `vector` | `drawio` | `.drawio`, `.dio`, `.drawio.svg`, `.drawio.png` |
| `vector` | `emf` | `.emf`, `.wmf` |
| `video` | `mp4` | `.mp4`, `.m4p` |
| `video` | `m4v` | `.m4v` |
| `video` | `quicktime` | `.mov`, `.qt` |
| `video` | `matroska` | `.mkv`, `.mk3d` |
| `video` | `webm` | `.webm` |
| `video` | `avi` | `.avi` |
| `video` | `mpeg` | `.mpeg`, `.mpg`, `.mpe`, `.m1v`, `.m2v`, `.vob` |
| `video` | `wmv` | `.wmv`, `.asf` |
| `video` | `flv` | `.flv`, `.f4v` |
| `video` | `ogv` | `.ogv` |
| `video` | `3gp` | `.3gp`, `.3g2` |
| `video` | `m2ts` | `.m2ts`, `.m2t` |
| `audio` | `mp3` | `.mp3` |
| `audio` | `wav` | `.wav`, `.wave` |
| `audio` | `flac` | `.flac` |
| `audio` | `aac` | `.aac` |
| `audio` | `m4a` | `.m4a`, `.m4b` |
| `audio` | `alac` | `.alac` |
| `audio` | `caf` | `.caf` |
| `audio` | `ogg` | `.ogg`, `.oga` |
| `audio` | `opus` | `.opus` |
| `audio` | `aiff` | `.aiff`, `.aif`, `.aifc` |
| `audio` | `wma` | `.wma` |
| `audio` | `amr` | `.amr` |
| `audio` | `ac3` | `.ac3`, `.eac3` |
| `audio` | `midi` | `.mid`, `.midi`, `.kar` |
| `audio` | `ape` | `.ape` |
| `audio` | `dsd` | `.dsf`, `.dff` |
| `subtitle` | `srt` | `.srt` |
| `subtitle` | `webvtt` | `.vtt` |
| `subtitle` | `ass` | `.ass` |
| `subtitle` | `ssa` | `.ssa` |
| `subtitle` | `ttml` | `.ttml`, `.dfxp` |
| `subtitle` | `lrc` | `.lrc` |
| `subtitle` | `sbv` | `.sbv` |
| `subtitle` | `microdvd` | `.sub` |
| `document` | `pdf` | `.pdf` |
| `document` | `doc` | `.doc` |
| `document` | `docx` | `.docx` |
| `document` | `docm` | `.docm` |
| `document` | `dotx` | `.dotx`, `.dotm` |
| `document` | `odt` | `.odt`, `.ott`, `.fodt` |
| `document` | `rtf` | `.rtf` |
| `document` | `pages` | `.pages` |
| `document` | `wordperfect` | `.wpd` |
| `document` | `plain-text` | `.txt`, `.text` |
| `spreadsheet` | `xls` | `.xls` |
| `spreadsheet` | `xlsx` | `.xlsx` |
| `spreadsheet` | `xlsm` | `.xlsm` |
| `spreadsheet` | `xlsb` | `.xlsb` |
| `spreadsheet` | `xlt` | `.xlt` |
| `spreadsheet` | `xltx` | `.xltx`, `.xltm` |
| `spreadsheet` | `ods` | `.ods`, `.ots`, `.fods` |
| `spreadsheet` | `numbers` | `.numbers` |
| `spreadsheet` | `csv` | `.csv` |
| `spreadsheet` | `tsv` | `.tsv`, `.tab` |
| `presentation` | `ppt` | `.ppt` |
| `presentation` | `pptx` | `.pptx` |
| `presentation` | `pptm` | `.pptm` |
| `presentation` | `pps` | `.pps` |
| `presentation` | `ppsx` | `.ppsx`, `.ppsm` |
| `presentation` | `pot` | `.pot` |
| `presentation` | `potx` | `.potx`, `.potm` |
| `presentation` | `odp` | `.odp`, `.otp`, `.fodp` |
| `presentation` | `keynote` | `.keynote` |
| `ebook` | `epub` | `.epub` |
| `ebook` | `mobi` | `.mobi`, `.prc` |
| `ebook` | `kindle` | `.azw`, `.azw3`, `.kfx` |
| `ebook` | `fictionbook` | `.fb2`, `.fb2.zip` |
| `ebook` | `djvu` | `.djvu`, `.djv` |
| `ebook` | `comic-zip` | `.cbz` |
| `ebook` | `comic-rar` | `.cbr` |
| `ebook` | `comic-7z` | `.cb7` |
| `font` | `truetype` | `.ttf` |
| `font` | `opentype` | `.otf` |
| `font` | `opentype-collection` | `.otc` |
| `font` | `woff` | `.woff` |
| `font` | `woff2` | `.woff2` |
| `font` | `eot` | `.eot` |
| `font` | `type1` | `.pfb`, `.pfm`, `.afm` |
| `design` | `photoshop` | `.psd`, `.psb` |
| `design` | `sketch` | `.sketch` |
| `design` | `adobe-xd` | `.xd` |
| `design` | `gimp` | `.xcf` |
| `design` | `krita` | `.kra` |
| `design` | `after-effects` | `.aep`, `.aet` |
| `design` | `indesign` | `.indd`, `.indt`, `.idml` |
| `design` | `affinity` | `.afdesign`, `.afphoto`, `.afpub` |
| `design` | `premiere` | `.prproj` |
| `model3d` | `blender` | `.blend`, `.blend1` |
| `model3d` | `gltf` | `.gltf` |
| `model3d` | `glb` | `.glb`, `.vrm` |
| `model3d` | `fbx` | `.fbx` |
| `model3d` | `stl` | `.stl` |
| `model3d` | `usd` | `.usd`, `.usda`, `.usdc`, `.usdz` |
| `model3d` | `3mf` | `.3mf` |
| `model3d` | `collada` | `.dae` |
| `model3d` | `3ds` | `.3ds`, `.max` |
| `model3d` | `ply` | `.ply` |
| `model3d` | `alembic` | `.abc` |
| `model3d` | `wavefront-mtl` | `.mtl` |
| `model3d` | `step` | `.step`, `.stp`, `.p21` |
| `model3d` | `iges` | `.iges`, `.igs` |
| `model3d` | `dwg` | `.dwg` |
| `model3d` | `dxf` | `.dxf` |
| `model3d` | `solidworks` | `.sldprt`, `.sldasm`, `.slddrw` |
| `model3d` | `ifc` | `.ifc` |
| `data` | `json` | `.json`, `.jsonc`, `.json5` |
| `data` | `json-lines` | `.jsonl`, `.ndjson` |
| `data` | `xml` | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.rss`, `.atom`, `.plist` |
| `data` | `yaml` | `.yaml`, `.yml` |
| `data` | `toml` | `.toml` |
| `data` | `parquet` | `.parquet` |
| `data` | `arrow` | `.arrow`, `.feather`, `.ipc` |
| `data` | `avro` | `.avro` |
| `data` | `orc` | `.orc` |
| `data` | `jupyter-notebook` | `.ipynb` |
| `data` | `protobuf-binary` | `.pb`, `.binpb` |
| `data` | `hdf5` | `.h5`, `.hdf5`, `.hdf` |
| `data` | `numpy` | `.npy`, `.npz` |
| `data` | `ini` | `.ini`, `.cfg`, `.conf` |
| `database` | `sqlite` | `.sqlite`, `.sqlite3`, `.db3`, `.s3db`, `.sl3` |
| `database` | `access` | `.mdb`, `.accdb` |
| `database` | `duckdb` | `.duckdb`, `.ddb` |
| `database` | `sql` | `.sql`, `.dump` |
| `geospatial` | `geojson` | `.geojson` |
| `geospatial` | `kml` | `.kml` |
| `geospatial` | `kmz` | `.kmz` |
| `geospatial` | `gpx` | `.gpx` |
| `geospatial` | `shapefile` | `.shp`, `.shx`, `.dbf`, `.prj`, `.cpg` |
| `geospatial` | `geopackage` | `.gpkg` |
| `geospatial` | `topojson` | `.topojson` |
| `geospatial` | `mbtiles` | `.mbtiles`, `.pmtiles` |
| `archive` | `zip` | `.zip`, `.zipx` |
| `archive` | `7z` | `.7z` |
| `archive` | `rar` | `.rar` |
| `archive` | `tar` | `.tar` |
| `archive` | `tar.gz` | `.tar.gz`, `.tgz`, `.taz` |
| `archive` | `tar.bz2` | `.tar.bz2`, `.tbz`, `.tbz2`, `.tb2` |
| `archive` | `tar.xz` | `.tar.xz`, `.txz` |
| `archive` | `tar.zst` | `.tar.zst`, `.tzst` |
| `archive` | `tar.lz` | `.tar.lz`, `.tlz` |
| `archive` | `gzip` | `.gz` |
| `archive` | `bzip2` | `.bz2` |
| `archive` | `xz` | `.xz` |
| `archive` | `zstd` | `.zst` |
| `archive` | `lz4` | `.lz4` |
| `archive` | `cab` | `.cab` |
| `archive` | `iso` | `.iso` |
| `archive` | `dmg` | `.dmg` |
| `archive` | `virtual-disk` | `.vhd`, `.vhdx`, `.vmdk`, `.qcow2`, `.img` |
| `binary` | `windows-executable` | `.exe`, `.com`, `.scr` |
| `binary` | `windows-library` | `.dll`, `.sys` |
| `binary` | `windows-installer` | `.msi`, `.msix`, `.appx`, `.msp` |
| `binary` | `shared-library` | `.so` |
| `binary` | `macos-library` | `.dylib`, `.bundle` |
| `binary` | `static-library` | `.a`, `.lib` |
| `binary` | `debian-package` | `.deb`, `.udeb` |
| `binary` | `rpm-package` | `.rpm` |
| `binary` | `android-package` | `.apk`, `.aab`, `.xapk` |
| `binary` | `ios-package` | `.ipa` |
| `binary` | `macos-package` | `.pkg` |
| `binary` | `java-archive` | `.jar`, `.war`, `.ear`, `.aar` |
| `binary` | `java-class` | `.class` |
| `binary` | `webassembly` | `.wasm` |
| `binary` | `python-bytecode` | `.pyc`, `.pyo` |
| `binary` | `python-wheel` | `.whl`, `.egg` |
| `binary` | `appimage` | `.appimage` |
| `binary` | `snap` | `.snap`, `.flatpak` |
| `email` | `eml` | `.eml` |
| `email` | `outlook-msg` | `.msg` |
| `email` | `mbox` | `.mbox`, `.mbx` |
| `email` | `icalendar` | `.ics`, `.ical`, `.icalendar`, `.ifb` |
| `email` | `vcard` | `.vcf`, `.vcard` |
