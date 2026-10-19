# Presenting duplicate code to people and to agents

Research for issue #207: how the `duplicates` report should show repeated code to a
person reading it and to an AI agent consuming it. Nothing here changes the command;
every proposal below is a candidate for a follow-up issue.

Measured on `feat/207-token-twins` at `71ec6d0`, release `mct-cli`, this repository,
default options (`broad`, `--min-lines 6`, `--min-tokens 40`): 183 groups, 545 members.
Token counts use the project heuristic `mct_eval::approx_tokens` (a word run is one
token, every other non-space character is one), not a model tokenizer.

## Who reads the report, and for what

| Reader | Task | What they need first |
|---|---|---|
| Person, triage | "Is there duplication worth acting on, and where?" | Totals split by production / tests / examples / corpus, then the few largest production groups |
| Person, judging one group | "Are these really the same rule?" | The members side by side, with only the differing tokens highlighted |
| Person, reviewing a change | "Did this PR add a copy?" | Only groups that touch the changed lines, inline in the diff |
| Agent, discovery | Decide whether any group is relevant to its task | A compact, ranked index with stable ids and `path:line` |
| Agent, refactoring | Write the shared function | The common code once, plus the values that differ per member (the future parameters) |
| Agent, verification | Check that the group is gone after the edit | Re-run with the same id or path filter and a cheap answer |

The two audiences need the same data at different depths. Both are served by
progressive disclosure (summary, index, one group), not by one large dump.

## The current report, measured

The report is one TOON document: header, then every group with its verdict,
priority, alignment, context, subgroups and members.

| Measure | Value |
|---|---|
| Whole report with excerpts | ~121,900 tokens (494 KB), ~666 per group |
| Header alone | ~480 tokens |
| Raw source of every member of every group | ~54,400 tokens |
| Detail for the 158 groups the prototype could align | ~103,200 tokens, vs ~43,900 for those members' full source |

The group detail costs about 2.3 times the source code it describes, yet it never
shows that code: the excerpt is the first three lines, which are almost always the
signature.

Where the tokens go:

| Section | Share | Note |
|---|---|---|
| `members` (incl. excerpts) | 33.3% | `language`, `kind`, `lines` and `tokens` repeat per member, though Type-2 members share them |
| `alignment.differences` | 17.6% | The useful part: what differs and where |
| `evidence` | 11.8% | Restates every member's `path:start-end` already in `members` |
| `alignment.warnings` | 10.1% | Restates `differences` in prose, with the same example values |
| `subgroups` | 6.7% | 146 of 183 groups are `single`, so the section says nothing for them |
| `context`, `priority`, `types`, `verdict` | 15.2% | |
| Empty arrays (`conflicts[0]`, `warnings[0]`, …) | 5.2% | Printed even when empty |

Other observations:

- There is no output for people. The ranked TOON is the only format. The MCP tools
  default to TOON as well, but offer `format: text` as an opt-in plain shape
  (`OutputFormat` in `crates/mct-mcp-server/src/toon.rs`).
- Nothing is written inside the user's project. The report exists only on stdout,
  and there is no MCP tool (ADR-007), so an assistant connected over MCP cannot
  show it at all.
- Production, tests, examples and the labeled corpus are mixed in one ranking. In
  this repository 113 groups are test-only, 32 are the labeled corpus or fixtures
  (intentional), 25 are production-only and 13 mix areas. A reader has to classify
  them by hand to find the 25 that matter most.
- Group ids are ranks. They shift when any group above changes, so neither a person
  nor an agent can refer to "group 6" across runs.
- The ranking ignores how many copies there are beyond size. 123 of the 183 groups
  have two copies and 60 have three or more, yet a two-copy group outranks a
  five-copy group of the same size.
- The report already has consumers that read its fields:
  - `benchmarks/score_duplicate_labels.py` reads `members`, `verdict` and
    `subgroups`, and scores a group without a `subgroups` block as `absent`.
  - `crates/mct-cli/tests/duplicates.rs` asserts on `evidence` and `warnings`.
  - `COMMANDS.md` documents the fields.
- Verdicts are not reliable enough to lead the display. On the labeled corpus the
  verdict accuracy is 0.296 and every group is called `SUSPICIOUS`
  (`token-twins-labels-integrated.toon`); grouping recall, by contrast, is 1.000.

## Prior art

| Tool / work | Presentation | Lesson |
|---|---|---|
| PMD CPD | text, xml, csv, csv-with-line-count, vs, markdown (7.17+), HTML via XSLT; text and markdown print the duplicated code once with the list of occurrences | Show the code once, then the locations. A Visual Studio format makes `path(line)` clickable |
| jscpd | console, html, markdown, json, sarif, badge, codeclimate, an `ai` reporter claimed to use ~79% fewer tokens than console, and an MCP server mode | Ship a separate compact format for agents, and an agent-callable tool, instead of making one format serve both |
| SonarQube | Project metrics (duplicated lines density, blocks, files; also for new code only) and, in the file view, a gutter marker that opens the other locations | Lead with one density number, track it on new code, and put the detail next to the source |
| IntelliJ Locate Duplicates | Tree of fragments with a size "cost", and a side-by-side diff of the selected fragments with next/previous difference | People judge a clone by diffing it, with only the differences highlighted |
| GitHub code scanning (SARIF) | One result per finding, `relatedLocations` linked from the message, `partialFingerprints` to match results across commits | A stable fingerprint lets a finding survive edits; SARIF puts candidates into PR review |
| Rieger, Ducasse, Lanza (WCRE 2004) | Polymetric views: duplication web between files, clone scatterplots, aggregation treemaps, clone class family enumerations | System-wide views answer "which parts copy each other"; a crate-by-crate matrix is the small version |
| Dotplots, scatter plots, treemaps; Tairas' Visualizer (bar per file, color per clone class) | Visual overviews of where clones are | Scatter plots grow with system size, treemaps lose the pairing; both need drill-down to the code |
| Hammad et al., clone visualization mapping study (2020, 68 publications) | Classifies visualizations by user goal and information need | Pick the view per task, as in the reader table above |
| Li & Thompson (Wrangler), anti-unification | The least general common abstraction of the clones: one template, the differing parts as parameters | The template with holes is both the explanation and the refactoring plan |
| Liu et al., "Lost in the Middle" (TACL 2024) | Models use information at the start and end of long inputs best | Keep agent input short. Put stable material (notice, totals) first and the group the agent must act on last, never buried in the middle of a long dump |
| Aider, unified diffs | Unified diffs improved GPT-4 Turbo's editing (20% to 61% on its benchmark) | Models read diffs well. The result is about the model's output format and is self-reported, so it is only weak evidence for input |

## Prototypes

Built from the current report and the files on disk (scripts outside the repository;
none of this is implemented in `mct-cli`):

- **P1, compact index:** one header with notice, status, policy and total, then one
  row per group: `id, verdict, area, copies, lines, symbol, locations`.
- **P1b, triage summary:** counts per area (production, tests, examples, corpus and
  mixes), then the ten largest production groups.
- **P2, template with holes:** each member is tokenized and the members are aligned
  token by token. The code is printed once, and each differing position becomes a
  numbered hole `⟨n⟩`. A table then gives each member's value for each hole.
- **P3, unified diff:** the first member in full, then a zero-context diff to each
  other member.

| Format | Scope | Tokens | vs current |
|---|---|---|---|
| Current report | all 183 groups, 3-line excerpts | ~121,900 | — |
| P1 compact index | all 183 groups, every location, no code | ~9,600 | −92% |
| P1b triage summary | areas + top 10 production | ~340 | −99.7% |
| P2 template with holes | 158 aligned groups, **full code** | ~41,500 | −60% vs those groups' current detail (~103,200) |
| P3 unified diff | all 183 groups, **full code** | ~70,000 | −42% |

P2 printed for group 6, the `find_references` / `find_calls` / `find_callers` MCP
handlers (trimmed):

```text
group 6: 3 copies x 35 lines, verdict SUSPICIOUS
members[3]{id,location,symbol}:
  m1,mct-mcp-server/src/server.rs:1973-2007,find_references
  m2,mct-mcp-server/src/server.rs:2011-2045,find_calls
  m3,mct-mcp-server/src/server.rs:2049-2083,find_callers
template (holes ⟨n⟩ differ between members):
    pub async fn ⟨1⟩(
        &self,
        Parameters(⟨2⟩ { ⟨3⟩, path, language, limit, depth, offset, format, }): Parameters<⟨2⟩>,
    ) -> Result<CallToolResult, McpError> {
        let ⟨3⟩ = validate_name(&⟨3⟩)?;
        …
            .relation_page(⟨3⟩, mct_index::RelationDirection::⟨4⟩, depth.unwrap_or(1), (limit, offset), scope)
        …
        let text = format::relation_page_hits(⟨3⟩, ⟨5⟩, &hits, output_format == OutputFormat::Toon);
holes[5]{hole,m1,m2,m3}:
  ⟨1⟩,find_references,find_calls,find_callers
  ⟨2⟩,FindReferencesArgs,FindCallsArgs,FindCallersArgs
  ⟨3⟩,symbol,function,function
  ⟨4⟩,References,Calls,Callers
  ⟨5⟩,"reference(s)","call(s) made by this function","caller(s) of this function"
```

The current report needs ~924 tokens for this group and shows only the signature. P2
needs ~400 tokens, which is less than the members' own source (~650), and shows all of
it. Read as a refactoring plan, holes ⟨1⟩ and ⟨2⟩ are the declaration and its argument
type. The real parameters are ⟨4⟩ (the relation direction) and ⟨5⟩ (the label). Hole
⟨3⟩ is a binding name, so a merge would have to rename it.

Limits of the prototypes:

- The naive lexer aligns 158 of 183 groups. The rest differ in comments, so
  members have different token counts under it. The real implementation would use the
  index token sources, which already align every group.
- P3 grows with the number of members: group 1 (15 members) is cheap only because
  its copies are identical.
- No measurement with a model tokenizer and no user study was done.

## Recommendations

Every field in today's TOON is observable output, and the consumers listed above
already depend on some of them. The recommendations are therefore additive: new
depths and formats sit next to the current report, which stays as it is. Removing or
renaming a field is a contract change and needs its own decision.

1. **Render one data model at three depths for both audiences.**
   - **Summary:** status, a one-line notice, and totals by area and by language.
   - **Index:** ranked rows with id, area, copies, lines, symbol and `path:line`.
   - **Group:** the template with holes and the hole table, then context and
     warnings only when they say something.
   - **Full:** today's TOON, unchanged.
   - **Defaults:** people get summary plus index. Agents ask for the depth they need.
   - **Order for agents:** stable material (notice, policy, totals) first, the group
     to act on last.
2. **Classify members by area.** The areas are `production`, `test`, `example` and
   `corpus`, read from the path and from `#[test]` / `#[cfg(test)]` context, which the
   report already collects. Rank production first, collapse tests, and label the
   corpus. Totals stay unchanged, and filters (`--area`) are optional. Collapsing tests
   is also a judgment about their value: duplication in tests is often deliberate,
   because each test should read on its own (DAMP over DRY), so most of the 113
   test-only groups are not refactoring work.
3. **Rank groups with three or more copies above pairs.** A common rule is not to
   extract an abstraction before the third use. Within the same area, a group with
   three or more copies should rank above a two-copy group of similar size.
4. **Show the code once.**
   - In the new depths, show the template with holes instead of the three-line
     excerpt.
   - Show a side-by-side or unified diff when alignment cannot form a template.
   - Ignore holes whose kind is `declaration` when counting parameters. Report the
     remaining count as "differs in N places".
5. **Leave restated information out of the new agent depths.**
   - `evidence` repeats `members`, and `warnings` repeats `differences`.
   - Members of a Type-2 group share `language`, `lines` and `tokens`, so those
     fields need not repeat per member.
   - Empty arrays and `single` subgroups carry no information.
   - `evidence`, `warnings` and the empty arrays alone are about a quarter of
     today's cost (sections overlap slightly, so this is not an exact sum).
   - Trimming the full TOON itself would break the CLI tests and the scorer's
     subgroup scoring, so it is not proposed here.
6. **Give groups stable ids.** Derive a short id from the normalized fingerprint, so
   that `--group <id>` keeps working when ranks shift and a re-run can confirm a group
   is gone. ADR-007 says the hash is not part of the contract, so changing that is a
   decision for the maintainer.
7. **Give people a plain text format, and keep TOON as the default.** The MCP tools
   default to TOON and offer `format: text` as an opt-in. The CLI should follow the
   same convention with `--format text`, which leaves every existing invocation,
   the scorer, the tests and `COMMANDS.md` unchanged. In the text output:
   - Print `path:line` so terminals and editors make it clickable.
   - Highlight holes with color on a TTY.
8. **Make verdicts secondary.** Until verdict accuracy improves, do not use the verdict
   as the headline. Lead with the concrete differences, for example "differs in 2
   places: the relation direction and a label". That explanation is what both
   readers act on.
9. **Record the accepted decisions in a new ADR.** Later #207 decisions went into
   their own ADRs (008 to 012) instead of rewriting ADR-007. The presentation decisions (depths,
   areas, ranking, stable ids, default format) belong in the next one, ADR-013, which
   states which parts of ADR-007 it updates or supersedes.
10. **Later, as separate issues:**
   - **MCP tool.** Exposing `duplicates` (listed in ADR-007 as a follow-up) is an MCP
     contract change, so it needs an issue first. It would be the summary/index/group
     levels with `limit` and `offset`.
   - **New-code view.** Report only the groups touching lines changed against a base
     ref, as SonarQube does with new code.
   - **SARIF.** Emit one result per group, with the other members in
     `relatedLocations` and the stable id as the fingerprint, for code scanning and
     IDEs. SARIF is JSON, but an external format requires it, which `rules.md` allows.
   - **HTML report inside the project.** See
     [Proposal: reports inside the project](#proposal-reports-inside-the-project)
     and its issue draft below.

## System-wide view for people

The crate-by-crate counts already show the "duplication web":

- **Pairs that share groups:** the language crates with similar grammars share the
  most. `mct-lang-python` and `mct-lang-rust` share 20 groups. `bash`/`powershell`
  and `html`/`xaml` share 15 each. `java`/`kotlin` share 12.
- **Groups inside one crate:** 98 groups stay inside one crate (`mct-mcp-server` 26,
  the corpus 26, `mct-index` 24). The other 85 cross crates.

A matrix of these counts works for the one-page overview. Rieger et al. found that
scatter plots stop working at system scale, so this size is about the limit.

## Proposal: reports inside the project

A user who runs the MCP server or `mct-cli` on their own project should find the
reports in that project, not on stdout only and not on an external service. The
place for them is `.mct-index/`, as long as the tool writes them there:

- **Regenerable.** The README calls `.mct-index/` "safe to delete/rebuild anytime",
  and a report can always be generated again.
- **Not only the index today.** The folder already holds data that is not the
  index: the embedding model cache (`.mct-index/models`) and the default output
  of `dead-code` (`.mct-index/dead-code-report.csv`).
- **Never indexed.** `.mct-index` is in the default exclusions
  (`crates/mct-index/src/exclude.rs`), so a report never indexes itself.
- **Kept out of git.** `mct-cli gitignore-init` adds `.mct-index/` to the
  project's `.gitignore`.
- **Local.** A self-contained HTML file opens in any browser without network
  access, so the project's source never leaves the machine.

Other tools do the same: jscpd writes its reports into a folder of the project,
`cargo llvm-cov --html` into `target/llvm-cov/html` and coverage.py into `htmlcov/`.

Every report gets two formats, because its two readers need different things:

| Report | For people | For agents |
|---|---|---|
| Duplicates | `.mct-index/reports/duplicates.html` | TOON: `duplicates` on stdout, and the `find_duplicates` MCP tool at `summary`, `index` or `group` depth |
| Dead code | `.mct-index/reports/dead-code.html` | TOON: `dead-code --format toon` on stdout, and the existing `find_dead_code` MCP tool |

The CSV of `dead-code` stays for spreadsheets and scripts.

### Layout

One flat `reports/` folder holds every report, and one page at the top of
`.mct-index/` links to all of them:

```text
.mct-index/
  index.sqlite3       # unchanged; only mct-cli and the MCP server touch it
  models/             # unchanged
  index.html          # reports index, rebuilt from reports/*.html on every report write
  reports/
    duplicates.html   # first issue
    dead-code.html    # second issue
    dead-code.csv     # moved from .mct-index/dead-code-report.csv
```

- **Flat, not one folder per report.** The pages are self-contained, so no report
  needs a folder for its own assets.
- **Two pages, not one.** One page with both reports would have to run both
  analyses every time either is wanted, the slower duplicate analysis included. It
  would also mix reports generated with different options (`--path`, `--language`,
  normalization). An MCP tool for one report would have to run the other too.
- **`reports/` holds only reports.** Listing `reports/*.html` therefore never picks
  up the database, the model cache or the reports index itself.

### Reports index and navigation

A page opened from disk (`file://`) cannot list a folder or check whether another
file exists: browsers block reading other local files from a `file://` page. So the
tool does that work when it writes a report, not the browser when it opens one:

1. **Every report write rebuilds `index.html`.** The tool lists
   `.mct-index/reports/*.html` and reads each page's `<meta name="mct-report-…">`
   tags. These carry the report name, generation time, commit, options and result
   count, and the tool wrote them itself. A new kind of report therefore shows up
   in the index without changing the index code. A deleted page disappears on the
   next rebuild.
2. **The index page** shows one row per report: name, last update (date and
   time), commit, options, result count and a link. A report that does not exist yet shows the
   command that generates it, for example
   `mct-cli --root . dead-code --format html`. A report generated before the last
   reindex is marked as older than the index.
3. **Every report page has a navigation header.** It links to `../index.html` and to
   the other reports that existed when the page was written. The link to the index
   always works, because the index is rebuilt on every write. A sibling created
   later is reachable from the index until the page is regenerated.
4. **A file in `reports/` that the tool did not write** (no `mct-report` meta tags)
   is listed by its file name only, with every text escaped.

### Opening the browser

So that the person does not have to find the file, every command that writes a
report takes `--open`:

```sh
mct-cli --root . duplicates --format html --open
# Wrote 183 duplicate group(s) to .mct-index/reports/duplicates.html
# Opened .mct-index/reports/duplicates.html in the default browser
```

- **Opt-in.** Opening a window would be wrong in CI, in a script, over SSH, or when
  an agent runs the CLI without the person asking, so the default is not to open.
- **No new dependency and no shell.** The path is passed as an argument to the
  platform's opener: `open` on macOS, `xdg-open` on Linux, `explorer` on Windows.
  `explorer` is used instead of `cmd /c start`, so a `&` or a space in the path
  cannot break the command line.
- **Never fatal.** If no opener exists or it fails (no display, no `xdg-open`), the
  report is still written. The tool prints its path with a one-line warning and
  exits successfully.
- **Only paths the tool wrote.** `--open` opens the page just written (or the index,
  for `report`), never a URL taken from input.
- **From MCP.** The tool output always carries the page as a `file://` link, which
  most terminals make clickable. With `open: true` the server opens it on the
  user's machine, where a stdio MCP server runs. The tool description tells the
  agent to set it only when the person asks to see the report.

### One command for everything

`mct-cli --root . report --open` writes every HTML report (duplicates, dead code),
rebuilds the index and opens `index.html`. The person needs to remember one
command, and the agent can suggest one command. It belongs to the second issue,
since it needs both pages.

### Refreshing from the page

Every page shows when it was last updated, as a date and time, and a **Refresh**
button that is always visible. A page opened from disk cannot run the tool, so the
button only works when the tool serves the page itself:
`mct-cli --root . report --serve --open`. The button is enabled only when something
changed since the page was generated. While the refresh runs, a progress panel
shows each phase until the new report is loaded. The details are in the
[third issue draft](#issue-draft-refresh-from-the-page).

### Issue draft: duplicate report

The text below is ready to open as a GitHub issue. Opening it, and every decision
it lists as open, belongs to the maintainer.

**Title:** `feat(reports): write the duplicate and dead-code reports under .mct-index/reports/`

#### Problem

- `mct-cli duplicates` prints a TOON report on stdout and writes nothing. People
  have no readable view of it, and the full report costs ~121,900 tokens on this
  repository, so it is also a poor input for an agent.
- No MCP tool exposes duplicates (ADR-007), so an assistant connected over MCP
  cannot find or show them.
- `dead-code` writes its CSV directly into `.mct-index/`, next to the database.
  There is no convention for where reports go.
- Even with a page on disk, the person would have to find it by hand.

#### Proposal

Four parts, each shippable as its own PR, in this order. The layout, the index and
`--open` are described in [Proposal: reports inside the project](#proposal-reports-inside-the-project).

1. **Reports folder.** Reports are written by default to `.mct-index/reports/`.
   `dead-code` moves its default from `.mct-index/dead-code-report.csv` to
   `.mct-index/reports/dead-code.csv`. `--output` keeps working as today.
2. **HTML duplicate report.** `mct-cli duplicates --format html` writes a
   self-contained page to `.mct-index/reports/duplicates.html` (or to
   `--output <file>`) and prints one line with the path and the group count.
   `--format toon` is the default, and its stdout is byte-identical to today's. The
   same `--format` flag later takes `text` (recommendation 7) and is shared with
   `dead-code` (second issue below).
3. **Reports index, navigation header and `--open`.** Writing any HTML report
   rebuilds `.mct-index/index.html` from `reports/*.html`. Every page links to the
   index and its siblings, and `--open` opens the page just written.
4. **MCP tool `find_duplicates`.** It returns the report at the depth the agent
   asks for: `summary`, `index` or one `group`. With `html: true` it also writes the
   HTML report and returns its `file://` link, so the agent can point the person to
   it; with `open: true` as well, it opens the page.

#### The HTML page

- **Navigation header:** links to `../index.html` and to the other reports that
  existed when the page was written.
- **Header:** project name, last update, index status, the options used, and the
  command that regenerates the page. The same values go into the `mct-report` meta
  tags that the reports index reads.
- **Last update, date and time:** the time the page was generated, written as UTC
  in a `<time datetime="2026-10-17T12:32:05Z">` element, so it reads correctly with
  scripts disabled. The inline script shows it in the reader's local time, with how
  long ago it was ("Last updated 17 Oct 2026, 14:32:05 · 2 hours ago"). The
  relative part updates every minute. The tool formats the UTC value itself from
  `SystemTime`, so no date crate is added.
- **Summary:** total groups and members, counts by language, complete or partial,
  and skipped candidates.
- **Index:** one row per group with rank, verdict, copies, lines, first symbol and
  `path:start-end` of every member. Filtering by language, verdict and path is done
  by a small inline script.
- **One section per group:** the member list; the common code once, with each
  differing position shown as a numbered, colored hole `⟨n⟩`; the hole table with
  one column per member; then verdict reasons and context only when they say
  something.
- **No external resources.** CSS and the script are inline, and a
  Content-Security-Policy forbids any network request.
- **No JSON payload.** The page is rendered as HTML on the Rust side, in line with
  the `rules.md` ban on new JSON output. It stays readable with scripts disabled;
  the script only filters rows and collapses sections.

#### Pseudocode

Report paths, shared by `dead-code` and `duplicates`:

```text
const REPORTS_DIR  = ".mct-index/reports"
const REPORTS_HOME = ".mct-index/index.html"

enum ReportKind { Duplicates, DeadCode, DeadCodeCsv }

fn default_report_path(root, kind) -> PathBuf:
    match kind:
        Duplicates  -> root / REPORTS_DIR / "duplicates.html"
        DeadCode    -> root / REPORTS_DIR / "dead-code.html"
        DeadCodeCsv -> root / REPORTS_DIR / "dead-code.csv"

// What every known report needs for the index and for "not generated yet".
const KNOWN_REPORTS = [
    ("duplicates", "Duplicate code", "mct-cli --root . duplicates --format html"),
    ("dead-code",  "Dead code",      "mct-cli --root . dead-code --format html"),
]

fn write_report(path, contents) -> Result<()>:
    create_dir_all(path.parent())
    // Write next to the target, then rename: a browser or a reader never sees
    // half a file, and a failed write leaves the previous report in place.
    tmp = path.with_extension("tmp")
    write(tmp, contents)
    rename(tmp, path)

fn write_html_report(root, kind, output: Option<PathBuf>, page) -> Result<PathBuf>:
    path = output.unwrap_or(default_report_path(root, kind))
    write_report(path, page)
    // A custom --output outside reports/ is the user's own file: it is written,
    // but the index lists only reports/*.html.
    rebuild_reports_index(root, index)?
    return path
```

Reports index and navigation:

```text
struct ReportMeta { name, title, generated_at, commit, options, results }

fn rebuild_reports_index(root, index) -> Result<()>:
    found = []
    for file in read_dir(root / REPORTS_DIR) sorted by name:
        if file.extension() != "html": continue
        // Read only the <head>: the tool wrote the meta tags one per line.
        meta = read_meta_tags(file, prefix: "mct-report-")
        found.push(meta.unwrap_or(ReportMeta::file_name_only(file)))
    index_built_at = index.status().last_indexed
    write_report(root / REPORTS_HOME, render_reports_index(found, KNOWN_REPORTS, index_built_at))

fn render_reports_index(found, known, index_built_at) -> String:
    out = SHELL_HEAD                       // same shell and CSP as the report pages
    out += "<h1>Reports for " + escape_html(project_name) + "</h1>"
    out += "<table>"
    for meta in found:
        stale = meta.generated_at < index_built_at
        out += row(link("reports/" + escape_attr(meta.file), escape_html(meta.title)),
                   escape_html(meta.generated_at), escape_html(meta.commit),
                   escape_html(meta.options), meta.results,
                   if stale: "older than the index" else: "")
    for (name, title, command) in known where name not in found:
        out += row(escape_html(title), "not generated yet", "<code>" + escape_html(command) + "</code>")
    out += "</table>" + SHELL_TAIL
    return out

fn navigation_header(root, current) -> String:
    // Written once, with the page. The link to the index always works because the
    // index is rebuilt on every write; siblings created later appear there.
    links = [link("../index.html", "All reports")]
    for (name, title, _) in KNOWN_REPORTS where name != current:
        if exists(root / REPORTS_DIR / (name + ".html")):
            links.push(link(name + ".html", title))
    return "<nav>" + join(links, " · ") + "</nav>"
```

Opening the browser:

```text
fn open_in_browser(path) -> Result<(), OpenError>:
    // Only paths this process just wrote. Arguments, never a shell string.
    command = match os:
        macos   -> Command::new("open").arg(path)
        linux   -> Command::new("xdg-open").arg(path)
        windows -> Command::new("explorer").arg(path)   // its exit code is unreliable: ignore it
    command.stdin(null).stdout(null).stderr(null).spawn()?   // do not wait for the browser
    return Ok(())

fn finish(path, open: bool):
    println("Wrote {n} {what} to {path}")
    if open:
        match open_in_browser(path):
            Ok  -> println("Opened {path} in the default browser")
            Err -> println("warning: could not open a browser ({error}); open {path} yourself")
    // Exit status stays 0 either way: the report was written.
```

Template with holes, in `mct-index`. The members of a group share one normalized
token sequence, so position `i` is the same construct in every member; only
identifier and literal slots can differ, and punctuation is equal by construction.
`Slot` stores only a line today, so it needs the token's byte range:

```text
struct GroupTemplate { segments: Vec<Segment>, holes: Vec<Hole> }
enum   Segment       { Code(String), Hole(u16) }
struct Hole          { id: u16, kind: DifferenceKind, values: Vec<String> } // one value per member

fn group_template(members: &[MemberSlots], first_source: &str) -> GroupTemplate:
    first    = members[0]
    holes    = []
    hole_of  = HashMap<Vec<TextId>, u16>     // the same column of values reuses its hole
    segments = []
    cursor   = first.span.start_byte
    for i in 0..first.slots.len():
        column = [m.slots[i].text for m in members]
        if all_equal(column): continue
        slot = first.slots[i]
        segments.push(Code(first_source[cursor .. slot.start_byte]))
        id = hole_of.get_or_insert(column, || {
            holes.push(Hole { id: holes.len() + 1, kind: difference_kind(column), values: texts(column) })
            holes.len()
        })
        segments.push(Hole(id))
        cursor = slot.end_byte
    segments.push(Code(first_source[cursor .. first.span.end_byte]))
    merge_adjacent_code(segments)
    return GroupTemplate { segments, holes }

fn parameter_count(template) -> usize:
    // Declaration holes are the function's own name; they are not parameters of a merge.
    template.holes.filter(|h| h.kind != Declaration).count()
```

Layout and comments come from the first member. Members whose comments differ
still align, because comments are not slots; this is the case the prototype's
naive lexer could not handle.

HTML rendering:

```text
fn render_html(report, templates, meta) -> String:
    out = SHELL_HEAD                 // include_str!: doctype, inline CSS, CSP meta
    out += header(meta)              // project, generated_at, index status, options, regenerate command
    out += summary(report)
    out += index_table(report)       // one <tr> per group
    for (rank, group, template) in report.groups.zip(templates):
        out += "<section id=\"group-{rank}\">"
        out += member_list(group)    // escape_html(path):start-end escape_html(symbol)
        out += "<pre><code>"
        for segment in template.segments:
            Code(text) -> out += escape_html(text)
            Hole(id)   -> out += "<mark class=\"hole h{id % 8}\" title=\"{kind}\">⟨{id}⟩</mark>"
        out += "</code></pre>"
        out += holes_table(template) // escape_html on every value
        out += verdict_and_context(group), only when non-empty
        out += "</section>"
    out += SHELL_TAIL                // inline filter script
    return out

fn escape_html(text) -> String:
    // Every text taken from the repository goes through this: & < > " '
```

CLI flow, `crates/mct-cli/src/duplicates.rs`:

```text
fn run(root, options, format: ReportFormat, output: Option<PathBuf>, open: bool):
    index  = Index::open_existing_read_only(...)                    // unchanged
    report = find_duplicate_groups(&index, &token_sources, options) // unchanged
    match format:
        Toon ->                                                     // default
            if open: bail("--open needs --format html")             // nothing to open
            print(render_toon(report))                              // unchanged
        Html ->
            templates = group_templates(&index, &report)
            page = render_html(report, templates, meta(root, &index, options),
                               navigation_header(root, "duplicates"))
            path = write_html_report(root, Duplicates, output, page)?   // also rebuilds index.html
            finish(path, open)                                          // "Wrote …", then --open
            if !gitignore_lists_index(root):
                println("hint: run `mct-cli --root . gitignore-init` to keep .mct-index/ out of git")
```

MCP tool, `crates/mct-mcp-server/src/server.rs`:

```text
struct FindDuplicatesArgs {
    path, language, normalization,     // same meaning as the CLI options
    depth: Option<String>,             // summary | index (default) | group
    group: Option<usize>,              // required when depth = group; a rank until ids are stable
    limit, offset,                     // index depth only; default limit 50, like find_dead_code
    format: Option<String>,            // toon (default) | text, as every tool
    html: Option<bool>,                // default false: the tool writes nothing unless asked
    open: Option<bool>,                // default false; needs html; only when the person asks
}

async fn find_duplicates(args) -> CallToolResult:
    depth  = parse_depth(args.depth)?           // unknown value -> invalid_params
    require(args.group.is_some() == (depth == Group))?
    index  = self.index.lock().await
    report = find_duplicate_groups(&index, &token_sources, options_from(&args))
    text = match depth:
        Summary -> render_summary(&report, format)
        Index   -> render_index(&report, format)          // compact rows, P1 shape
        Group   -> render_group(&report, group, format)   // template + hole table
    require(args.open != Some(true) || args.html == Some(true))?
    if args.html == Some(true):
        // Fixed path: the tool has no path argument.
        path = write_html_report(root, Duplicates, None, render_html(...))?   // rebuilds index.html
        text += "report: file://" + path + "\nall reports: file://" + root / REPORTS_HOME
        if args.open == Some(true):
            match open_in_browser(path):
                Ok  -> text += "\nopened in the default browser"
                Err -> text += "\ncould not open a browser; give the person the link above"
    return success(text)
```

For agents, the summary and stable header lines come first and the requested group
last, as recommended above.

#### Compatibility

- **`duplicates` without `--format`:** stdout stays byte-identical. A golden test
  compares it against today's output on the fixtures.
- **`dead-code` default path changes.** This is a visible change: list it under
  "Changed" in `CHANGELOG.md`. An old `.mct-index/dead-code-report.csv` is left
  where it is; the tool never deletes a user's file. `--output` is unchanged.
- **New MCP tool.** Adding a tool changes the published catalog, which `rules.md`
  treats as cross-cutting, hence this issue. `scripts/unix/new-tool-check.sh
  find_duplicates` must pass, including the description-byte budget.
- **Module doc.** `crates/mct-cli/src/duplicates.rs` says the command "never writes
  anything". That stays true for the default TOON format, and the doc has to say so.
- **New file outside `reports/`.** `.mct-index/index.html` is written only when an
  HTML report is written. TOON and CSV runs never create it.
- **`--open`** is new and off by default, so no existing invocation opens anything.
- **ADR.** The decisions go into ADR-013, which states which parts of ADR-007
  (TOON only, no MCP tool) it supersedes.

#### Security

- **Repository text is untrusted.** Paths, symbols, code and hole values are
  escaped before they enter the HTML, so a `</script>` or `<img onerror>` in the
  source shows up as text. A test covers it.
- **No network.** The CSP is `default-src 'none'; style-src 'unsafe-inline';
  script-src 'unsafe-inline'`, so the page cannot load or send anything.
- **Writes stay inside the project.** The MCP tool has no path argument; it writes
  only the fixed report path. Before writing, the resolved parent directory must be
  inside `<root>/.mct-index/`, so a symlinked `reports` folder cannot redirect the
  write elsewhere. The CLI's `--output` is the user's explicit choice and is
  written as given, as `dead-code --output` is today.
- **The reports index trusts nothing it reads.** Meta values and file names from
  `reports/` are escaped like repository text. A file without the meta tags is
  listed by name only.
- **Opening the browser.** Only a path the process has just written is opened. It
  is passed as an argument to `open`, `xdg-open` or `explorer`, never through a
  shell. The process does not wait for the browser, and a failure is a warning,
  never an error.
- **Code in agent output is data.** The `group` depth returns repository source to
  the agent. The tool description should say that member code is content to
  analyze, not instructions. No current tool says this, so it would be the first.
- **Precedent.** `crates/mct-mcp-server/src/graph_render.rs` already treats indexed
  paths as untrusted and escapes them for DOT and Mermaid. The HTML renderer
  follows the same rule.

#### Acceptance criteria

- [ ] `dead-code` without `--output` writes `.mct-index/reports/dead-code.csv`,
      creating the folder; a test covers the default path.
- [ ] `duplicates --format html` writes `.mct-index/reports/duplicates.html`
      and prints its path; `--format html --output <file>` writes there instead.
- [ ] Writing an HTML report rebuilds `.mct-index/index.html`. With only
      `duplicates.html` present, the index links to it and shows the command
      for the missing dead-code page; a test covers both states.
- [ ] A report generated before the last reindex is marked as older than the
      index.
- [ ] Every report page and every row of the index shows the last update as a
      date and time. The `<time>` element carries the UTC value, so it is readable
      with scripts disabled (test on the rendered HTML).
- [ ] A foreign `reports/evil.html` without meta tags is listed by escaped file
      name only (test).
- [ ] Every report page links to `../index.html` and to the siblings that existed
      when it was written.
- [ ] `--open` with `--format html` calls the platform opener with the written
      path. A missing opener prints a warning and exits 0. `--open` without
      `--format html` is rejected. Tests replace the opener with a stub; none
      launches a real browser.
- [ ] `duplicates` without `--format` prints exactly today's TOON (golden test).
- [ ] The page has no external URL and passes an HTML parser check. Each group
      shows the template, the hole table and every member's `path:start-end`.
- [ ] Source containing `</script>`, `<`, `&` and quotes renders as text (test).
- [ ] A group whose members differ only in comments still gets a template (test).
- [ ] A write never leaves a partial file: the report is renamed into place.
- [ ] `find_duplicates` returns `summary`, `index` and `group` in TOON and text,
      rejects an unknown `depth`, and requires `group` exactly when `depth` is
      `group`.
- [ ] `find_duplicates` writes nothing unless `html: true`, and then only
      `.mct-index/reports/duplicates.html` and `.mct-index/index.html`. It returns
      both as `file://` links, and rejects `open: true` without `html: true`.
- [ ] `COMMANDS.md`, `website/content/docs/cli.mdx`, the README tool table,
      `CHANGELOG.md` and ADR-013 are updated; `scripts/unix/check.sh` passes.

#### Out of scope

Each is its own issue (recommendations 2, 3, 6 and 7 above):

- Area classification (production, tests, examples, corpus) and area filters.
- Ranking by copy count.
- Stable group ids. Until then, `group` and the page anchors use ranks, which shift
  between runs.
- `--format text` for the CLI's stdout.
- Any change to the fields of today's TOON.
- SARIF and the new-code view.

#### Open questions for the maintainer

1. **Where the renderers live.** The TOON renderer is local to `mct-cli` on
   purpose (its module doc explains why), but the MCP tool needs the same
   renderers. Options: a `render` module in `mct-index`, which both binaries already
   depend on, or a new small crate. A module is the smaller change.
2. **How many groups the page shows.** Rendering every filtered group gives a
   complete page (~460 KB for 183 groups here) but computes evidence for all of
   them. Alternative: honor `--limit`, or cap at a fixed number and say so on the
   page.
3. **Whether the MCP tool should write the page by default.** The draft says no
   (opt-in `html: true`), so that calling a query tool never writes files unasked.
   The same goes for `open`: an MCP client could run the server on another
   machine or in a container, where opening a browser does nothing useful.
4. **The old `dead-code` file.** Leave it silently, or mention it once when it
   exists.

#### Files likely touched

| File | Change |
|---|---|
| `crates/mct-index/src/duplicates/analysis/evidence.rs` | Byte range on `Slot` |
| `crates/mct-index/src/duplicates/template.rs` (new) | `group_template` |
| `crates/mct-index/src/duplicates.rs` | Public `GroupTemplate`, `Segment`, `Hole` |
| `crates/mct-cli/src/duplicates.rs` | `--format toon/html`, `--output`, `render_html` |
| `crates/mct-cli/src/duplicates/report-shell.html` (new) | Page shell: CSS, CSP, filter script |
| `crates/mct-cli/src/reports.rs` (new), or the shared renderer module | Report paths, `write_report`, `rebuild_reports_index`, `navigation_header`, `open_in_browser` |
| `crates/mct-cli/src/main.rs` | `dead-code` default path and help text, `--open` |
| `crates/mct-mcp-server/src/server.rs` | `FindDuplicatesArgs`, `find_duplicates` |
| `crates/mct-cli/tests/duplicates.rs`, `crates/mct-mcp-server/tests/` | Tests above |
| `COMMANDS.md`, `website/content/docs/cli.mdx`, `README.md`, `CHANGELOG.md` | Docs |
| `docs/01-architecture/adrs/013-*.md` (new) | ADR |

### Issue draft: dead-code report

This second issue depends on the first one. It reuses the `reports/` folder,
`write_html_report`, the reports index, the navigation header, `--open` and the HTML
page shell. Once `dead-code.html` exists, the index and the duplicates page link to
it with no change to their code.

**Title:** `feat(dead-code): explain each candidate, with an HTML report for people and TOON for agents`

#### Problem

`find_dead_code_candidates` (`crates/mct-index/src/dead_code.rs`) flags a symbol
when all of the following hold:

- its kind is a function, class, struct, interface, enum, trait or type alias;
- it is not named `main`;
- it does not look like a test;
- no indexed reference anywhere in the project has its name.

The output is a bare list: CSV from the CLI, text or TOON from the MCP tool. Nothing
says why a candidate might still be in use, so a person or an agent has to check
every row by hand.

Measured on this repository (`dead-code` at `71ec6d0`): 181 candidates in 59 files.

| Area | Candidates |
|---|---|
| Production | 98 |
| Labeled corpus (`benchmarks/token-twins-labels/`, intentional) | 75 |
| Examples | 8 |

A rough classification of the 106 that are not corpus, by searching each name in
the repository's sources (an approximation outside the tool, not a measurement of
it):

| Probable reason it is still used | Candidates | Example |
|---|---|---|
| Used as a type; type uses are not indexed as references | 80 | `SymbolAlias` in `mct-core/src/lib.rs`, `Fingerprint` in `duplicates/analysis/fingerprint.rs` |
| Framework entry point (Next.js `page`/`layout`, `generate*`) | 10 | `HomePage`, `generateStaticParams` in `website/app/` |
| Called, but the call is not counted (cause not investigated) | 8 | `semantic_hits`, called as `format::semantic_hits(...)` in `mct-mcp-server/src/server.rs` |
| Named without a call: passed as a value or exported | 6 | `score_verdicts`, passed as `safely(score_verdicts, ...)` |
| No other occurrence of the name | 2 | `DefaultSearchDialog` (a default export), `MDXProvidedComponents` (a type inside a global declaration) |

None of the 106 is clearly dead. A plain list of these candidates presented in a
polished page would only make the false positives look more trustworthy.

#### Proposal

1. **Explain each candidate**, in `mct-index`, so the CLI and the MCP tool share it:
   - **Area:** production, example or corpus (tests are already excluded).
   - **Hint:** why it may still be used, when the tool can tell. Each hint comes only
     from data the tool already has:
     - `framework_entry_point`: a path and name rule. Next.js `page`, `layout`,
       `route`, `template`, `loading`, `error` and `not-found` files exporting a
       component or a `generate*` function.
     - `named_in_file`: the name appears again as an identifier in its own file. It
       uses the token sources, which exist for Rust and Python today. This covers
       the largest bucket, private types used in their own module.
     - `exported`: the declaration is plain `pub` in Rust (not `pub(crate)`),
       `export` in JS/TS, or listed in `__all__` in Python. It may be public API
       with no caller inside the repository.
   - **Confidence:**
     - `low`: a hint applies, so the symbol is probably in use.
     - `medium`: no hint was found, but the language has no token source, so the
       tool cannot check further.
     - `high`: no hint, and the token source shows the name nowhere else in its file.
2. **Two formats, plus the CSV kept as it is:**
   - **People:** `dead-code --format html` writes
     `.mct-index/reports/dead-code.html`, rebuilds the reports index, and with
     `--open` opens the page.
   - **Agents:** `dead-code --format toon` prints TOON on stdout, and
     `find_dead_code` returns the same TOON.
   - **Spreadsheets:** `dead-code` without `--format` (or with `--format csv`) writes
     the CSV as today, with the new columns appended at the end.
3. **Order by confidence:** `high` first, then `medium`, then `low`. Within a level,
   the order is by path and line, as today.
4. **One command for every report.** `mct-cli --root . report` writes
   `duplicates.html` and `dead-code.html` with their default options and rebuilds
   `index.html`. With `--open` it opens `index.html`. `--path` and `--language` pass
   through to both reports. The command lives here because it needs both pages.

#### The HTML page, for people

- **Header:** project, last update (date and time, shown as on the duplicates
  page), index status, scope (`--path`, `--language`), the heuristic in one
  sentence, and the regenerate command.
- **Summary:** candidates by area and by confidence, and how many each hint
  explains.
- **Table:** grouped by directory, then file. Each row shows name, kind,
  `path:line`, confidence and hint in plain words, for example "appears 4 more times
  in this file; probably used as a type".
- **Detail on demand:** a `<details>` row with the signature and up to 15 source
  lines.
- **Default view:** `high` and `medium` open, `low` collapsed, and the corpus
  collapsed. Totals always count everything.
- **Same rules as the duplicates page:** self-contained, CSP without network,
  everything escaped, no JSON payload.

#### The TOON output, for agents

Short and ordered for the agent: the stable lines first (scope, heuristic, totals),
then the rows to act on, most likely dead first. No source unless asked:

```text
scope: .
heuristic: "zero indexed references for this name anywhere in the project; verify before deleting"
total: 181
by_confidence[3]{confidence,candidates}:
  high,2
  medium,...
  low,...
by_hint[3]{hint,candidates}:
  framework_entry_point,10
  named_in_file,...
  exported,...
candidates[50]{confidence,area,kind,name,path,line,end_line,hint}:
  high,prod,function,<name>,<path>,<line>,<end_line>,-
  ...
  low,prod,struct,<name>,<path>,<line>,<end_line>,named_in_file:4
next_offset: 50
```

The counts marked `...` are what the implementation will report. Only the bucket
sizes measured above are known today, and they are not hint counts.

#### Pseudocode

Candidate evidence, `crates/mct-index/src/dead_code.rs`:

```text
enum Area       { Production, Example, Corpus }
enum Hint       { FrameworkEntryPoint, NamedInFile(usize), Exported }
enum Confidence { High, Medium, Low }

struct DeadCodeCandidate {
    entry: SymbolListEntry,          // unchanged fields: name, kind, path, lines, language
    area: Area,
    hints: Vec<Hint>,
    confidence: Confidence,
}

fn explain_candidates(index, candidates, token_sources) -> Vec<DeadCodeCandidate>:
    by_file = group candidates by relative_path
    for (path, group) in by_file:
        tokens = token_sources.get(language(path)).map(|src| src.tokenize(read(path)))
        for c in group:
            hints = []
            if is_framework_entry_point(path, c.name): hints.push(FrameworkEntryPoint)
            if let Some(tokens) = tokens:
                n = tokens.count(|t| t.is_identifier() && t.text == c.name) - 1  // minus the definition
                if n > 0: hints.push(NamedInFile(n))
            if is_exported(c): hints.push(Exported)
            confidence = match (hints.is_empty(), tokens.is_some()):
                (false, _)    -> Low
                (true, true)  -> High
                (true, false) -> Medium
            yield DeadCodeCandidate { entry: c, area: area_of(path), hints, confidence }
    sort by (confidence, path, line)

fn is_framework_entry_point(path, name) -> bool:
    // Next.js app router: the framework imports these files by convention.
    file_stem(path) in ["page", "layout", "route", "template", "loading", "error", "not-found"]
        && path.contains("/app/")
        && (starts_with_uppercase(name) || name.starts_with("generate"))
```

A file that cannot be read or tokenized gives no `NamedInFile` hint and is reported
with `medium` confidence. It never fails the whole report.

CLI flow, `crates/mct-cli/src/main.rs`:

```text
fn dead_code_report(index, path, language, output, format):
    candidates = explain_candidates(index, find_dead_code_candidates(index, path, language)?, token_sources)
    match format:
        Csv  -> path = output.unwrap_or(default_report_path(root, DeadCodeCsv))
                write_report(path, render_csv(candidates))
                finish(path, open: false)          // a CSV is not opened in a browser
        Html -> page = render_dead_code_html(candidates, meta, navigation_header(root, "dead-code"))
                path = write_html_report(root, DeadCode, output, page)?   // rebuilds index.html
                finish(path, open)
        Toon -> print(render_dead_code_toon(candidates, offset: 0, limit: None))
    // --open is rejected with Csv and Toon, as with duplicates.
```

The `report` command, `crates/mct-cli/src/main.rs`:

```text
fn report_all(root, path, language, open):
    index = Index::open_existing_read_only(...)
    written = []
    // Each report is independent: one failing does not stop the other.
    for kind in [Duplicates, DeadCode]:
        match write_default_html(root, &index, kind, path, language):
            Ok(file) -> written.push(file); println("Wrote {file}")
            Err(e)   -> eprintln("warning: {kind} report failed: {e}")
    rebuild_reports_index(root, &index)?
    home = root / REPORTS_HOME
    println("Reports index: {home}")
    if open: finish(home, open: true)
    exit status: 0 if every report was written, 1 otherwise
```

`find_dead_code` keeps its arguments and gains no file write. Its TOON and text
output carry the new columns and the summary lines, so this is a change to a
published tool's output, which this issue makes explicit.

#### Compatibility

- **CSV:** the existing seven columns keep their names and order; `area`,
  `confidence` and `hint` are appended. A script reading columns by position keeps
  working.
- **`find_dead_code` output** gains columns and summary lines. Its arguments are
  unchanged.
- **Candidate set unchanged.** The same symbols are reported. Hints only explain
  them and change the order, never filter them.

#### Acceptance criteria

- [ ] On a fixture with a private struct used only in its own module, the struct is
      reported with `named_in_file` and `low` confidence.
- [ ] A Next.js `app/**/page.tsx` default export gets `framework_entry_point`.
- [ ] A function with no other occurrence in a Rust file gets `high` confidence; the
      same case in a language without a token source gets `medium`.
- [ ] The candidate set equals today's on the same index (test).
- [ ] `report` writes both pages and `index.html`. With `--open` it opens the index.
      If one report fails, the other is still written and the exit status is 1.
- [ ] After `dead-code --format html`, `index.html` lists both reports, and the
      dead-code page links to `duplicates.html` when that page exists.
- [ ] `--format html` writes `.mct-index/reports/dead-code.html`, with no
      external URL, escaped source, and `low` rows collapsed.
- [ ] `--format toon` prints the summary and the table, ordered by confidence.
- [ ] The CSV keeps its first seven columns unchanged.
- [ ] `find_dead_code` returns the new columns in TOON and text, and its
      description states what each confidence level means.
- [ ] Docs (`COMMANDS.md`, `website/content/docs/cli.mdx`, README tool table) and
      `CHANGELOG.md` are updated; `scripts/unix/check.sh` passes.

#### Out of scope

- **Indexing type uses as references.** It would fix the largest bucket (80 of 106
  here) at the source, in the language parsers, but it changes what every language
  crate records. That needs its own issue, and the maintainer may prefer it first.
- **Uncounted qualified calls** such as `format::semantic_hits(...)`. Their cause
  needs investigation before a fix.
- **Hiding or filtering candidates by hint.** The report explains; it does not
  decide.

#### Open questions for the maintainer

1. Fix type references in the parsers first, which would make many `named_in_file`
   hints unnecessary, or ship the hints first because they help every language now?
2. Should the framework entry-point rules be configurable (in `.mctignore` or a
   new setting), or only built in for known frameworks?
3. Is `exported` a hint (keep the candidate, lower its confidence) or a filter
   option (`--exclude-exported`) for libraries whose public API has no in-repo
   callers?

#### Files likely touched

| File | Change |
|---|---|
| `crates/mct-index/src/dead_code.rs` | `DeadCodeCandidate`, `explain_candidates`, entry-point rules |
| `crates/mct-cli/src/main.rs` | `dead-code --format csv/html/toon`, `--open`, new CSV columns, the `report` command |
| `crates/mct-cli/src/dead_code_html.rs` (new) or the shared renderer module | HTML page |
| `crates/mct-mcp-server/src/format.rs` | `find_dead_code` and `find_dead_code_toon` columns and summary |
| `crates/mct-mcp-server/src/server.rs` | Pass token sources to the explanation step |
| Tests in `crates/mct-index`, `crates/mct-cli`, `crates/mct-mcp-server` | Acceptance criteria above |
| `COMMANDS.md`, `website/content/docs/cli.mdx`, `README.md`, `CHANGELOG.md` | Docs |

### Issue draft: refresh from the page

This third issue depends on the first two. It reuses the page shell, the
`mct-report` meta tags, `write_html_report`, the reports index and the `report`
command. It is kept separate because it adds the only network listener in this
proposal, which deserves its own review.

**Title:** `feat(reports): refresh the reports from the page, with live progress`

#### Problem

- A report page is a snapshot. After editing code, the person has to go back to
  the terminal, remember the command, run it and reload the page.
- The page does not say whether it is still current. It shows when it was
  generated, but not whether the code or the index changed since.
- A page opened from disk (`file://`) cannot start a process. A button in the
  static file alone cannot refresh anything.
- Reindexing and the analyses report no progress today. Neither `mct-cli` nor
  `mct-index` has any progress output, so a long refresh would look frozen.

#### Proposal

1. **`mct-cli --root . report --serve [--open]`** writes the reports as `report`
   does, then serves `.mct-index/` on `127.0.0.1` at a random free port and prints
   the URL. With `--open` it opens that URL instead of the file. It stops on Ctrl+C,
   or after 30 minutes without a request.
2. **A toolbar on every page**, added to the shared page shell. It shows the last
   update as a date and time, a **Refresh** button that is always visible, and one
   line explaining the button's state.
3. **The button is enabled only when there is something to refresh.** The server
   checks whether the page is out of date. When nothing changed, the button stays
   visible but disabled, and the line next to it says "No changes since the last
   update". The server enforces the same rule: a refresh request with nothing
   changed gets `409 Conflict`.
4. **A progress panel.** A refresh runs in the background. The server streams each
   phase and its counters to the page with Server-Sent Events. The page shows them
   step by step until the refresh is complete, then loads the new report.
5. **A progress interface in `mct-index`** for the reindex and both analyses. Today's
   functions keep their signatures and report no progress, so every current caller,
   the MCP server included, is unchanged.

#### Toolbar states

| State | How the page knows | Button | Text next to it |
|---|---|---|---|
| Opened from disk | `location.protocol` is `file:` | Visible, disabled | "Last updated … · To refresh from this page, run `mct-cli --root . report --serve --open`" |
| Checking | Waiting for `/status` | Visible, disabled | "Last updated … · Checking for changes…" |
| Up to date | `/status` returns no reasons | Visible, disabled | "Last updated … · No changes since the last update" |
| Changes found | `/status` returns reasons | Enabled | "Last updated … · 3 functions changed in 2 files since the last update", with a "What changed" list (see [Which functions changed](#which-functions-changed)) |
| Not generated yet | The report is missing from the index | Enabled, labeled "Generate" | "This report has not been generated yet" |
| Refreshing | Events stream is open | Disabled, labeled "Refreshing…" | Progress panel |
| Failed | `failed` event | Enabled, labeled "Try again" | The error. The previous report stays on screen |
| Done | `done` event | Not shown: the page reloads | The new page shows the new last update, and the button returns to "Up to date" |
| Server stopped | A request fails or the stream closes early | Visible, disabled | "The report server stopped. Run `mct-cli --root . report --serve --open` again" |

- **The reason is always visible text**, not only a tooltip. A disabled button
  cannot receive keyboard focus, so a tooltip alone would hide the reason from
  keyboard and screen-reader users. The button points to that line with
  `aria-describedby`.
- **State changes are announced.** The status line and the progress panel are
  `aria-live="polite"` regions.
- **The status is checked again** when the tab becomes visible and every 30 seconds
  while it is visible. A change made in the editor therefore enables the button
  without reloading the page.

#### What counts as a change

The server compares the page with the current state. Each reason it finds is shown
on the page:

| Reason | Check |
|---|---|
| `files_changed` | A file on disk was added, changed or removed since the last reindex. The same walk, exclusions and git blob hash as `reindex`, compared with `Index::file_hashes()`, without writing anything |
| `index_changed` | The index changed after the page was written, for example a reindex by the MCP server. The page records a fingerprint of `Index::file_hashes()` and the server compares it with the current one |
| `tool_version` | The page was written by another `mct-cli` version, whose output may differ |
| `report_missing` | The report in `KNOWN_REPORTS` has not been generated yet |

The page's own `--path`, `--language` and normalization options are reused for the
refresh, so the same options never count as a change. Hashing every file on each
check would be slow on a large repository. The server therefore hashes once at
start, then watches the project with `notify` (already used by `mct-mcp-server`)
and rehashes only the paths it reports. A detected change is pushed to open pages
as a `changes` event.

Two meta tags are added to the shell: `mct-report-index-fingerprint` and
`mct-report-tool-version`. Pages written before this issue lack them and count as
`tool_version`, so they can always be refreshed.

#### Which functions changed

A file name alone does not say much. For every changed file the page also lists
the symbols that changed, by name:

```text
What changed since the last update (3 functions in 2 files)
  crates/mct-index/src/duplicates/analysis.rs
    modified  fn group_candidates        lines 120-188
    added     fn merge_buckets           lines 190-214
  crates/mct-cli/src/main.rs
    modified  fn dead_code_report        lines 640-702
    changes outside indexed symbols
  website/app/page.tsx
    removed   function HomeHero
```

How it is computed:

1. **The index keeps one hash per symbol, for its own bytes.** A migration adds a
   nullable `symbols.body_hash` column. It holds the hash of the bytes from the
   symbol's start to its end, with the ranges of its child symbols left out, so an
   edit inside a method changes the method's hash and not its class's. The same
   migration adds `files.outside_hash`, the hash of the file with every top-level
   symbol left out. `reindex` fills both from the ranges the parser already
   returns. Like earlier migrations, it empties every `content_hash` so that the
   next reindex fills the columns for every file.
2. **Changed files are parsed again, read-only.** The file on disk goes through the
   same `LanguageParser` as `reindex`, and nothing is written. Only files whose
   blob hash changed are parsed, so the cost grows with the edit, not with the
   repository.
3. **Symbols are matched by kind, parent and name**, and the body hashes are
   compared:
   - same key and same hash: unchanged, even if the symbol moved to other lines
     because code above it changed;
   - same key and another hash: `modified`;
   - only on disk: `added`;
   - only in the index: `removed`.

   A rename appears as one `removed` and one `added`. Telling a rename apart is out
   of scope.
4. **Only the innermost symbol is listed.** Because each hash covers only the
   symbol's own bytes, an edit inside a method lists the method, not its class as
   well. A class is listed only when its own bytes changed, for example a new field
   in a language that does not index fields.
5. **Changes outside indexed symbols.** When `outside_hash` differs, the file shows
   "changes outside indexed symbols". Examples are imports, comments between
   functions, and module-level variables the language does not index.
6. **Files the tool cannot parse** (no parser for the extension, or a parse error)
   are listed by name only, with the reason.
7. **Lists are capped.** At most 20 symbols per file and 200 in total, then "+N
   more". The full list is in `/status` for whoever needs it.

Which symbols each language indexes today, which decides what the list can name:

| Symbol | Indexed in |
|---|---|
| Functions, methods, classes, structs, interfaces, enums, traits, type aliases | Every language with a parser |
| Constants (`const`, `static`) | Rust, C++, C#, PHP |
| Fields | C++, C#, Java, JavaScript/TypeScript, Kotlin, PHP |
| Variables | Bash, C++, PowerShell only |
| Local variables (`let`, Python assignments) | No language |

In Rust and Python, then, a change to a module-level variable or to a local
variable is reported as a change in its enclosing function, or as "changes outside
indexed symbols" when there is no enclosing function. Indexing more variables is a
parser change for its own issue. When it lands, the list names them with no change
to this code.

#### Progress

A refresh runs these phases, in order:

| # | Phase | Counter |
|---|---|---|
| 1 | Reindex changed files | files done / files to reindex |
| 2 | Duplicates: tokenize | files done / files |
| 3 | Duplicates: group candidates | none (indeterminate bar) |
| 4 | Duplicates: align evidence | groups done / groups |
| 5 | Dead code: find and explain candidates | files done / files with candidates |
| 6 | Write the pages and rebuild `index.html` | pages written / pages |

- **The panel lists every phase** as pending, running (with a bar, the counter and
  the elapsed time), done (with its duration), or failed. A line above shows
  "Step 2 of 6" and the total elapsed time.
- **Progress advances step by step, but results appear only when the refresh is
  complete.** A duplicate group is not final until every file has been compared,
  and the ranking depends on all groups. A partial list would show results that
  later change or disappear. Until the end, the old report stays visible, dimmed,
  under the panel.
- **Writes are atomic.** Each page is written to a temporary file and renamed, as in
  the first issue. A refresh that fails or is interrupted leaves the previous pages
  intact.
- **Event format.** Server-Sent Events carry one `key=value` line per event, for
  example `data: phase=2 done=120 total=812`. The page parses that line. This
  follows the `rules.md` ban on new JSON output.

#### Pseudocode

Progress interface, `crates/mct-index/src/progress.rs` (new):

```text
pub trait Progress: Sync {
    fn phase(&self, id: Phase, total: Option<u64>);   // None: no counter, indeterminate bar
    fn advance(&self, n: u64);
}

pub struct NoProgress;                                  // every method is a no-op
impl Progress for NoProgress { ... }

// Existing entry points keep their signatures and delegate:
pub fn reindex(index, root, options) -> Result<ReindexReport>:
    reindex_with_progress(index, root, options, &NoProgress)

pub fn find_duplicate_groups(index, sources, options) -> Result<DuplicateReport>:
    find_duplicate_groups_with_progress(index, sources, options, &NoProgress)

// Inside the loops that already exist:
progress.phase(Tokenize, Some(files.len()))
for file in files:
    ...unchanged work...
    progress.advance(1)
```

Read-only change detection, `crates/mct-index/src/indexer.rs`:

```text
pub struct PendingChanges { added: Vec<String>, changed: Vec<String>, removed: Vec<String> }

pub fn pending_changes(index, root) -> Result<PendingChanges>:
    // Same walk, exclusions and blob hash as reindex. Writes nothing.
    indexed = index.file_hashes()? as map path -> hash
    on_disk = walk_indexable_files(root)                // shared with reindex
    for path in on_disk:
        hash = Oid::hash_object(Blob, read(path))
        match indexed.get(path):
            None                 -> added.push(path)
            Some(h) if h != hash -> changed.push(path)
            _                    -> {}
    removed = indexed.keys() not in on_disk

pub enum SymbolChange { Modified, Added, Removed }

pub struct FileChange {
    path: String,
    symbols: Vec<(SymbolChange, SymbolKind, String /* parent::name */, Option<LineRange> /* on disk */)>,
    outside_symbols: bool,
    unparsed: Option<String>,        // why the file could not be parsed, if so
}

pub fn changed_symbols(index, root, parsers, path) -> Result<FileChange>:
    // Read-only: parses the file on disk, writes nothing.
    (old, old_outside) = index.symbols_with_body_hash(path)?   // key -> body_hash, plus files.outside_hash
    bytes = read(path)
    parsed = match parsers.for_path(path):
        None    -> return FileChange::unparsed(path, "no parser for this extension")
        Some(p) -> match p.parse(bytes):
            Err(e)  -> return FileChange::unparsed(path, e)
            Ok(pf)  -> pf.symbols
    // The same two functions reindex uses to fill the columns:
    new         = parsed.map(|s| (key(s), own_hash(bytes, s, children_of(s)), s.lines))
    new_outside = outside_hash(bytes, top_level(parsed))
    key(s) = (s.kind, s.parent, s.name)
    for k in new.keys() union old.keys():
        match (old.get(k), new.get(k)):
            (Some(a), Some(b)) if a.hash != b.hash -> push(Modified, k, b.lines)
            (None,    Some(b))                     -> push(Added,    k, b.lines)
            (Some(_), None)                        -> push(Removed,  k, None)
            _                                      -> {}
    outside_symbols = old_outside != new_outside
    return FileChange { path, symbols, outside_symbols, unparsed: None }

fn own_hash(bytes, symbol, children) -> Hash:
    // The symbol's range minus its children's ranges: a child's edit never changes its parent.
    hash(bytes[symbol.start .. symbol.end] with every child range removed)
```

The status the page reads, `/status?report=duplicates`, in TOON:

```text
reasons: files_changed
files_changed: 2
symbols_changed: 3
changes[4]{path,change,kind,symbol,lines}:
  crates/mct-index/src/duplicates/analysis.rs,modified,function,group_candidates,120-188
  crates/mct-index/src/duplicates/analysis.rs,added,function,merge_buckets,190-214
  crates/mct-cli/src/main.rs,modified,function,dead_code_report,640-702
  crates/mct-cli/src/main.rs,outside_symbols,-,-,-
```

Every name and path goes through `escape_html` before the page shows it, like any
other repository text.

Server, `crates/mct-cli/src/serve.rs` (new):

```text
enum Job { Idle, Running { phase, done, total, started }, Failed(String) }

struct Server {
    root, port,
    token: [u8; 32],                 // random per run, from the OS
    job: Mutex<Job>,                 // at most one refresh at a time
    pending: Mutex<PendingChanges>,  // hashed at start, then kept current by the watcher
    events: Broadcast<String>,       // "key=value" lines sent to every open /events stream
}

fn serve(root, open):
    report_all(root, ...)                                   // the pages exist before serving
    listener = TcpListener::bind("127.0.0.1:0")             // never 0.0.0.0
    server = Server::new(root, listener.port(), random_token())
    server.pending = pending_changes(index, root)
    watch(root, excluding .mct-index/, on_change = |paths| {
        server.pending.update(rehash(paths))
        server.events.send("changes=" + server.pending.count())
    })
    url = "http://127.0.0.1:{port}/?t={hex(token)}"
    println("Serving reports at {url}  (Ctrl+C to stop)")
    if open: open_in_browser(url)
    for request in listener with a 30-minute idle timeout:
        handle(server, request)

fn handle(server, req):
    // Checked before any route:
    require(req.host == "127.0.0.1:{port}" or "localhost:{port}") else 421   // DNS rebinding
    require(authenticated(req)) else 403
    match (req.method, req.path):
        (GET,  "/")                      -> serve_page(REPORTS_HOME)
        (GET,  "/reports/{name}.html")   -> name in KNOWN_REPORTS ? serve_page(...) : 404
        (GET,  "/status")                -> status_lines(server, req.query.report)
        (GET,  "/events")                -> sse_stream(server.events)
        (POST, "/refresh")               -> start_refresh(server, req)
        _                                -> 404

fn authenticated(req) -> bool:
    // First visit: the token in the URL is exchanged for a cookie, then the server
    // redirects to the same URL without it, so the token leaves the address bar.
    if req.query.t == hex(token): set_cookie("mct=…; HttpOnly; SameSite=Strict; Path=/"); redirect
    return constant_time_eq(req.cookie("mct"), hex(token))

fn start_refresh(server, req):
    require(req.header("Origin") == "http://127.0.0.1:{port}") else 403      // cross-site request
    require(req.header("X-MCT-Refresh") == "1") else 403                     // forces a CORS preflight, which the server never approves
    reasons = change_reasons(server)                     // files_changed, index_changed, tool_version, report_missing
    if reasons.is_empty(): return 409 "up to date"       // the server enforces the disabled button
    lock job: if Running: return 409 "already refreshing"
    job = Running
    spawn thread:
        progress = EventProgress(server.events)          // implements Progress, sends "phase=… done=… total=…"
        result = refresh_all(root, &progress)            // reindex_with_progress, then both analyses with progress, then write pages
        match result:
            Ok  -> server.pending = pending_changes(...); events.send("done=1")
            Err -> job = Failed(e); events.send("failed=" + one_line(e))
        job = Idle (or Failed)
    return 202

fn serve_page(path):
    // Pages on disk forbid every connection. Served pages may reach this server only:
    // replace the shell's known CSP line, and send the same policy as a header.
    page = read(path).replace(CSP_FILE_LINE, CSP_SERVED_LINE)   // CSP_SERVED adds connect-src 'self'
    respond(200, page, headers: CSP_SERVED, Cache-Control: no-store)
```

Page script, in the shared shell:

```text
on load:
    show_last_update(meta("mct-report-generated-at"))   // local time + "2 hours ago", every minute
    if location.protocol == "file:":
        set_state(OpenedFromDisk); return
    set_state(Checking)
    check_status()
    every 30 s while visible, and on visibilitychange: check_status()
    events = new EventSource("/events")
    events.on("changes")  -> check_status()
    events.on("phase")    -> update_panel(parse_key_values(data))
    events.on("done")     -> location.reload()
    events.on("failed")   -> set_state(Failed, data.failed)
    events.on("error")    -> set_state(ServerStopped)

fn check_status():
    lines = fetch("/status?report=" + this_report).text()   // "reasons=files_changed,index_changed" + "files=3"
    set_state(lines.reasons.empty ? UpToDate : ChangesFound(lines))

on refresh click:
    set_state(Refreshing)
    response = fetch("/refresh", POST, headers: {"X-MCT-Refresh": "1"})
    if response.status == 409: check_status()            // changed back, or another tab started it
```

#### Compatibility

- **Nothing listens by default.** Only `report --serve` opens a socket. `duplicates`,
  `dead-code` and `report` without `--serve` behave as in the first two issues.
- **Existing `mct-index` signatures are unchanged.** The `*_with_progress` variants
  are new, and `NoProgress` keeps every current caller's behavior.
- **Pages on disk keep the strict CSP** of the first issue. The toolbar shows the
  last update and a disabled button with the command, and the script makes no
  request.
- **The MCP server is unchanged.** An agent already calls the tools directly, so it
  gains nothing from a button.
- **Schema migration.** `symbols.body_hash` and `files.outside_hash` are nullable
  and additive. Emptying every `content_hash` makes the first reindex after
  upgrading parse every file again, as earlier migrations did. Until then the
  page falls back to listing changed files only.
- **No new crate, if `getrandom` is accepted as a direct dependency.** It is already
  in `Cargo.lock` as a transitive dependency. `notify` is already a workspace
  dependency. The HTTP server is an open question.

#### Security

- **Loopback only.** The listener binds `127.0.0.1`, never `0.0.0.0`, so nothing on
  the network can reach it.
- **Other local pages are the threat.** Any website open in the browser can send
  requests to `127.0.0.1`. The server is defended by layers:
  - a random 256-bit token per run, exchanged for an `HttpOnly`, `SameSite=Strict`
    cookie and compared in constant time;
  - a strict `Host` check, against DNS rebinding;
  - on `POST /refresh`, an exact `Origin` check and a custom header, so a
    cross-site form or `fetch` cannot trigger it. No CORS headers are ever sent.
- **One fixed action.** `/refresh` takes no parameters. The options come from the
  page's own meta tags, read by the server from the file it wrote, never from the
  request. No request value reaches a path, a command or a shell.
- **Only known files are served.** `/reports/{name}.html` accepts only names in
  `KNOWN_REPORTS`. There is no static file route, so `..` or an absolute path
  cannot read anything else.
- **Bounded work.** One refresh at a time (`409` otherwise), a request size limit,
  a limit on open event streams, and an idle shutdown after 30 minutes.
- **Served pages may connect only to this server** (`connect-src 'self'`). Pages on
  disk keep `connect-src 'none'`.
- **Errors sent to the page are one escaped line**, with no file contents.

#### Acceptance criteria

- [ ] `report --serve` binds `127.0.0.1` on a free port, prints the URL, and with
      `--open` opens it. Ctrl+C and the idle timeout stop it.
- [ ] Every page shows the last update as a date and time, in local time with the
      script and in UTC without it.
- [ ] Opened from disk, the button is visible and disabled, and the text shows the
      `--serve` command.
- [ ] Served with no changes, the button is visible and disabled with "No changes
      since the last update". A `POST /refresh` then returns `409`.
- [ ] Editing an indexed file enables the button within one status check, and the
      text shows the number of changed functions and files. A reindex done
      elsewhere shows `index_changed`.
- [ ] On a fixture, editing one function's body lists it as `modified` with its new
      lines; adding and deleting functions lists `added` and `removed`.
- [ ] Inserting lines above a function moves it without listing it as changed.
- [ ] An edit inside a method lists the method only, not its class.
- [ ] Editing only an import or a Python module-level variable shows "changes
      outside indexed symbols" and lists no function.
- [ ] A file with no parser, or that fails to parse, is listed by name with the
      reason.
- [ ] The migration adds `symbols.body_hash` and `files.outside_hash`, and after one
      reindex every symbol and file has them (test). Computing changes writes nothing to the index (test on
      `PRAGMA data_version`).
- [ ] A refresh streams every phase in order with its counters, keeps the old
      report visible until `done`, then reloads, showing a new last update and a
      disabled button.
- [ ] A failing refresh shows the error, keeps the previous pages byte-identical,
      and enables "Try again".
- [ ] A second refresh while one runs returns `409`, from the same tab or another.
- [ ] Requests without the token cookie, with a foreign `Host`, or a `POST` with a
      foreign `Origin` or without the custom header are rejected (tests for each).
- [ ] `/reports/../index.sqlite3`, an encoded `..`, and unknown report names
      return `404`.
- [ ] `reindex`, `find_duplicate_groups` and `find_dead_code_candidates` produce the
      same results with and without progress (test).
- [ ] Tests stub the browser opener and use a temporary project; none opens a real
      browser.
- [ ] `COMMANDS.md`, `website/content/docs/cli.mdx`, `CHANGELOG.md` and ADR-013 (or
      a new ADR for the local server) are updated; `scripts/unix/check.sh` passes.

#### Out of scope

- **Refresh without a button.** A `--watch` mode that regenerates on every change
  without being asked. The watcher above makes it a small follow-up, but it runs
  the analyses on every save, and that cost needs measuring first.
- **Showing partial results during a refresh**, for the reasons in
  [Progress](#progress).
- **Cancelling a running refresh** from the page.
- **A refresh button in the MCP output.**
- **Remote access** to the reports, from another machine or a container.

#### Open questions for the maintainer

1. **HTTP server.** A minimal server on `std::net::TcpListener` adds no dependency
   but means parsing HTTP by hand, which is a risk of its own. A small crate such as
   `tiny_http` is the alternative. The routes are few either way.
2. **Change check cost.** Is one full hash at start plus `notify` acceptable, or
   should the first check compare modification times only and hash on demand?
3. **ADR.** Does a local listener belong in ADR-013, or in its own ADR? It is the
   project's first network listener.
4. **Port.** Random by default. Is a `--port` flag wanted, for example for a
   bookmark?
5. **Refresh scope.** Should the button refresh only its own report, or always every
   report, as `report` does? Refreshing all keeps the pages consistent, but makes
   the duplicates refresh pay for dead code too.
6. **Per-symbol hash or git.** `content_hash` is a git blob id, so when the indexed
   state of a file was committed, its old content can be read from the repository
   with `git2`, and no schema change is needed. That fails for files indexed with
   uncommitted edits, and outside git. The draft proposes `symbols.body_hash`
   because it always works. Reading git first and keeping the column only as a
   fallback is the alternative.
7. **Is the per-symbol list worth more?** The same `changed_symbols` could later
   tell an agent which functions changed since the last reindex, for example in
   `get_indexing_status`. That would change a published tool, so it is out of scope
   here.

#### Files likely touched

| File | Change |
|---|---|
| `crates/mct-index/src/progress.rs` (new) | `Progress`, `Phase`, `NoProgress` |
| `crates/mct-index/src/indexer.rs` | `reindex_with_progress`, `pending_changes`, `changed_symbols`, `body_hash` on write, shared file walk |
| `crates/mct-index/src/schema.rs` | Migration: `symbols.body_hash`, `files.outside_hash`, empty every `content_hash` |
| `crates/mct-index/src/queries.rs` | `symbols_with_body_hash` |
| `crates/mct-index/src/duplicates/analysis.rs`, `crates/mct-index/src/dead_code.rs` | Progress calls inside the existing loops |
| `crates/mct-cli/src/serve.rs` (new) | Server, routes, authentication, refresh job, event stream, watcher |
| `crates/mct-cli/src/main.rs` | `report --serve` |
| `crates/mct-cli/src/duplicates/report-shell.html` | Toolbar, last update, progress panel, script, CSP placeholder line |
| `crates/mct-cli/src/reports.rs` | New meta tags: index fingerprint, tool version |
| `crates/mct-cli/Cargo.toml` | `notify`, `getrandom`, and the HTTP crate if chosen |
| `crates/mct-cli/tests/serve.rs` (new), `crates/mct-index/tests/` | Acceptance criteria above |
| `COMMANDS.md`, `website/content/docs/cli.mdx`, `CHANGELOG.md`, ADR | Docs |

## Sources

- PMD CPD report formats: https://pmd.github.io/pmd/pmd_userdocs_cpd_report_formats.html
- jscpd README (reporters, `ai` reporter, MCP mode): https://github.com/kucherenko/jscpd
- SonarQube metric definitions (duplications): https://docs.sonarsource.com/sonarqube-server/latest/user-guide/code-metrics/metrics-definition/
- IntelliJ IDEA Duplicates tool window: https://www.jetbrains.com/help/idea/duplicates-tool-window.html
- GitHub SARIF support for code scanning: https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning
- Rieger, Ducasse, Lanza, "Insights into System-Wide Code Duplication" (WCRE 2004): https://www.si.usi.ch/assets/publications/conf/wcre/wcre2004/RiegerDL04.pdf
- Hauptmann et al., "Using Edge Bundle Views for Clone Visualization" (IWSC 2012), with a summary of dotplots, scatter plots and treemaps: https://wwwbroy.in.tum.de/publ/papers/IWSC12Hauptmann.pdf
- cargo-llvm-cov HTML output: https://github.com/taiki-e/cargo-llvm-cov
- coverage.py HTML report: https://coverage.readthedocs.io/en/latest/cmd.html#html-reporting
- MDN, Using server-sent events: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events
- MDN, CSP `connect-src`: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src
- MDN, CORS preflight requests: https://developer.mozilla.org/en-US/docs/Glossary/Preflight_request
- WAI-ARIA Authoring Practices, keyboard focus of disabled controls: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#focusabilityofdisabledcontrols
- Hammad, Basit, Jarzabek, Koschke, "A systematic mapping study of clone visualization", Computer Science Review 2020, doi:10.1016/j.cosrev.2020.100266
- Li, Thompson, clone detection and removal for Erlang by anti-unification: https://www.cs.kent.ac.uk/pubs/2010/2975/index.html
- Liu et al., "Lost in the Middle: How Language Models Use Long Contexts": https://arxiv.org/abs/2307.03172
- Aider, "Unified diffs make GPT-4 Turbo 3X less lazy": https://aider.chat/docs/unified-diffs.html
