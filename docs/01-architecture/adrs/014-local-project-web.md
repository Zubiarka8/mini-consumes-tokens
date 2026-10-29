# ADR-014: One local website for atlas and duplicate review

## Status

Accepted for the local implementation, 2026-10-20. Publication is separate.

## Context

People need to move from project structure to duplicate groups and back without
reading the large agent-oriented TOON report. An Obsidian vault already derives
metadata from SQLite. The report presentation document proposes HTML/report
workflows but this branch has no shipped HTTP viewer. A common site avoids two
unrelated local servers and duplicated index storage.

## Decision

Add `mct-cli web [--port N] [--no-obsidian]`. It opens the existing index through
`Index::open_existing_read_only`, obtains a public bulk navigation snapshot, and
calls the established typed duplicate-analysis API with its default policy.
`mct-index` remains independent of HTML and Obsidian. No migrations, MCP tool
signature changes, evaluation/corpus hooks or source retrieval changes.

The shared overview links atlas and duplicate views. Files link to their groups;
group members link to files. Directory maps, symbols, resolved imports/importers
and members paginate at 100 entries and group lists at 20. Search uses the startup
metadata, including path and symbol names, without changing MCP/FTS retrieval or
token budgets. Every item is reachable; group detail and full-report TOON output
reuse the existing evidence renderer. The atlas carries metadata, not source
or Markdown bodies. Existing bounded duplicate excerpts remain in review pages.

`mct-obsidian` regenerates the disk vault at startup unless disabled. A failure
is a visible warning while the site remains usable. The site never reads user
notes as an alternative source. `web --no-obsidian` still exposes the SQLite atlas.

The HTTP server binds IPv4 loopback only, reuses the port remembered in
`.mct-index/web-port` (first `8787`, else an OS-selected free port) and prints
`http://localhost:<port>/` with no capability key. Update: the former 256-bit URL key was
removed so the address is stable; it also stopped other local processes, which is now
an accepted limitation. Exact Host/Origin checks,
Fetch Metadata checks, HTML/URL escaping, CSP, no-store/no-referrer and no framing
protect the private views. HTTP/1.1 GET/HEAD serve views. ADR-015 adds one protected POST endpoint for
review preferences. Headers are bounded to 16 KiB, review forms to 4 KiB, and
reading to a 2-second deadline; connections close after one response.
No request path maps to disk. No external resources, arbitrary file downloads,
automatic browser opener, network binding flag, index/source write endpoint or
CORS allowance.

## Consequences and limitations

- One foreground process and URL show both views, without Node, a frontend
  runtime, new heavy dependencies or an additional database. The two small
  direct dependencies (OS randomness and form URL encoding) already exist in
  the workspace lockfile.
- Pages use an immutable startup snapshot. After changes, reindex and restart;
  browser reload does not refresh the analysis. Automatic refresh and index/source
  mutations remain deferred. Human review preferences update immediately as
  specified in ADR-015.
- Duplicate analysis reads current source with existing stale-input safeguards;
  its metadata and diagnostics can therefore reflect a stale index. All diagnostics
  remain visible. It computes a typed report rather than parsing the TOON cache.
- Startup and memory cost grow with metadata and the full existing duplicate
  report. Pagination bounds rendered lists, not the analysis itself. No benchmark
  claims beyond measured local startup evidence.
- Group numbers belong to this snapshot; file paths are the cross-view identity.
- HTTP handles requests serially under strict timeouts, suitable for one local
  user. This is not a public or multi-user server. Local processes under the same
  account are trusted; no TLS or account authentication is provided.
- The generated Obsidian vault retains the ownership/publication limitations
  in ADR-013. The website does not embed the Obsidian app or its plugins.

## Follow-up: reviewed findings and exclusions

[ADR-015](015-duplicate-review-decisions.md) adds reversible human review decisions
through protected POST forms. The database remains read-only and raw analysis is
unchanged. `ignore-init --generated` configures the existing `.mctignore` rules;
public navigation snapshots apply those exclusions to files, symbols and imports,
including stale rows, rather than inventing website-only ignore semantics.
