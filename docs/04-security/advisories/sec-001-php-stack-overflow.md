# SEC-001: AST-walker stack overflow (unbounded recursion)

- **Status**: Fixed (commit `44a3a25`)
- **Component**: originally caught on `mct-lang-php`; root cause shared by all 17 crates' `Walker::visit`/`visit_children`
- **Severity**: crash on adversarial/pathological input (ASan stack-overflow), not memory-unsafe — a DoS class, not RCE

## Description
CI's `fuzz-smoke` job crashed `mct-lang-php` on deeply nested input (AddressSanitizer stack-overflow). Every crate shared the same unbounded mutual recursion pattern; PHP was just the one the fuzzer reached first.

## Fix
Added `mct_core::MAX_TRAVERSAL_DEPTH` (256) and threaded a depth counter through every crate's walker and its recursive helpers — past the ceiling, `visit()` returns instead of recursing further. Regression test reproduces the original crash shape (5,000 nested parens) and asserts `parse()` still returns `Ok`.

## Related
[[overview]] · [[limits-spec]] · [[mct-lang-php]]

#security #advisory
