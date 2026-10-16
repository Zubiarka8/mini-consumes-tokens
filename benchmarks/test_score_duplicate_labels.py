"""Regressions for the ADR-012 budget output consumed by the label scorer."""

import unittest

from score_duplicate_labels import parse_toon, score_partial_blocks


class PartialBlockBudgetTests(unittest.TestCase):
    def test_nested_budget_exhaustion_is_reported(self):
        for value in ("false", "max-windows", "max-comparisons", "max-blocks"):
            with self.subTest(value=value):
                report = parse_toon(
                    "partial_blocks_budget:\n"
                    f"  budget_exhausted: {value}\n"
                    "partial_block_count: 0\n"
                    "partial_blocks[0]:\n"
                )
                result = score_partial_blocks(report, {}, {}, {}, "accepted")
                header = {row["field"]: row["value"] for row in result["header"]}
                self.assertEqual(header["budget_exhausted"], report["partial_blocks_budget"]["budget_exhausted"])
                self.assertNotEqual(header["budget_exhausted"], "absent")

    def test_an_absent_budget_is_still_identified_as_absent(self):
        result = score_partial_blocks({"partial_blocks": []}, {}, {}, {}, "accepted")
        header = {row["field"]: row["value"] for row in result["header"]}
        self.assertEqual(header["budget_exhausted"], "absent")


if __name__ == "__main__":
    unittest.main()
