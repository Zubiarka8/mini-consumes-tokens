//! Which `pkg.F(…)` calls a file can only mean as the Go standard library's:
//! `pkg` is the name an import of a standard package binds (`errors`,
//! `json` for `encoding/json`, an alias), and nothing in the file rebinds
//! it. `cmd/go` resolves a standard import path from GOROOT before any
//! module, so a repository directory of the same name is never that import.
//!
//! A name counts as rebound when it appears anywhere in the file other than
//! as the operand of a selector (`errors.Is`): a local `path := …`, a
//! parameter `strings []string`, or a use as a value. That loses some
//! external answers but never claims a wrong one. Dot and blank imports
//! bind no name.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

/// Go 1.24's public standard packages (`src/` of the go1.24.0 tag, without
/// `internal`, `vendor`, `testdata` and `cmd`).
const PACKAGES: &[&str] = &[
    "archive/tar",
    "archive/zip",
    "bufio",
    "bytes",
    "cmp",
    "compress/bzip2",
    "compress/flate",
    "compress/gzip",
    "compress/lzw",
    "compress/zlib",
    "container/heap",
    "container/list",
    "container/ring",
    "context",
    "crypto",
    "crypto/aes",
    "crypto/cipher",
    "crypto/des",
    "crypto/dsa",
    "crypto/ecdh",
    "crypto/ecdsa",
    "crypto/ed25519",
    "crypto/elliptic",
    "crypto/fips140",
    "crypto/hkdf",
    "crypto/hmac",
    "crypto/md5",
    "crypto/mlkem",
    "crypto/pbkdf2",
    "crypto/rand",
    "crypto/rc4",
    "crypto/rsa",
    "crypto/sha1",
    "crypto/sha256",
    "crypto/sha3",
    "crypto/sha512",
    "crypto/subtle",
    "crypto/tls",
    "crypto/x509",
    "crypto/x509/pkix",
    "database/sql",
    "database/sql/driver",
    "debug/buildinfo",
    "debug/dwarf",
    "debug/elf",
    "debug/gosym",
    "debug/macho",
    "debug/pe",
    "debug/plan9obj",
    "embed",
    "encoding",
    "encoding/ascii85",
    "encoding/asn1",
    "encoding/base32",
    "encoding/base64",
    "encoding/binary",
    "encoding/csv",
    "encoding/gob",
    "encoding/hex",
    "encoding/json",
    "encoding/pem",
    "encoding/xml",
    "errors",
    "expvar",
    "flag",
    "fmt",
    "go/ast",
    "go/build",
    "go/build/constraint",
    "go/constant",
    "go/doc",
    "go/doc/comment",
    "go/format",
    "go/importer",
    "go/parser",
    "go/printer",
    "go/scanner",
    "go/token",
    "go/types",
    "go/version",
    "hash",
    "hash/adler32",
    "hash/crc32",
    "hash/crc64",
    "hash/fnv",
    "hash/maphash",
    "html",
    "html/template",
    "image",
    "image/color",
    "image/color/palette",
    "image/draw",
    "image/gif",
    "image/jpeg",
    "image/png",
    "index/suffixarray",
    "io",
    "io/fs",
    "io/ioutil",
    "iter",
    "log",
    "log/slog",
    "log/syslog",
    "maps",
    "math",
    "math/big",
    "math/bits",
    "math/cmplx",
    "math/rand",
    "math/rand/v2",
    "mime",
    "mime/multipart",
    "mime/quotedprintable",
    "net",
    "net/http",
    "net/http/cgi",
    "net/http/cookiejar",
    "net/http/fcgi",
    "net/http/httptest",
    "net/http/httptrace",
    "net/http/httputil",
    "net/http/pprof",
    "net/mail",
    "net/netip",
    "net/rpc",
    "net/rpc/jsonrpc",
    "net/smtp",
    "net/textproto",
    "net/url",
    "os",
    "os/exec",
    "os/signal",
    "os/user",
    "path",
    "path/filepath",
    "plugin",
    "reflect",
    "regexp",
    "regexp/syntax",
    "runtime",
    "runtime/asan",
    "runtime/cgo",
    "runtime/coverage",
    "runtime/debug",
    "runtime/metrics",
    "runtime/msan",
    "runtime/pprof",
    "runtime/race",
    "runtime/trace",
    "slices",
    "sort",
    "strconv",
    "strings",
    "structs",
    "sync",
    "sync/atomic",
    "syscall",
    "syscall/js",
    "testing",
    "testing/fstest",
    "testing/iotest",
    "testing/quick",
    "testing/slogtest",
    "testing/synctest",
    "text/scanner",
    "text/tabwriter",
    "text/template",
    "text/template/parse",
    "time",
    "time/tzdata",
    "unicode",
    "unicode/utf16",
    "unicode/utf8",
    "unique",
    "unsafe",
    "weak",
];

/// The package name a standard import path binds: its last element, or the
/// one before a `/vN` suffix (`math/rand/v2` binds `rand`).
fn package_name(path: &str) -> Option<&str> {
    if !PACKAGES.contains(&path) {
        return None;
    }
    let mut segments = path.rsplit('/');
    let last = segments.next()?;
    let is_version = last
        .strip_prefix('v')
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    if is_version {
        segments.next()
    } else {
        Some(last)
    }
}

/// The standard packages a file names and can't have rebound.
#[derive(Default)]
pub(crate) struct StdImports {
    names: HashSet<String>,
}

impl StdImports {
    pub(crate) fn of(root: Node, source: &str) -> Self {
        let mut bound: HashMap<String, usize> = HashMap::new();
        let mut std = HashSet::new();
        collect_imports(root, source, &mut bound, &mut std, 0);
        // Two imports binding one name is a compile error; prove neither.
        std.retain(|name: &String| bound.get(name) == Some(&1));
        let mut other = HashSet::new();
        let opaque = collect_other_uses(root, source, &std, &mut other, 0);
        if opaque {
            std.clear();
        }
        std.retain(|name| !other.contains(name));
        Self { names: std }
    }

    /// `name` in `name.F(…)` is a standard package of this file.
    pub(crate) fn is_std(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

fn collect_imports(
    node: Node,
    source: &str,
    bound: &mut HashMap<String, usize>,
    std: &mut HashSet<String>,
    depth: u32,
) {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return;
    }
    if node.kind() == "import_spec" {
        let path = node
            .child_by_field_name("path")
            .map(|p| text(p, source).trim_matches(|c| c == '"' || c == '`'));
        let alias = node.child_by_field_name("name");
        let name = match alias {
            // `import . "fmt"` and `import _ "embed"` bind no name.
            Some(a) if a.kind() != "package_identifier" => None,
            Some(a) => Some(text(a, source)),
            None => path.and_then(package_name),
        };
        let Some(name) = name else {
            return;
        };
        *bound.entry(name.to_string()).or_default() += 1;
        if path.and_then(package_name).is_some() {
            std.insert(name.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_imports(child, source, bound, std, depth + 1);
    }
}

/// Records every `std` name used other than as a selector operand; `true`
/// when the walk was cut short and so can't vouch for anything.
fn collect_other_uses(
    node: Node,
    source: &str,
    std: &HashSet<String>,
    other: &mut HashSet<String>,
    depth: u32,
) -> bool {
    if depth >= MAX_TRAVERSAL_DEPTH {
        return true;
    }
    if node.kind() == "import_spec" {
        return false;
    }
    if node.kind() == "identifier" {
        let name = text(node, source);
        let operand = node.parent().is_some_and(|p| {
            p.kind() == "selector_expression"
                && p.child_by_field_name("operand")
                    .is_some_and(|o| o.id() == node.id())
        });
        if !operand && std.contains(name) {
            other.insert(name.to_string());
        }
    }
    let mut opaque = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        opaque |= collect_other_uses(child, source, std, other, depth + 1);
    }
    opaque
}
