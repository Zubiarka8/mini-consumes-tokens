# ADR-013: Obsidian vault location and generation

## Status

Accepted. (Requested as ADR-010 in the task brief; 010-012 were already taken
by the duplicate-report ADRs, so this is the next free number.)

## Context

Users want to browse the indexed code as linked notes in Obsidian. The vault
must follow the index (`init`, `reindex`, the server's background refresh), must
not pollute the project, and must not couple `mct-index` to any note format.

## Decision

- **Location:** `<root>/.mct-index/obsidian/`. The folder is already ignored by
  git, excluded from indexing (dot directories are skipped), regenerable, and
  next to the other derived reports.
- **Granularity:** a root `index.md`, one map note per indexed source directory,
  and one metadata note per indexed source file. The maps form a hierarchy so
  large repositories do not become one oversized note. A source-file note
  records its path, language, content hash, imports, and indexed symbols with
  line ranges; it never copies source contents. Binary assets are not listed.
- **Decoupling:** `mct-index` exposes a generic post-reindex hook
  (`Box<dyn Fn(&Index) -> Result<()> + Send>`), run after a successful
  `Index::reindex` and `Index::reindex_paths` (and the exclusion-change sweep).
  The new `mct-obsidian` crate registers it. `mct-eval` and `mct-corpus` never
  register it, so benchmarks write no vaults.
- **Failure:** a vault error never fails a reindex; it is added to
  `ReindexReport::warnings` and the CLI prints it on stderr.
- **Shared exclusions:** SQLite indexing and vault navigation use the same
  project-root `.mctignore`, with opt-in imported `.gitignore` rules. CLI and
  MCP retain the project-aware exclusion policy so checked reload rejects
  unreadable rules rather than silently removing exclusions. No vault-specific
  parser or ignore file is introduced. Reindex synchronizes stored rows and
  managed notes/links; manual generation only filters existing indexed metadata.
  Rule-read failures stop indexing/generation before publication, preserving
  the last valid vault; they are distinct from post-reindex vault-write warnings.
- **Incremental publication:** `.manifest.toon` records paths and SHA-256
  hashes. Actual note contents are verified before reuse or deletion. A no-op
  rewrites no notes; changed runs copy unchanged notes with their modification
  times preserved. Unique, exclusively created staging directories beside the
  vault contain the complete next tree before publication. A writer lock
  refuses concurrent generators. Two same-filesystem renames publish it, with
  rollback on a failed second rename. This is portable failure recovery, not
  a crash-atomic directory exchange: there is a short gap between renames.
  If rollback also fails, the error identifies the retained recovery directory.
- **Ownership:** all files absent from the manifest, including personal notes
  and `.obsidian/`, are copied intact. Unmanaged collisions, modified managed
  notes and corrupt manifests fail closed with a warning. No fixed `.tmp` or
  `.old` directories are deleted. Symlinks in existing ancestors and anywhere
  inside the vault are refused, including on no-op runs.
- **Names and links:** a flat filename under `files/` or `directories/`
  combines a sanitized ASCII label (80 characters maximum) and the full
  SHA-256 of the exact source path. Names are independent of enumeration and
  other files, bounded, and distinct for case-only, normalization and sanitized
  collisions. Hierarchy lives in map links, avoiding file/directory conflicts
  on case-insensitive filesystems. Links are escaped relative Markdown links,
  supported by Obsidian and ordinary readers.
- **Bounds:** file notes show up to 100 symbols and 50 unique resolved imports,
  with exact omitted counts. Maps split into pages of at most 100 entries
  linked by previous/next navigation. Displayed code labels cap at 160 Unicode
  characters and link labels at 240; full source paths remain in frontmatter.
  These are explicit item/character limits, not token estimates.
- **Manifest format:** `.manifest.toon` (a TOON table) rather than the
  `.manifest.json` first proposed, because the repository's rules reserve JSON
  for external protocols.
- **Opt-out:** `--no-obsidian` on `init`, `reindex` and `mct-mcp-server`;
  `mct-cli obsidian` regenerates by hand.
- **Source privacy and context:** the vault contains navigation metadata only,
  never project file bodies. This avoids duplicating private source text and
  leaves code excerpts to the existing MCP context/search tools. Markdown
  headings are listed as symbols like other indexed definitions.
- **Decided:** a vault failure is a warning, not a non-zero exit; the reindex
  it follows succeeded and the index is the source of truth.

## Alternatives considered

- **Vault at the project root.** Needs `.gitignore` entries and makes Obsidian
  index the whole project. Rejected.
- **One note per symbol.** Precise links, but explodes the file count and the
  cost of every regeneration. Rejected; one file note groups its symbols.
- **One monolithic index note.** Minimal file count, but directory navigation
  and AI context both degrade as the repository grows. Replaced by hierarchical
  maps that link to bounded file-level metadata notes.
- **Obsidian logic inside `mct-index`.** Couples the engine to a note format.
  Rejected in favour of the hook.
- **Rewrite the whole vault each time.** Simpler, but churns every file Obsidian
  watches on each background refresh. Rejected for the manifest comparison.
- **Failing the reindex on a vault error.** A derived convenience view must not
  block the primary index. Rejected.

## Consequences

- `Index::navigation_snapshot` reads files, symbols and direct imports in three
  bulk queries within one SQLite read transaction. No schema migration or
  source-content read is needed. Rendering and content verification remain
  proportional to index/vault size; preservation also copies personal files
  on a changed run. The writer lock is temporary even on no-op runs.
- Power loss or process termination between directory renames can leave the
  previous vault in a staging directory and an `obsidian.lock` file. Recover
  the retained previous tree and remove the lock only after confirming no
  writer is active. Filesystem operations do not defend against a malicious
  process concurrently replacing directories; do not run against an
  untrusted, concurrently mutated output tree.
- The server binary now depends on `mct-obsidian` and writes the vault by default.
- The generated note tree was checked by rendering tests, not yet by opening
  the vault in Obsidian.

## Reconciliation with the original brief

The current requirements supersede `prompt/claude/prompt14.md`: no Markdown
bodies are copied, failures are warnings, directory maps are paginated, names
use stable digests instead of order-dependent `~N`, and publication preserves
all unmanaged content instead of replacing it with generated notes alone.
TOON follows repository policy; ADR-013 retains the next available ADR number.
The original brief's requested author attribution does not establish authorship
for this continuation; no commit or publication is implied.

## Local web navigation

[ADR-014](014-local-project-web.md) adds `mct-cli web`, a shared localhost view of
the atlas and duplicate analysis. Its atlas uses public SQLite navigation APIs,
not Markdown vault contents. Vault generation remains in `mct-obsidian`; web
startup invokes it unless disabled and reports failures as warnings. No HTTP index/source write or refresh endpoint is added. ADR-015 subsequently
adds review-preference writes only.
