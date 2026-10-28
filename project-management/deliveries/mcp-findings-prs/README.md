# Draft PR mapping

Create drafts in this order; the base is the preceding branch, starting at #246.
Bodies are ready for gh --body-file. Refresh remote status first.

- [fix(mcp): preserve continuation for partial duplicate groups](01-duplicate-pagination.md): base `feature/mcp-find-duplicate-code`, head `8d06959`.
- [feat(mcp): apply TOON reviews before dead-code pagination](02-dead-code-reviews.md): base `fix/mcp-duplicate-pagination`, head `4b0d012`.
- [fix(mcp): verify indexed source revisions and recovery](03-source-revisions.md): base `feature/mcp-dead-code-reviews`, head `43e77a9`.
- [fix(mcp): pin review fingerprints across live reindexing](04-live-reindex.md): base `feature/mcp-source-revisions`, head `1022c54`.
- [fix(web): keep dashboard queries in one index generation](05-web-generation.md): base `feature/mcp-live-reindex`, head `bf91ccf`.
- [docs: record MCP findings delivery and remaining acceptance](06-delivery.md): base `fix/web-findings-generation`, head `branch HEAD`.
