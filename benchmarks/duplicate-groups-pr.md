Title: `fix(dashboard): distinguish duplicate locations and display readable group names`

## Summary

```text
duplicate group
  → server counts: locations + distinct files
  → bounded title / explicit preview omissions
  → English word labels + locations panel
  → full detail → comparison / AI template
```

Show every previewed identifier, collapse repeated visual labels, correct
singular files and describe directory scope plainly. Preserve detector groups,
review keys, saved decisions and exact identifiers in exported evidence.

## Evidence

- **Before:** failing API tests show `alpha ≈ beta` for three members; ES/EN
  DOM tests show `1 archivos` / `1 files`; recent activity has its own cutoff.
- **After:** requested Rust/JS gates pass, 36 React tests pass, real desktop
  groups #1/#3/#6 show 2/1, 3/1 and 5/4 location/file counts. Generated tests
  cover 21 nested layouts and all 256 path partitions for eight locations.

## Merge Danger

**Door:** two-way. No schema or review-file migration.

**Blast Radius:** dashboard.

The existing detail response still scales with group size. Explicit title sorts
can change because titles now describe more members. The internal React type
requires server counts already present in the API. No PR has been opened.
