# ADR-047: CMake include directories for quoted includes

## Status
Proposed (2026-11-21). Issue: [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (the C/C++ row). Adds schema migration 30, on top of migration 29 ([ADR-046](046-cargo-dependency-renames.md)).

## Context
- `#include "core/api.h"` that the compiler finds through `-I lib/include` named no file. The parser pins a quoted include to the path next to the including file, so the include stayed unresolved and the header looked unused.

## Decision
- **Parser:** a quoted include also stores its spec as written, in `RelationTarget::include_spec` and the new `relations.include_spec` column.
  - A spec that is absolute or holds `.`/`..` segments stores none. It keeps only the file-relative path.
- **Table:** `include_dirs(manifest_path, directory, position, include_path)`.
  - Filled for each `CMakeLists.txt` from literal `include_directories(…)` and `target_include_directories(<target> …)` calls, resolved from that file's directory.
  - Kept: plain relative paths, `${CMAKE_CURRENT_SOURCE_DIR}[/x]`, and `$<BUILD_INTERFACE:…>` around either.
  - Dropped: keywords, absolute paths, `$<INSTALL_INTERFACE:…>`, and any other variable or generator expression. These are dropped silently.
  - The table is swept like the other manifest tables.
- **View:** when no file sits at the file-relative path, `relation_candidates` also matches `<include dir>/<spec>`.
  - Only include dirs from a `CMakeLists.txt` whose directory contains the including file count. This approximates CMake's directory scope; the target a file belongs to is not modelled.
  - A header next to the includer still wins, as the compiler searches its directory first.
  - Several matching include dirs give several candidates, so the include is ambiguous rather than guessed.
- Migration 30 reparses C/C++ files to fill `include_spec`.

## Not done
- `compile_commands.json` `-I` flags.
- Include dirs inherited through `target_link_libraries`, or from a parent target outside the file's directory tree.
- `get_indexing_status` does not list the include dirs read.

## Verification
- `mct-lang-cpp/tests/depgraph.rs` › `a_cmake_include_directory_resolves_a_quoted_include`. It covers:
  - `$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>` resolves `core/api.h`;
  - a header next to the includer beats one in the include dir;
  - a file outside the CMakeLists' directory stays unresolved;
  - removing the call unresolves the include.

  It failed before the change (`left: [(Unresolved, None)]`).
- `indexer::cmake_include_tests::only_literal_include_directories_are_kept`.
