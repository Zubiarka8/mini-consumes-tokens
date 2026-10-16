#!/usr/bin/env python3
"""Score `mct-cli duplicates` against the labeled token-twins corpus.

Copies the corpus sources into a temporary project, runs `init` and then
`duplicates` with the given binary and flags, parses the TOON report and scores
it against benchmarks/token-twins-labels/labels.toon. Standard library only;
the corpus is parsed by mct-cli and never executed, and the index is touched
only through mct-cli.

Aspects:
- grouping: whole-function grouping recall per normalization (baseline).
- verdicts: per-group `verdict.level` against the labeled level (confusion
  matrix; `ambiguous` cases are excluded and counted separately).
- subgroups: per-group `subgroups` partition against the labeled families
  (exact-partition rate).
- partial_blocks: top-level `partial_blocks` against labeled blocks
  (precision/recall under a line-overlap rule).
An aspect whose fields are absent from the report is `not-implemented`.
Output is TOON on stdout.
"""

import argparse
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parent.parent
DEFAULT_CORPUS = REPO / "benchmarks" / "token-twins-labels"
LEVELS = ["REAL", "SUSPICIOUS", "FALSE_POSITIVE"]
PREDICTED = LEVELS + ["missing", "not-grouped", "membership-differs"]
SOURCE_SUFFIXES = {".py", ".rs"}
LINE_JACCARD = 0.5


# --------------------------------------------------------------------- TOON


KEY_LINE = re.compile(
    r'^(?P<key>"(?:[^"\\]|\\.)*"|[^\s:\[\]{}"][^:\[\]{}]*?)'
    r"(?:\[(?P<count>[^\]]*)\](?:\{(?P<fields>[^}]*)\})?)?"
    r":(?:[ \t](?P<value>.*))?$"
)


def unquote(text):
    text = text.strip()
    if len(text) >= 2 and text[0] == '"' and text[-1] == '"':
        body, out, i = text[1:-1], [], 0
        if "\\" not in body:
            return body
        while i < len(body):
            char = body[i]
            if char == "\\" and i + 1 < len(body):
                nxt = body[i + 1]
                out.append({"n": "\n", "t": "\t", "r": "\r"}.get(nxt, nxt))
                i += 2
                continue
            out.append(char)
            i += 1
        return "".join(out)
    return text


def split_row(text, delimiter=","):
    if '"' not in text:  # No quoted cell: every delimiter separates cells.
        return [cell.strip() for cell in text.split(delimiter)]
    cells, current, quoted, i = [], [], False, 0
    while i < len(text):
        char = text[i]
        if quoted:
            current.append(char)
            if char == "\\" and i + 1 < len(text):
                current.append(text[i + 1])
                i += 2
                continue
            if char == '"':
                quoted = False
        elif char == '"':
            quoted = True
            current.append(char)
        elif char == delimiter:
            cells.append(unquote("".join(current)))
            current = []
        else:
            current.append(char)
        i += 1
    cells.append(unquote("".join(current)))
    return cells


def toon_lines(text):
    lines = []
    for raw in text.splitlines():
        if not raw.strip():
            continue
        stripped = raw.lstrip(" ")
        lines.append([len(raw) - len(stripped), stripped.rstrip()])
    return lines


def parse_toon(text):
    """Tolerant TOON reader: objects, inline arrays, tables and `- ` lists.

    Unknown line shapes are kept under `_raw` instead of failing.
    """
    lines = toon_lines(text)
    result, _ = parse_object(lines, 0, lines[0][0] if lines else 0)
    return result


def parse_object(lines, i, indent):
    obj = {}
    while i < len(lines):
        depth, text = lines[i]
        if depth < indent:
            break
        if depth > indent or text.startswith("- ") or text == "-":
            obj.setdefault("_raw", []).append(text)
            i += 1
            continue
        match = KEY_LINE.match(text)
        if not match:
            obj.setdefault("_raw", []).append(text)
            i += 1
            continue
        key = unquote(match.group("key"))
        value = match.group("value")
        i += 1
        if match.group("fields") is not None:
            fields = split_row(match.group("fields"))
            rows = []
            while i < len(lines) and lines[i][0] > indent:
                cells = split_row(lines[i][1])
                rows.append(dict(zip(fields, cells + [""] * (len(fields) - len(cells)))))
                i += 1
            obj[key] = rows
        elif match.group("count") is not None:
            if value is not None and value.strip():
                obj[key] = split_row(value.strip())
            elif i < len(lines) and lines[i][0] > indent:
                obj[key], i = parse_children(lines, i)
            else:
                obj[key] = []
        elif value is None or not value.strip():
            if i < len(lines) and lines[i][0] > indent:
                obj[key], i = parse_children(lines, i)
            else:
                obj[key] = ""
        else:
            obj[key] = unquote(value)
    return obj, i


def parse_children(lines, i):
    depth, text = lines[i]
    if text.startswith("- ") or text == "-":
        return parse_list(lines, i, depth)
    return parse_object(lines, i, depth)


def parse_list(lines, i, indent):
    items = []
    while i < len(lines) and lines[i][0] == indent and (lines[i][1].startswith("- ") or lines[i][1] == "-"):
        content = lines[i][1][2:] if lines[i][1] != "-" else ""
        if KEY_LINE.match(content):
            lines[i] = [indent + 2, content]
            item, i = parse_object(lines, i, indent + 2)
            items.append(item)
        else:
            items.append(unquote(content))
            i += 1
    return items, i


def toon_value(value):
    text = str(value)
    if text == "" or re.search(r'[,:"\\\n]|^\s|\s$|^-', text):
        return '"' + text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    return text


def emit_table(out, indent, key, fields, rows):
    pad = " " * indent
    out.append(f"{pad}{key}[{len(rows)}]{{{','.join(fields)}}}:")
    for row in rows:
        out.append(f"{pad}  " + ",".join(toon_value(row.get(field, "")) for field in fields))


def emit_list(out, indent, key, values):
    out.append(f"{' ' * indent}{key}[{len(values)}]: " + ",".join(toon_value(v) for v in values))


# ------------------------------------------------------------------- labels


def load_labels(path):
    labels = parse_toon(Path(path).read_text(encoding="utf-8"))
    cases = {row["id"]: dict(row, members=[]) for row in labels.get("cases", [])}
    for row in labels.get("members", []):
        member = {
            "path": row["path"],
            "function": row["function"],
            "start": int(row["start_line"]),
            "end": int(row["end_line"]),
            "family": row.get("family", "-"),
            "modes": {mode: row.get(mode) == "true" for mode in ("broad", "consistent", "exact")},
        }
        cases[row["case"]]["members"].append(member)
    blocks = {row["id"]: dict(row, occurrences=[]) for row in labels.get("partial_blocks", [])}
    for row in labels.get("partial_block_occurrences", []):
        blocks[row["block"]]["occurrences"].append(occurrence(row))
    negatives = {row["id"]: dict(row, occurrences=[]) for row in labels.get("negative_blocks", [])}
    for row in labels.get("negative_block_occurrences", []):
        negatives[row["block"]]["occurrences"].append(occurrence(row))
    return labels, cases, blocks, negatives


def occurrence(row):
    return {"path": row["path"], "function": row.get("function", ""),
            "start": int(row["start_line"]), "end": int(row["end_line"])}


# ------------------------------------------------------------------ running


def run(command):
    process = subprocess.run(command, capture_output=True, text=True, check=False)
    return process.returncode, process.stdout, process.stderr


def prepare_project(corpus, temporary):
    root = Path(temporary) / "corpus"
    for source in sorted(corpus.rglob("*")):
        if source.is_file() and source.suffix in SOURCE_SUFFIXES:
            target = root / source.relative_to(corpus)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
    return root


def run_duplicates(binary, root, flags, normalization, partial_flag):
    """Returns (report text, partial-flag status, error)."""
    base = [binary, "--root", str(root), "duplicates", *flags, "--normalization", normalization, "--no-excerpts"]
    if partial_flag:
        code, out, err = run(base + [partial_flag])
        if code == 0:
            return out, "accepted", None
        if partial_flag in err or "unexpected argument" in err or "unrecognized" in err:
            code, out, err = run(base)
            if code == 0:
                return out, "rejected", None
        return None, "error", err.strip()[-400:]
    code, out, err = run(base)
    return (out, "not-requested", None) if code == 0 else (None, "not-requested", err.strip()[-400:])


# ------------------------------------------------------------------ reports


def parse_range(text):
    match = re.match(r"^\s*(\d+)\s*(?:-\s*(\d+))?\s*$", str(text))
    if not match:
        return None
    start = int(match.group(1))
    return start, int(match.group(2) or start)


def first_table(value):
    if isinstance(value, list) and value and isinstance(value[0], dict):
        return value
    if isinstance(value, dict):
        for key, item in value.items():
            if key != "_raw" and isinstance(item, list) and item and isinstance(item[0], dict):
                return item
    return None


def report_groups(report):
    groups = []
    for raw in report.get("groups", []) if isinstance(report.get("groups"), list) else []:
        if not isinstance(raw, dict):
            continue
        members = []
        for row in raw.get("members", []) if isinstance(raw.get("members"), list) else []:
            if not isinstance(row, dict):
                continue
            span = parse_range(row.get("range", "")) or (
                (int(row["start_line"]), int(row.get("end_line", row["start_line"])))
                if str(row.get("start_line", "")).isdigit() else None)
            if span:
                members.append({"path": row.get("path", ""), "symbol": row.get("symbol", ""),
                                "start": span[0], "end": span[1], "row": row})
        groups.append({"raw": raw, "members": members,
                       "keys": {(m["path"], m["start"]) for m in members}})
    return groups


def case_keys(case, mode):
    return {(m["path"], m["start"]) for m in case["members"] if m["modes"].get(mode)}


def match_case(case, groups, mode):
    """Best reported group for a case: (status, group)."""
    expected = case_keys(case, mode)
    everything = {(m["path"], m["start"]) for m in case["members"]}
    best, overlap = None, 0
    for group in groups:
        shared = len(group["keys"] & everything)
        if shared > overlap:
            best, overlap = group, shared
    if len(expected) < 2:
        return ("absent-ok" if overlap < 2 else "absent-violated"), best
    if best is None or overlap < 2:
        return "missing", None
    if best["keys"] == expected:
        return "exact", best
    if expected < best["keys"]:
        return "superset", best
    if best["keys"] < expected:
        return "subset", best
    return "different", best


def grouping_scores(cases, groups, mode):
    rows, mismatches, claimed = {}, [], set()
    counts = {"expected": 0, "exact": 0, "partial": 0, "missing": 0, "absent_ok": 0, "absent_violated": 0}
    for case_id, case in cases.items():
        status, group = match_case(case, groups, mode)
        if group is not None:
            claimed.add(id(group))
        if status == "absent-ok":
            counts["absent_ok"] += 1
            continue
        if status == "absent-violated":
            counts["absent_violated"] += 1
            mismatches.append({"normalization": mode, "case": case_id, "status": status,
                               "detail": "labels expect no group in this mode"})
            continue
        counts["expected"] += 1
        if status == "exact":
            counts["exact"] += 1
        elif status == "missing":
            counts["missing"] += 1
            mismatches.append({"normalization": mode, "case": case_id, "status": status, "detail": ""})
        else:
            counts["partial"] += 1
            mismatches.append({"normalization": mode, "case": case_id, "status": status,
                               "detail": " ".join(f"{m['path']}:{m['start']}" for m in group["members"])})
        rows[case_id] = (status, group)
    unexpected = [g for g in groups if id(g) not in claimed]
    for group in unexpected:
        mismatches.append({"normalization": mode, "case": "-", "status": "unexpected-group",
                           "detail": " ".join(f"{m['path']}:{m['start']}" for m in group["members"])})
    recall = f"{counts['exact'] / counts['expected']:.3f}" if counts["expected"] else "n/a"
    summary = {"normalization": mode, "reported_groups": len(groups), "expected_groups": counts["expected"],
               "exact": counts["exact"], "partial": counts["partial"], "missing": counts["missing"],
               "unexpected_groups": len(unexpected), "expected_absent": counts["absent_ok"] + counts["absent_violated"],
               "absent_violated": counts["absent_violated"], "exact_recall": recall}
    return summary, mismatches, rows


# ----------------------------------------------------------------- verdicts


def group_level(group):
    verdict = group["raw"].get("verdict")
    if isinstance(verdict, dict):
        level = str(verdict.get("level", "")).strip().upper().replace("-", "_")
        reasons = verdict.get("reasons", [])
        return level or None, reasons if isinstance(reasons, list) else [reasons]
    if isinstance(verdict, str) and verdict.strip():
        return verdict.strip().upper().replace("-", "_"), []
    return None, []


def score_verdicts(report, cases, matched):
    groups = report_groups(report)
    present = any("verdict" in g["raw"] for g in groups) or "verdict_counts" in report
    if not present:
        return {"status": "not-implemented", "detail": "no group `verdict` block and no `verdict_counts` header"}
    matrix = {level: {p: 0 for p in PREDICTED} for level in LEVELS}
    ambiguous = {p: 0 for p in PREDICTED}
    rows, reason_counts = [], {}
    for case_id, case in cases.items():
        status, group = matched.get(case_id, ("missing", None))
        if status == "missing":
            predicted, reasons = "not-grouped", []
        elif status != "exact":
            predicted, reasons = "membership-differs", []
        else:
            level, reasons = group_level(group)
            predicted = level if level in LEVELS else "missing"
        for reason in reasons:
            if reason:
                reason_counts[reason] = reason_counts.get(reason, 0) + 1
        expected = case["verdict"]
        if expected == "ambiguous":
            ambiguous[predicted] += 1
        else:
            matrix[expected][predicted] += 1
        rows.append({"case": case_id, "expected": expected, "predicted": predicted,
                     "reasons": " ".join(r for r in reasons if r)})
    scored = sum(matrix[e][p] for e in LEVELS for p in LEVELS)
    correct = sum(matrix[level][level] for level in LEVELS)
    per_level = []
    for level in LEVELS:
        predicted_total = sum(matrix[e][level] for e in LEVELS)
        expected_total = sum(matrix[level].values())
        per_level.append({
            "level": level,
            "precision": f"{matrix[level][level] / predicted_total:.3f}" if predicted_total else "n/a",
            "recall": f"{matrix[level][level] / expected_total:.3f}" if expected_total else "n/a",
        })
    return {
        "status": "scored",
        "verdict_counts_header": "present" if "verdict_counts" in report else "absent",
        "determinate_cases": sum(sum(row.values()) for row in matrix.values()),
        "scored_with_level": scored,
        "accuracy": f"{correct / scored:.3f}" if scored else "n/a",
        "unsafe_real": matrix["SUSPICIOUS"]["REAL"] + matrix["FALSE_POSITIVE"]["REAL"],
        "real_called_false_positive": matrix["REAL"]["FALSE_POSITIVE"],
        "matrix": [dict({"expected": level}, **matrix[level]) for level in LEVELS],
        "ambiguous": dict({"expected": "ambiguous"}, **ambiguous),
        "per_level": per_level,
        "reasons": [{"reason": k, "groups": v} for k, v in sorted(reason_counts.items())],
        "cases": rows,
    }


# ---------------------------------------------------------------- subgroups


def resolve_ref(token, group, zero_based):
    token = token.strip()
    if not token:
        return None
    members = group["members"]
    if token.isdigit():
        index = int(token) - (0 if zero_based else 1)
        return (members[index]["path"], members[index]["start"]) if 0 <= index < len(members) else None
    match = re.match(r"^(?P<path>.+?):(?P<start>\d+)(?:-\d+)?$", token)
    if match:
        key = (match.group("path"), int(match.group("start")))
        return key if key in group["keys"] else None
    named = [m for m in members if m["symbol"] == token]
    return (named[0]["path"], named[0]["start"]) if len(named) == 1 else None


REF_KEYS = ("member", "members", "member_index", "member_indices", "member_refs", "refs", "ref", "indices", "index")
ID_KEYS = ("id", "subgroup", "subgroup_id", "name")


def predicted_partition(group):
    """Returns (status, partition as a set of frozensets or None, problem)."""
    if "subgroups" not in group["raw"] and not any(m["row"].get("subgroup") for m in group["members"]):
        return "absent", None, "group has no subgroups block"
    block = group["raw"].get("subgroups")
    status = None
    if isinstance(block, dict):
        status = str(block.get("status", "")).strip() or None
    rows = first_table(block)
    member_column = [m["row"].get("subgroup") for m in group["members"]]
    if status == "not-evaluated":
        return status, None, ""
    if status == "single" and not rows:
        return status, {frozenset(group["keys"])}, ""
    assignment = {}
    if rows:
        tokens = [str(row.get(k, "")) for row in rows for k in REF_KEYS if k in row]
        zero_based = any(t == "0" for tok in tokens for t in re.split(r"\s+/\s+|\s*[;|]\s*|\s+", tok.strip()))
        for row in rows:
            ident = next((str(row[k]) for k in ID_KEYS if k in row and k not in REF_KEYS), None)
            refs = [str(row[k]) for k in REF_KEYS if k in row]
            if "path" in row and ("range" in row or "start_line" in row):
                refs.append(f"{row['path']}:{row.get('range') or row.get('start_line')}")
            for ref in refs:
                for token in re.split(r"\s+/\s+|\s*[;|]\s*|\s+", ref):
                    key = resolve_ref(token, group, zero_based)
                    if key is not None:
                        assignment[key] = ident if ident is not None else str(len(assignment))
    elif any(member_column):
        for member, value in zip(group["members"], member_column):
            if value:
                assignment[(member["path"], member["start"])] = value
    if not assignment:
        return status or "unparsed", None, "no member references could be resolved"
    families = {}
    for key, ident in assignment.items():
        families.setdefault(ident, set()).add(key)
    problems = []
    if set(assignment) != group["keys"]:
        problems.append("some members carry no subgroup")
    if status == "split" and len(families) < 2:
        problems.append("status split but references resolve to one subgroup")
    if status == "single" and len(families) > 1:
        problems.append("status single but references resolve to several subgroups")
    return status or "split", {frozenset(v) for v in families.values()}, "; ".join(problems)


def score_subgroups(report, cases, matched):
    groups = report_groups(report)
    present = any("subgroups" in g["raw"] for g in groups) or "split_groups" in report
    if not present:
        return {"status": "not-implemented", "detail": "no group `subgroups` block and no `split_groups` header"}
    rows, correct, scored, statuses = [], 0, 0, {}
    for case_id, case in cases.items():
        status, group = matched.get(case_id, ("missing", None))
        expectation = case["partition"]
        sub_status, partition, problem = ("not-grouped", None, "") if group is None else predicted_partition(group)
        statuses[sub_status] = statuses.get(sub_status, 0) + 1
        if expectation == "unscored":
            rows.append({"case": case_id, "expected": "unscored", "predicted_status": sub_status,
                         "result": "unscored", "detail": problem})
            continue
        if group is None:
            rows.append({"case": case_id, "expected": expectation, "predicted_status": sub_status,
                         "result": "not-grouped", "detail": ""})
            scored += 1
            continue
        present_keys = group["keys"]
        families = {}
        for member in case["members"]:
            key = (member["path"], member["start"])
            if key in present_keys:
                families.setdefault(member["family"], set()).add(key)
        expected = {frozenset(v) for v in families.values()}
        if partition is not None:
            partition = {frozenset(p & set().union(*expected)) for p in partition} - {frozenset()}
        ok = partition is not None and partition == expected
        scored += 1
        correct += ok
        rows.append({"case": case_id, "expected": "split" if len(expected) > 1 else "single",
                     "predicted_status": sub_status, "result": "exact" if ok else "wrong",
                     "detail": problem or ("" if ok or partition is None else
                                           " | ".join(sorted(" ".join(f"{p}:{s}" for p, s in sorted(part))
                                                             for part in partition)))})
    return {
        "status": "scored",
        "split_groups_header": "present" if "split_groups" in report else "absent",
        "scored_cases": scored,
        "exact_partitions": correct,
        "exact_partition_rate": f"{correct / scored:.3f}" if scored else "n/a",
        "status_counts": [{"status": k, "groups": v} for k, v in sorted(statuses.items())],
        "cases": rows,
    }


# ----------------------------------------------------------- partial blocks


def block_occurrences(block):
    occurrences = []
    raw = block.get("occurrences", [])
    for row in raw if isinstance(raw, list) else []:
        if not isinstance(row, dict):
            continue
        span = None
        if str(row.get("start_line", "")).isdigit():
            span = (int(row["start_line"]), int(row.get("end_line") or row["start_line"]))
        else:
            span = parse_range(row.get("range", "") or row.get("lines", ""))
        if span:
            occurrences.append({"path": row.get("path", ""), "function": row.get("function", row.get("symbol", "")),
                                "start": span[0], "end": span[1]})
    return occurrences


def jaccard(a, b):
    if a["path"] != b["path"]:
        return 0.0
    inter = min(a["end"], b["end"]) - max(a["start"], b["start"]) + 1
    if inter <= 0:
        return 0.0
    union = max(a["end"], b["end"]) - min(a["start"], b["start"]) + 1
    return inter / union


def overlaps(a, b):
    return a["path"] == b["path"] and min(a["end"], b["end"]) >= max(a["start"], b["start"])


def inside(a, b):
    return a["path"] == b["path"] and b["start"] <= a["start"] and a["end"] <= b["end"]


def match_block(reported, expected):
    """Indices of expected occurrences matched one-to-one, or None."""
    used = set()
    for occ in reported:
        best = max(((jaccard(occ, e), i) for i, e in enumerate(expected) if i not in used), default=(0.0, None))
        if best[0] < LINE_JACCARD:
            return None
        used.add(best[1])
    return used if len(used) >= 2 else None


def score_partial_blocks(report, cases, blocks, negatives, flag_status):
    section = report.get("partial_blocks")
    header = {k: report.get(k, section.get(k) if isinstance(section, dict) else None)
              for k in ("partial_blocks_budget", "budget_exhausted", "partial_block_count")}
    # ADR-012 places exhaustion inside the budget object, not the block list.
    budget = report.get("partial_blocks_budget")
    if isinstance(budget, dict) and "budget_exhausted" in budget:
        header["budget_exhausted"] = budget["budget_exhausted"]
    if section is None:
        detail = "no top-level `partial_blocks` section"
        if flag_status == "rejected":
            detail += "; the binary rejected the opt-in flag"
        return {"status": "not-implemented", "flag": flag_status, "detail": detail}
    reported = section if isinstance(section, list) else (first_table(section) or [])
    if isinstance(section, dict):
        for key, value in section.items():
            if isinstance(value, list) and value and isinstance(value[0], dict) and "occurrences" in value[0]:
                reported = value
    found = {block_id: set() for block_id in blocks}
    rows, true_positive = [], 0
    false_kinds = {"below-minimum": 0, "shape-only": 0, "covered-by-group": 0, "unlabeled": 0}
    for index, block in enumerate(reported):
        if not isinstance(block, dict):
            continue
        occs = block_occurrences(block)
        label = None
        for block_id, expected in blocks.items():
            used = match_block(occs, expected["occurrences"])
            if used is not None:
                found[block_id] |= used
                label = block_id
                break
        if label:
            true_positive += 1
            kind = "true-positive"
        else:
            kind = "unlabeled"
            for negative in negatives.values():
                if occs and all(any(overlaps(o, n) for n in negative["occurrences"]) for o in occs):
                    kind, label = negative["kind"], negative["id"]
                    break
            if kind == "unlabeled":
                for case_id, case in cases.items():
                    spans = [{"path": m["path"], "start": m["start"], "end": m["end"]} for m in case["members"]]
                    if occs and all(any(inside(o, s) for s in spans) for o in occs):
                        kind, label = "covered-by-group", case_id
                        break
            false_kinds[kind] += 1
        rows.append({"reported": block.get("id", index + 1), "result": kind, "label": label or "-",
                     "occurrences": " ".join(f"{o['path']}:{o['start']}-{o['end']}" for o in occs)})
    total = len(rows)
    recalled = sum(1 for block_id, used in found.items() if len(used) == len(blocks[block_id]["occurrences"]))
    expected_occ = sum(len(b["occurrences"]) for b in blocks.values())
    matched_occ = sum(len(used) for used in found.values())
    return {
        "status": "scored",
        "flag": flag_status,
        "header": [{"field": k, "value": "absent" if v is None else v} for k, v in header.items()],
        "reported_blocks": total,
        "true_positives": true_positive,
        "precision": f"{true_positive / total:.3f}" if total else "n/a",
        "expected_blocks": len(blocks),
        "recalled_blocks": recalled,
        "recall": f"{recalled / len(blocks):.3f}" if blocks else "n/a",
        "occurrence_recall": f"{matched_occ / expected_occ:.3f}" if expected_occ else "n/a",
        "false_positive_kinds": [{"kind": k, "blocks": v} for k, v in false_kinds.items()],
        "missed": [{"block": b, "matched_occurrences": len(found[b]), "expected_occurrences": len(blocks[b]["occurrences"])}
                   for b in blocks if len(found[b]) < len(blocks[b]["occurrences"])],
        "blocks": rows,
    }


# ------------------------------------------------------------------- output


def display_path(path):
    path = Path(path).resolve()
    try:
        return str(path.relative_to(REPO))
    except ValueError:
        return path.name


def emit_aspect(out, name, result, tables):
    out.append(f"{name}:")
    for key, value in result.items():
        if key in tables:
            fields = tables[key]
            data = value if isinstance(value, list) else [value]
            emit_table(out, 2, key, fields, data)
        elif not isinstance(value, (list, dict)):
            out.append(f"  {key}: {toon_value(value)}")


def safely(function, *args):
    try:
        return function(*args)
    except Exception as error:  # Report the aspect as broken, never crash the run.
        return {"status": "scorer-error", "detail": f"{type(error).__name__}: {error}"}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, help="mct-cli binary to run (required unless --report)")
    parser.add_argument("--report", type=Path, help="score this saved TOON report instead of running a binary")
    parser.add_argument("--labels", type=Path, default=DEFAULT_CORPUS / "labels.toon")
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--normalization", default=None,
                        help="policy scored for verdicts, subgroups and blocks (default: the labels' reference)")
    parser.add_argument("--flags", default=None,
                        help="duplicates flags as one string (default: the labels' analysis_flags)")
    parser.add_argument("--partial-blocks-flag", default="--partial-blocks",
                        help="opt-in flag for partial blocks; empty string to not pass one")
    parser.add_argument("--grouping-modes", default="broad,consistent,exact",
                        help="comma-separated policies for the grouping baseline")
    args = parser.parse_args()
    if not args.binary and not args.report:
        parser.error("give --binary or --report")

    labels, cases, blocks, negatives = load_labels(args.labels)
    normalization = args.normalization or labels.get("reference_normalization", "broad")
    flags = args.flags.split() if args.flags is not None else list(labels.get("analysis_flags", []))
    out = ["scorer: score_duplicate_labels/1",
           f"labels: {toon_value(display_path(args.labels))}",
           f"cases: {len(cases)}",
           f"expected_partial_blocks: {len(blocks)}",
           f"normalization: {normalization}"]
    emit_list(out, 0, "flags", flags)
    out.append(f"line_match_rule: {toon_value(f'same path and line-range Jaccard >= {LINE_JACCARD} (shared lines / union lines), one-to-one per block; a reported block is a true positive when all its occurrences, at least two, match occurrences of one labeled block')}")
    out.append("member_match_rule: \"same repository-relative path and start line; a case's group must contain exactly the labeled members for verdict scoring\"")

    # The report scored for verdicts, subgroups and blocks, and its matched cases.
    grouping_rows, mismatch_rows, report, matched, flag_status = [], [], None, None, "not-requested"
    if args.report:
        out.append(f"source: {toon_value('saved report ' + display_path(args.report))}")
        report = parse_toon(args.report.read_text(encoding="utf-8"))
        flag_status = "unknown (saved report)"
        summary, mismatches, matched = grouping_scores(cases, report_groups(report), normalization)
        grouping_rows.append(summary)
        mismatch_rows += mismatches
    else:
        binary = str(args.binary.resolve(strict=True))
        out.append(f"binary: {toon_value(display_path(args.binary))}")
        with tempfile.TemporaryDirectory(prefix="mct-duplicate-labels-") as temporary:
            root = prepare_project(args.corpus, temporary)
            code, _, err = run([binary, "--root", str(root), "init"])
            if code:
                out.append("run_status: init-failed")
                out.append(f"run_error: {toon_value(err.strip()[-400:])}")
                print("\n".join(out))
                return 2
            modes = [m for m in args.grouping_modes.split(",") if m]
            if normalization not in modes:
                modes.append(normalization)
            for mode in modes:
                partial = args.partial_blocks_flag if mode == normalization else ""
                text, status, error = run_duplicates(binary, root, flags, mode, partial)
                if text is None:
                    grouping_rows.append({"normalization": mode, "reported_groups": "run-failed"})
                    mismatch_rows.append({"normalization": mode, "case": "-", "status": "run-failed",
                                          "detail": error or ""})
                    continue
                parsed = parse_toon(text)
                summary, mismatches, rows = grouping_scores(cases, report_groups(parsed), mode)
                if mode == normalization:
                    report, matched, flag_status = parsed, rows, status
                grouping_rows.append(summary)
                mismatch_rows += mismatches
    if report is None:
        out.append("run_status: duplicates-failed")
        emit_aspect(out, "grouping", {"modes": grouping_rows, "mismatches": mismatch_rows},
                    {"modes": ["normalization", "reported_groups"],
                     "mismatches": ["normalization", "case", "status", "detail"]})
        print("\n".join(out))
        return 2
    out.append("run_status: ok")
    out.append(f"report_status: {toon_value(report.get('status', 'absent'))}")
    out.append(f"partial_blocks_flag: {toon_value(args.partial_blocks_flag or '(none)')}")
    out.append(f"partial_blocks_flag_status: {toon_value(flag_status)}")

    group_fields = ["normalization", "reported_groups", "expected_groups", "exact", "partial", "missing",
                    "unexpected_groups", "expected_absent", "absent_violated", "exact_recall"]
    emit_aspect(out, "grouping", {"definition": "exact_recall = labeled groups reported with exactly their members / labeled groups expected in that policy",
                                  "modes": grouping_rows, "mismatches": mismatch_rows},
                {"modes": group_fields, "mismatches": ["normalization", "case", "status", "detail"]})
    emit_aspect(out, "verdicts", safely(score_verdicts, report, cases, matched), {
        "matrix": ["expected"] + PREDICTED, "ambiguous": ["expected"] + PREDICTED,
        "per_level": ["level", "precision", "recall"], "reasons": ["reason", "groups"],
        "cases": ["case", "expected", "predicted", "reasons"]})
    emit_aspect(out, "subgroups", safely(score_subgroups, report, cases, matched), {
        "status_counts": ["status", "groups"],
        "cases": ["case", "expected", "predicted_status", "result", "detail"]})
    emit_aspect(out, "partial_blocks", safely(score_partial_blocks, report, cases, blocks, negatives, flag_status), {
        "header": ["field", "value"], "false_positive_kinds": ["kind", "blocks"],
        "missed": ["block", "matched_occurrences", "expected_occurrences"],
        "blocks": ["reported", "result", "label", "occurrences"]})
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
