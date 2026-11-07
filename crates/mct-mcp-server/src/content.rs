//! Text extraction from MCP tool responses.

use rmcp::model::CallToolResult;

/// Join all text blocks with newlines, preserving empty text blocks and their order.
/// Non-text blocks are ignored; a result with no text yields an empty string.
pub fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|text| text.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ContentBlock;

    #[test]
    fn preserves_text_block_order_and_empty_blocks() {
        for (texts, expected) in [
            (vec![], ""),
            (vec!["only"], "only"),
            (vec!["first", "", "last"], "first\n\nlast"),
        ] {
            let result =
                CallToolResult::success(texts.into_iter().map(ContentBlock::text).collect());
            assert_eq!(result_text(&result), expected);
        }
    }
}
