"""Risk level and test selection for a set of changed files.

Deterministic and explainable: every changed path produces one reason line,
and the plan's level is the highest of them. Anything the policy or the crate
graph cannot place raises the level instead of lowering it.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import policy as pol
from dependency_graph import CrateGraph
from detect_changes import Change

# Paths inside a crate that only its own tests compile: changing them cannot
# break a dependent.
_LOCAL_ONLY_DIRS = ("tests/", "benches/", "examples/")


@dataclass(frozen=True)
class Reason:
    path: str
    level: str
    why: str


@dataclass
class Plan:
    level: str = "R0"
    full: bool = False
    touched_crates: set[str] = field(default_factory=set)
    test_crates: set[str] = field(default_factory=set)
    reasons: list[Reason] = field(default_factory=list)

    def raise_to(self, level: str) -> None:
        if pol.level_index(level) > pol.level_index(self.level):
            self.level = level

    def as_lines(self) -> list[str]:
        lines = [
            f"level={self.level}",
            f"full={'true' if self.full else 'false'}",
            f"touched_crates={','.join(sorted(self.touched_crates))}",
            f"test_crates={'*' if self.full else ','.join(sorted(self.test_crates))}",
        ]
        lines += [f"reason[{r.level}] {r.path}: {r.why}" for r in self.reasons]
        return lines


def full_plan(why: str, level: str = "R3") -> Plan:
    """The conservative answer when the impact cannot be computed."""
    plan = Plan(level=level, full=True)
    plan.reasons.append(Reason("*", level, why))
    return plan


def _best_rule(path: str, rules: list[dict]) -> dict | None:
    scope_rank = {"docs": 0, "crate": 1, "full": 2}
    matching = [r for r in rules if pol.glob_match(r["pattern"], path)]
    if not matching:
        return None
    return max(
        matching, key=lambda r: (pol.level_index(r["level"]), scope_rank[r["scope"]])
    )


def _owner(path: str, graph: CrateGraph, path_owners: dict[str, str]) -> str | None:
    for prefix, crate in path_owners.items():
        if path.startswith(prefix):
            return crate
    return graph.owner_of(path)


def _classify_crate_path(
    path: str, crate: str, graph: CrateGraph, policy: dict, plan: Plan
) -> Reason:
    plan.touched_crates.add(crate)
    if crate in policy.get("critical_crates", []):
        plan.full = True
        return Reason(path, "R3", f"critical crate {crate}")
    rel = path[len(graph.dirs[crate]) + 1 :] if graph.dirs.get(crate) else path
    if rel.startswith(_LOCAL_ONLY_DIRS) and not rel.endswith("Cargo.toml"):
        plan.test_crates.add(crate)
        return Reason(path, "R1", f"tests of {crate} only")
    affected = graph.dependents({crate})
    plan.test_crates |= affected
    if len(affected) == 1:
        return Reason(path, "R1", f"{crate} has no workspace dependents")
    others = ",".join(sorted(affected - {crate}))
    return Reason(path, "R2", f"{crate} and its dependents {others}")


def classify(changes: list[Change], graph: CrateGraph, policy: dict) -> Plan:
    plan = Plan()
    if not changes:
        plan.reasons.append(Reason("-", "R0", "no changed files"))
        return plan
    rules = policy.get("rules", [])
    path_owners = policy.get("path_owners", {})
    unknown = policy["unknown_level"]
    for path in sorted({p for change in changes for p in change.paths()}):
        rule = _best_rule(path, rules)
        if rule is None:
            reason = Reason(path, unknown, "no policy rule matches: unknown impact")
            plan.full = True
        elif rule["scope"] == "full":
            reason = Reason(path, rule["level"], rule["reason"])
            plan.full = True
        elif rule["scope"] == "docs":
            reason = Reason(path, rule["level"], rule["reason"])
            plan.test_crates |= set(policy.get("docs_consumer_crates", []))
        else:
            crate = _owner(path, graph, path_owners)
            if crate is None or crate not in graph.dirs:
                reason = Reason(path, unknown, "path in no workspace crate (deleted crate?)")
                plan.full = True
            else:
                reason = _classify_crate_path(path, crate, graph, policy, plan)
                reason = Reason(reason.path, max_level(reason.level, rule["level"]), reason.why)
        plan.reasons.append(reason)
        plan.raise_to(reason.level)

    share = len(plan.test_crates) / max(len(graph.dirs), 1)
    if not plan.full and share >= policy.get("full_suite_share", 1.0):
        plan.full = True
        plan.reasons.append(
            Reason("*", "R3", f"dependents cover {share:.0%} of the workspace")
        )
    if plan.full:
        plan.raise_to("R3")
        plan.test_crates = set(graph.dirs)
    return plan


def max_level(a: str, b: str) -> str:
    return a if pol.level_index(a) >= pol.level_index(b) else b
