"""Date parser tests that differ only in their input data.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never collected or run.
"""


# Case py-amb-tests: parametrize them, or keep each scenario readable on its own.
def test_parse_date_iso():
    parser = DateParser(strict=True)
    result = parser.parse("2024-01-31")
    assert result.year == 2024
    assert result.month == 1
    assert result.day == 31


def test_parse_date_compact():
    parser = DateParser(strict=True)
    result = parser.parse("20240131")
    assert result.year == 2024
    assert result.month == 1
    assert result.day == 31
