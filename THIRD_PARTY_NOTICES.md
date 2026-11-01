# Third-Party Notices

The project's original code and documentation are licensed under the Apache
License 2.0 (see `LICENSE`). That license does not replace the licenses of third-party
components, including Rust dependencies, bundled libraries, optional embedding
models, and imported skills. Earlier versions remain available under the
licenses under which they were released.

## Rust dependencies

The workspace uses dependencies with their own license terms. For example,
`rmcp`, `rmcp-macros`, and `rusqlite_migration` use Apache-2.0. The optional
semantic-search dependencies include Apache-2.0 components such as `fastembed`,
`hf-hub`, and `tokenizers`. These Apache-2.0 components are not an alternative
license for the project's original code.

Other dependencies include MIT, ISC, BSD, Unicode, and MPL-2.0 components.
Redistributors must retain the applicable upstream license texts and notices
for the components included in their distribution. This document is not a
complete dependency-license inventory; the required set depends on the target,
enabled features, and distributed artifacts.

## Optional embedding models

The default `bge-small-en-v1.5` model is published under MIT;
`all-MiniLM-L6-v2` is published under Apache-2.0. These models are downloaded
separately, and their licenses apply to their weights and associated files.
See their upstream model cards:

- [BAAI/bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5)
- [sentence-transformers/all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)

## Imported skills

The project-local skills in `.agents/skills/` include material from these repositories:

- Skills sourced from [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills) are licensed under the MIT License. Copyright (c) 2025 Addy Osmani.
- An earlier version of this notice attributed `code-review-skill` to [awesome-skills/code-review-skill](https://github.com/awesome-skills/code-review-skill), under the MIT License, copyright (c) 2025 awesome-skills. No corresponding import or lock entry has been identified; this attribution is retained pending provenance review rather than treated as evidence of an import.
- `skills-lock.json` records `extract-design` as sourced from [Manavarya09/design-extract](https://github.com/Manavarya09/design-extract). The upstream repository's [license](https://github.com/Manavarya09/design-extract/blob/main/LICENSE), checked 2026-11-01, is MIT, copyright (c) 2024 Manavarya Singh. The lock does not record an upstream revision, so the license at import time has not been verified.
- The local skill `reduce-duplication` declares `license: CC0-1.0` in its own front matter. Its upstream source is not recorded in this repository.
- Other local skills (for example `frontend-ui-engineering`, `browser-testing-with-devtools`, `performance-optimization`, `spec-driven-development`, `improve-prompt`, `interview-me`, `react-three-fiber`, `ui-ux-pro-max`, `claude-prompt-pm`) are not in `skills-lock.json` and have no recorded upstream source or license; add their origin here when it is established.

The following MIT License text is included for the MIT sources identified above;
the original import revision and other unrecorded skill licenses still require verification:

## MIT License

Copyright (c) 2025 Addy Osmani
Copyright (c) 2025 awesome-skills
Copyright (c) 2024 Manavarya Singh

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
