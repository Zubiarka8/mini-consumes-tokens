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
- `code-review-skill`, sourced from [awesome-skills/code-review-skill](https://github.com/awesome-skills/code-review-skill), is licensed under the MIT License. Copyright (c) 2025 awesome-skills.

The following MIT License applies to both sources:

## MIT License

Copyright (c) 2025 Addy Osmani
Copyright (c) 2025 awesome-skills

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
