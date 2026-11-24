---
name: mct-probe
description: "Check whether a file parses, without indexing it. Use to find the first syntax error and its line."
---
Use when: a file fails to index or a grammar seems to reject valid code. It parses without touching the index, and the files need not be under the root.
Returns: per file, either symbol and relation counts or the first syntax error with its line. The exit status is non-zero if any file fails.
1. Run `mct-cli probe <arg>`; several files are allowed and the extension picks the language.
2. Report the symbol counts, or the first syntax error with its line.
3. To find the construct a grammar rejects, probe smaller slices of the file until the error line stops moving.
Do not edit the file unasked.
