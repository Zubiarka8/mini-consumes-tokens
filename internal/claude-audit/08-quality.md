# Agent 08 — regressions, measurements, and integration review

Follow the shared rules. Branch: `codex/audit-quality-gates`, from the integrated fixes. Goal: close the audit with tests that catch real errors and repeatable measurements.

Add resolved cases for false relations, Lua, changing exclusions, Markdown, and dead code to the quality suite. Penalize false positives as well as checking that expected results appear. Do not change the baseline to normalize an incorrect result. Avoid duplicating identical tests at every layer.

Evaluate R01–R04 with bounded fixtures: deep/malformed AST in a subprocess, high fan-in and pagination, query blocking during semantic search, and read errors. Use reasonable time/memory limits and synthetic data, never credentials or personal files. Classify each measurement as a failure, limitation, or unreproduced risk.

Measure equivalent tasks with MCP versus reading/searching: catalog, arguments, responses, follow-up reads, and retries. Identify the tokenizer and model when measuring actual tokens; if no tokenizer is available, publish bytes or the heuristic with its label. 100% on 27 fixtures does not prove universal accuracy or measured billing savings.

Scope: `mct-eval`, tests and benchmark tools, results documentation, and tracking findings. Do not make production optimizations without measurements that justify them; create a separate task for each risk requiring broad changes.

Acceptance: final suite on the real integration, formatting, Clippy, and evaluation; reproducible results with versions/environment; distinguish sandbox errors from product errors; review diffs for unrelated changes/secrets. Update finding status with real commit/PR references while preserving the original report as a historical snapshot. Deliver a proposed PR and explicit outstanding items.
