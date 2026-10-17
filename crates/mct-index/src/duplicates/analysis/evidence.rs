//! Aligned evidence for a group: members share one normalized sequence, so
//! their identifier and literal tokens correspond position by position. This
//! module compares the original text at each position. It never changes
//! grouping and never infers meaning from a name.

use std::collections::{HashMap, HashSet};

use mct_core::{BodyShape, LiteralCategory, TokenRole};

use super::super::{
    AlignedDifference, CallShape, ContextAspect, ContextComparison, ContextEvidence, ContextStatus,
    DifferenceKind, EvidenceWarning, GroupEvidence, GroupPriority, IdentifierPattern,
    PriorityLevel, TrivialCallPolicy, TypeEvidence, WarningCode, MAX_DIFFERENCE_TEXT_CHARS,
    MAX_LISTED_DIFFERENCES,
};

/// What a candidate's callable span excludes, from its token source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MemberContext {
    pub(super) decorators: Vec<String>,
    pub(super) enclosing: Vec<String>,
}

/// Everything evidence needs about one candidate.
pub(super) struct MemberInput {
    pub(super) slots: Vec<Slot>,
    /// `None` when the token source reports no callable spans.
    pub(super) context: Option<MemberContext>,
    /// `None` when the token source reports no body shape.
    pub(super) body: Option<BodyShape>,
}

/// One identifier or literal token of a candidate, in sequence order.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    pub(super) literal: bool,
    pub(super) role: TokenRole,
    pub(super) primitive_spelling: bool,
    /// Index into the analysis' [`Texts`].
    pub(super) text: u32,
    pub(super) line: u32,
}

/// Interned token texts, shared by every candidate of one analysis.
#[derive(Default)]
pub(super) struct Texts {
    ids: HashMap<String, u32>,
    texts: Vec<String>,
}

impl Texts {
    pub(super) fn id(&mut self, text: &str) -> u32 {
        if let Some(id) = self.ids.get(text) {
            return *id;
        }
        let id = u32::try_from(self.texts.len()).unwrap_or(u32::MAX);
        self.texts.push(text.to_string());
        self.ids.insert(text.to_string(), id);
        id
    }

    pub(super) fn get(&self, id: u32) -> &str {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.texts.get(id))
            .map_or("", String::as_str)
    }
}

/// Compares `members` (one slot list per member, in member order).
pub(super) fn evaluate(members: &[&[Slot]], texts: &Texts) -> GroupEvidence {
    // Fewer than two lists means a member's slots were missing.
    let first = members.first().copied().unwrap_or_default();
    if members.len() < 2 || members.iter().any(|slots| slots.len() != first.len()) {
        return GroupEvidence {
            warnings: vec![EvidenceWarning {
                code: WarningCode::AlignmentUnavailable,
                detail: "member token lists do not align; no difference was evaluated".to_string(),
            }],
            ..GroupEvidence::default()
        };
    }

    let mut types = TypeEvidence::default();
    let mut positions: HashMap<DifferenceKind, usize> = HashMap::new();
    // Distinct differences by (kind, per-member texts), in first-seen order.
    let mut rows: Vec<AlignedDifference> = Vec::new();
    let mut seen: HashMap<(DifferenceKind, Vec<u32>), usize> = HashMap::new();
    // Spellings each member binds itself: its parameters. Nothing else is
    // known to be bound inside the member. Declaration names are left out: a
    // path that repeats the member's own name (`queries::find_x` inside
    // `fn find_x`) reaches another item.
    let declared: Vec<HashSet<u32>> = members
        .iter()
        .map(|slots| {
            slots
                .iter()
                .filter(|slot| !slot.literal && slot.role == TokenRole::Parameter)
                .map(|slot| slot.text)
                .collect()
        })
        .collect();
    let mut undeclared_name_positions = 0;
    for i in 0..first.len() {
        let column: Vec<Slot> = members.iter().map(|slots| slots[i]).collect();
        observe_types(&column, texts, &mut types);
        if column.iter().all(|slot| slot.text == column[0].text) {
            continue;
        }
        let kind = difference_kind(&column);
        *positions.entry(kind).or_default() += 1;
        if kind == DifferenceKind::Name
            && column
                .iter()
                .zip(&declared)
                .any(|(slot, declared)| !declared.contains(&slot.text))
        {
            undeclared_name_positions += 1;
        }
        let key = (kind, column.iter().map(|slot| slot.text).collect());
        if let Some(&row) = seen.get(&key) {
            rows[row].occurrences += 1;
            continue;
        }
        seen.insert(key, rows.len());
        rows.push(AlignedDifference {
            kind,
            occurrences: 1,
            values: column
                .iter()
                .map(|slot| display(texts.get(slot.text), kind, slot.role))
                .collect(),
            lines: column.iter().map(|slot| slot.line).collect(),
        });
    }

    let mut differing_positions: Vec<(DifferenceKind, usize)> = positions.into_iter().collect();
    differing_positions.sort_unstable();
    let pattern = identifier_pattern(members, texts);
    let mut warnings = warnings(&differing_positions, &pattern, &rows);
    if types.primitive_spelling_conflicts > 0 {
        warnings.push(EvidenceWarning {
            code: WarningCode::PrimitiveTypeSpellingsDiffer,
            detail: format!(
                "{} aligned type positions contain distinct primitive spellings; compatibility \
                 remains unknown because names, imports and aliases are unresolved",
                types.primitive_spelling_conflicts
            ),
        });
    }
    let distinct_differences = rows.len();
    rows.truncate(MAX_LISTED_DIFFERENCES);
    GroupEvidence {
        identifier_pattern: pattern,
        differing_positions,
        distinct_differences,
        undeclared_name_positions,
        differences: rows,
        context: ContextEvidence::default(),
        types,
        warnings,
    }
}

/// Compare only complete annotations recognized by the adapters. Unknown
/// members do not hide a conflict between two other members. No type is resolved.
fn observe_types(column: &[Slot], texts: &Texts, evidence: &mut TypeEvidence) {
    if !column.iter().any(|slot| slot.role == TokenRole::Type) {
        return;
    }
    evidence.compared_positions += 1;
    if column.iter().any(|slot| slot.role != TokenRole::Type) {
        return;
    }
    let mut primitive = None;
    let mut conflict = false;
    for slot in column
        .iter()
        .filter(|slot| slot.primitive_spelling && !slot.literal)
    {
        match primitive {
            Some(first) if first != slot.text => conflict = true,
            None => primitive = Some(slot.text),
            _ => {}
        }
    }
    if !conflict {
        return;
    }
    evidence.primitive_spelling_conflicts += 1;
    if evidence.conflicts.len() < MAX_LISTED_DIFFERENCES {
        evidence.conflicts.push(AlignedDifference {
            kind: DifferenceKind::Type,
            occurrences: 1,
            values: column
                .iter()
                .map(|slot| {
                    let value = cut(texts.get(slot.text));
                    if slot.primitive_spelling {
                        value
                    } else {
                        format!("{value} (unknown)")
                    }
                })
                .collect(),
            lines: column.iter().map(|slot| slot.line).collect(),
        });
    }
}

/// The kind of one differing position. Roles come from the first member;
/// members that disagree on the role make it a mixed-role difference.
pub(super) fn difference_kind(column: &[Slot]) -> DifferenceKind {
    let role = column[0].role;
    if column.iter().any(|slot| slot.literal != column[0].literal) {
        return DifferenceKind::MixedRole;
    }
    if column[0].literal {
        let categories: Vec<LiteralCategory> = column
            .iter()
            .map(|slot| match slot.role {
                TokenRole::Literal(category) => category,
                _ => LiteralCategory::Other,
            })
            .collect();
        if categories.iter().any(|category| *category != categories[0]) {
            return DifferenceKind::LiteralCategory;
        }
        return match categories[0] {
            LiteralCategory::Docstring => DifferenceKind::Docstring,
            LiteralCategory::InterpolatedString => DifferenceKind::InterpolatedString,
            _ => DifferenceKind::LiteralValue,
        };
    }
    if column.iter().any(|slot| slot.role != role) {
        return DifferenceKind::MixedRole;
    }
    match role {
        TokenRole::Declaration => DifferenceKind::Declaration,
        TokenRole::Parameter => DifferenceKind::Parameter,
        TokenRole::Member => DifferenceKind::Member,
        TokenRole::Callee => DifferenceKind::Callee,
        TokenRole::Type => DifferenceKind::Type,
        TokenRole::Unclassified | TokenRole::Literal(_) => DifferenceKind::Name,
    }
}

/// `text` cut to [`MAX_DIFFERENCE_TEXT_CHARS`] characters, `…` marking a cut.
pub(super) fn cut(text: &str) -> String {
    let mut shown: String = text.chars().take(MAX_DIFFERENCE_TEXT_CHARS).collect();
    if text.chars().nth(MAX_DIFFERENCE_TEXT_CHARS).is_some() {
        shown.push('…');
    }
    shown
}

fn display(text: &str, kind: DifferenceKind, role: TokenRole) -> String {
    let mut shown = cut(text);
    if kind == DifferenceKind::LiteralCategory {
        shown = format!("{shown} ({})", role.as_str());
    }
    shown
}

/// Compares the members' decorators and enclosing constructs (one entry per
/// member, in member order) and returns the comparison with its warnings.
/// Full texts are compared; only the shown values are cut.
pub(super) fn evaluate_context(
    members: &[Option<&MemberContext>],
) -> (ContextEvidence, Vec<EvidenceWarning>) {
    let Some(contexts) = members
        .iter()
        .copied()
        .collect::<Option<Vec<&MemberContext>>>()
        .filter(|contexts| contexts.len() >= 2)
    else {
        let warning = EvidenceWarning {
            code: WarningCode::ContextUnavailable,
            detail: "the token source reports no callable context for at least one member; \
                     decorators and enclosing constructs were not compared"
                .to_string(),
        };
        return (ContextEvidence::default(), vec![warning]);
    };
    let aspects = [
        (
            ContextAspect::Decorators,
            contexts
                .iter()
                .map(|context| joined(&context.decorators, " ", "(none)"))
                .collect::<Vec<_>>(),
            WarningCode::DecoratorsDiffer,
            "decorators or outer attributes differ; they can change how a member is called, \
             registered, cached or compiled, and their effect is not evaluated",
        ),
        (
            ContextAspect::Enclosing,
            contexts
                .iter()
                .map(|context| joined(&context.enclosing, " > ", "(top level)"))
                .collect(),
            WarningCode::EnclosingContextDiffers,
            "enclosing constructs differ; members may be defined under different conditions, \
             receivers or error handling, which is not evaluated",
        ),
    ];
    let mut warnings = Vec::new();
    let mut rows = Vec::new();
    for (aspect, values, code, text) in aspects {
        let differs = values.iter().any(|value| *value != values[0]);
        let shown: Vec<String> = if differs {
            values.iter().map(|value| cut(value)).collect()
        } else {
            vec![cut(&values[0])]
        };
        if differs {
            warnings.push(EvidenceWarning {
                code,
                detail: format!(
                    "{text}; a difference does not by itself make the repetition false; e.g. {}",
                    shown.join(" / ")
                ),
            });
        }
        rows.push(ContextComparison {
            aspect,
            differs,
            values: shown,
        });
    }
    let status = if warnings.is_empty() {
        ContextStatus::Same
    } else {
        ContextStatus::Differs
    };
    (
        ContextEvidence {
            status,
            aspects: rows,
        },
        warnings,
    )
}

/// The group's priority from its members' body shapes (one per member, in
/// member order) under `policy`. Only syntax decides: what is called, and
/// what the calls do, is never looked at.
pub(super) fn evaluate_priority(
    bodies: &[Option<BodyShape>],
    policy: TrivialCallPolicy,
) -> GroupPriority {
    let normal = |call_shape, reason: String| GroupPriority {
        level: PriorityLevel::Normal,
        call_shape,
        reason,
    };
    let Some(shapes) = bodies
        .iter()
        .copied()
        .collect::<Option<Vec<BodyShape>>>()
        .filter(|shapes| shapes.len() >= 2)
    else {
        return normal(
            CallShape::NotEvaluated,
            "the token source reports no body shape for at least one member; not demoted"
                .to_string(),
        );
    };
    let call_only = shapes.iter().filter(|shape| shape.is_call_only()).count();
    if call_only == 0 {
        return normal(
            CallShape::Other,
            "every member's body has a statement other than a single flat call".to_string(),
        );
    }
    if call_only < shapes.len() {
        return normal(
            CallShape::Mixed,
            format!(
                "{call_only} of {} members have bodies made only of flat calls; a group is \
                 demoted only when all do",
                shapes.len()
            ),
        );
    }
    let low = shapes
        .iter()
        .map(|shape| shape.statements)
        .min()
        .unwrap_or(0);
    let high = shapes
        .iter()
        .map(|shape| shape.statements)
        .max()
        .unwrap_or(0);
    let statements = match (low, high) {
        (1, 1) => "1 statement".to_string(),
        _ if low == high => format!("{low} statements"),
        _ => format!("{low}-{high} statements"),
    };
    let shape = format!(
        "every member's body is only flat calls ({statements}, each one call or \
         macro invocation whose arguments hold no further call, closure, comprehension, \
         conditional or block); callee names are not considered and the calls' effects are \
         not evaluated"
    );
    match policy {
        TrivialCallPolicy::Demote => GroupPriority {
            level: PriorityLevel::Demoted,
            call_shape: CallShape::CallOnly,
            reason: format!(
                "{shape}; ranked after the other groups by the demote policy, \
                 the keep policy restores rank order"
            ),
        },
        TrivialCallPolicy::Keep => normal(
            CallShape::CallOnly,
            format!("{shape}; kept in rank order by the keep policy"),
        ),
    }
}

pub(super) fn joined(items: &[String], separator: &str, empty: &str) -> String {
    if items.is_empty() {
        empty.to_string()
    } else {
        items.join(separator)
    }
}

/// Checks, between the first member and each other one, that identifier
/// spellings map one-to-one in both directions.
fn identifier_pattern(members: &[&[Slot]], texts: &Texts) -> IdentifierPattern {
    let first = members[0];
    let mut identical = true;
    for (other_index, other) in members.iter().enumerate().skip(1) {
        let mut forward: HashMap<u32, u32> = HashMap::new();
        let mut backward: HashMap<u32, u32> = HashMap::new();
        for (a, b) in first.iter().zip(other.iter()) {
            if a.literal || b.literal {
                continue;
            }
            identical &= a.text == b.text;
            let other_name = format!("member {}", other_index + 1);
            let conflict = match (forward.get(&a.text), backward.get(&b.text)) {
                (Some(&mapped), _) if mapped != b.text => Some((
                    a.text,
                    "member 1",
                    mapped,
                    b.text,
                    other_name.as_str(),
                    b.line,
                )),
                (_, Some(&mapped)) if mapped != a.text => Some((
                    b.text,
                    other_name.as_str(),
                    mapped,
                    a.text,
                    "member 1",
                    a.line,
                )),
                _ => None,
            };
            if let Some((name, holder, earlier, later, target, line)) = conflict {
                return IdentifierPattern::NotOneToOne(format!(
                    "`{}` in {holder} aligns with both `{}` and `{}` in {target} (line {line})",
                    texts.get(name),
                    texts.get(earlier),
                    texts.get(later),
                ));
            }
            forward.insert(a.text, b.text);
            backward.insert(b.text, a.text);
        }
    }
    if identical {
        IdentifierPattern::Identical
    } else {
        IdentifierPattern::OneToOne
    }
}

fn warnings(
    positions: &[(DifferenceKind, usize)],
    pattern: &IdentifierPattern,
    rows: &[AlignedDifference],
) -> Vec<EvidenceWarning> {
    let count = |kind: DifferenceKind| {
        positions
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0, |(_, n)| *n)
    };
    let example = |kind: DifferenceKind| {
        rows.iter()
            .find(|row| row.kind == kind)
            .map(|row| {
                let lines: Vec<String> = row.lines.iter().map(u32::to_string).collect();
                format!(
                    "; e.g. {} at lines {}",
                    row.values.join(" / "),
                    lines.join(" / ")
                )
            })
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    let mut add = |code: WarningCode, detail: String| out.push(EvidenceWarning { code, detail });
    if let IdentifierPattern::NotOneToOne(detail) = pattern {
        add(
            WarningCode::IdentifierPatternDiffers,
            format!(
                "identifier spellings are not a consistent renaming: {detail}; the members use \
                 their names differently although the normalized shape matches"
            ),
        );
    }
    let checks = [
        (
            DifferenceKind::Member,
            WarningCode::MemberNamesDiffer,
            "aligned attribute, field, method or keyword names differ: the members reach \
             different members of their values; no domain is inferred from the names",
        ),
        (
            DifferenceKind::Callee,
            WarningCode::CalleeNamesDiffer,
            "aligned call targets differ; whether the called functions are interchangeable \
             is not evaluated",
        ),
        (
            DifferenceKind::Type,
            WarningCode::TypeNamesDiffer,
            "aligned type names differ; compatibility is not evaluated (aliases, unions, \
             generics and imports are unresolved)",
        ),
        (
            DifferenceKind::LiteralCategory,
            WarningCode::LiteralCategoriesDiffer,
            "aligned literals change category, so the members use values of different \
             literal types at those positions",
        ),
        (
            DifferenceKind::LiteralValue,
            WarningCode::LiteralValuesDiffer,
            "aligned literal values differ; whether a value is essential or could become a \
             parameter is not evaluated",
        ),
        (
            DifferenceKind::InterpolatedString,
            WarningCode::InterpolatedStringsDiffer,
            "aligned interpolated strings differ; this policy compares them as whole \
             literals, so their embedded expressions may differ (the consistent and exact \
             policies compare those expressions)",
        ),
    ];
    for (kind, code, text) in checks {
        let n = count(kind);
        if n > 0 {
            add(code, format!("{n} {text}{}", example(kind)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn slots(texts: &mut Texts, tokens: &[(&str, TokenRole)]) -> Vec<Slot> {
        tokens
            .iter()
            .enumerate()
            .map(|(line, (text, role))| Slot {
                literal: matches!(role, TokenRole::Literal(_)),
                role: *role,
                primitive_spelling: false,
                text: texts.id(text),
                line: u32::try_from(line).unwrap() + 1,
            })
            .collect()
    }

    fn as_refs(owned: &[(String, TokenRole)]) -> Vec<(&str, TokenRole)> {
        owned
            .iter()
            .map(|(text, role)| (text.as_str(), *role))
            .collect()
    }

    fn names(names: &[&'static str]) -> Vec<(&'static str, TokenRole)> {
        names
            .iter()
            .map(|name| (*name, TokenRole::Unclassified))
            .collect()
    }

    fn typed(texts: &mut Texts, values: &[(&str, bool)]) -> Vec<Slot> {
        let tokens: Vec<_> = values
            .iter()
            .map(|(text, _)| (*text, TokenRole::Type))
            .collect();
        let mut result = slots(texts, &tokens);
        for (slot, (_, recognized)) in result.iter_mut().zip(values) {
            slot.primitive_spelling = *recognized;
        }
        result
    }

    #[test]
    fn primitive_conflicts_compare_nonfirst_members_without_resolving_unknown_names() {
        let mut texts = Texts::default();
        let a = typed(&mut texts, &[("Alias", false)]);
        let b = typed(&mut texts, &[("u32", true)]);
        let c = typed(&mut texts, &[("u64", true)]);
        let evidence = evaluate(&[&a, &b, &c], &texts);
        assert_eq!(evidence.types.compared_positions, 1);
        assert_eq!(evidence.types.primitive_spelling_conflicts, 1);
        assert_eq!(
            evidence.types.conflicts[0].values,
            ["Alias (unknown)", "u32", "u64"]
        );
        assert!(evidence.warnings.iter().any(|warning| warning.code
            == WarningCode::PrimitiveTypeSpellingsDiffer
            && warning.detail.contains("compatibility remains unknown")));
    }

    #[test]
    fn equal_unknown_and_mixed_type_positions_never_establish_compatibility() {
        let mut texts = Texts::default();
        let a = typed(
            &mut texts,
            &[("int", true), ("Custom", false), ("int", true)],
        );
        let mut b = typed(
            &mut texts,
            &[("int", true), ("Other", false), ("str", true)],
        );
        b[2].role = TokenRole::Callee;
        let evidence = evaluate(&[&a, &b], &texts);
        assert_eq!(evidence.types.compared_positions, 3);
        assert_eq!(evidence.types.primitive_spelling_conflicts, 0);
        assert!(evidence.types.conflicts.is_empty());
        assert!(!evidence
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::PrimitiveTypeSpellingsDiffer));
        assert_eq!(evaluate(&[&a], &texts).types.compared_positions, 0);
    }

    #[test]
    fn type_conflict_listing_is_bounded_and_counts_include_the_unlisted_tail() {
        let mut texts = Texts::default();
        let a = typed(&mut texts, &[("int", true); MAX_LISTED_DIFFERENCES + 2]);
        let b = typed(&mut texts, &[("str", true); MAX_LISTED_DIFFERENCES + 2]);
        let evidence = evaluate(&[&a, &b], &texts);
        assert_eq!(
            evidence.types.primitive_spelling_conflicts,
            MAX_LISTED_DIFFERENCES + 2
        );
        assert_eq!(evidence.types.conflicts.len(), MAX_LISTED_DIFFERENCES);
        assert_eq!(evidence.types.conflicts[0].lines, [1, 1]);
    }

    #[test]
    fn the_pattern_check_runs_in_both_directions() {
        let mut texts = Texts::default();
        let first = slots(&mut texts, &names(&["x", "y"]));
        let second = slots(&mut texts, &names(&["a", "a"]));
        let evidence = evaluate(&[&first, &second], &texts);
        let IdentifierPattern::NotOneToOne(detail) = &evidence.identifier_pattern else {
            panic!("{evidence:?}");
        };
        assert!(
            detail.contains("`a` in member 2 aligns with both `x` and `y` in member 1"),
            "{detail}"
        );
        assert_eq!(
            evidence.warnings[0].code,
            WarningCode::IdentifierPatternDiffers
        );
    }

    #[test]
    fn identical_and_renamed_members_are_told_apart() {
        let mut texts = Texts::default();
        let a = slots(&mut texts, &names(&["x", "x", "y"]));
        let b = slots(&mut texts, &names(&["x", "x", "y"]));
        let c = slots(&mut texts, &names(&["p", "p", "q"]));
        let same = evaluate(&[&a, &b], &texts);
        assert_eq!(same.identifier_pattern, IdentifierPattern::Identical);
        assert!(same.differences.is_empty() && same.warnings.is_empty());
        let renamed = evaluate(&[&a, &b, &c], &texts);
        assert_eq!(renamed.identifier_pattern, IdentifierPattern::OneToOne);
        assert_eq!(renamed.differing_positions, [(DifferenceKind::Name, 3)]);
        // Equal per-member texts are one distinct difference.
        assert_eq!(renamed.distinct_differences, 2);
        assert_eq!(renamed.differences[0].occurrences, 2);
        assert_eq!(renamed.differences[0].values, ["x", "x", "p"]);
        assert_eq!(renamed.differences[0].lines, [1, 1, 1]);
        assert!(renamed.warnings.is_empty());
    }

    #[test]
    fn renamed_names_count_as_undeclared_unless_each_member_declares_them() {
        let mut texts = Texts::default();
        let a = slots(
            &mut texts,
            &[
                ("find_a", TokenRole::Declaration),
                ("find_a", TokenRole::Unclassified),
                ("x", TokenRole::Parameter),
                ("x", TokenRole::Unclassified),
                ("local", TokenRole::Unclassified),
                ("Prefix", TokenRole::Unclassified),
                ("same", TokenRole::Unclassified),
            ],
        );
        let b = slots(
            &mut texts,
            &[
                ("find_b", TokenRole::Declaration),
                ("find_b", TokenRole::Unclassified),
                ("y", TokenRole::Parameter),
                ("y", TokenRole::Unclassified),
                ("other", TokenRole::Unclassified),
                ("Fuzzy", TokenRole::Unclassified),
                ("same", TokenRole::Unclassified),
            ],
        );
        let evidence = evaluate(&[&a, &b], &texts);
        assert_eq!(evidence.identifier_pattern, IdentifierPattern::OneToOne);
        assert_eq!(
            evidence.differing_positions,
            [
                (DifferenceKind::Declaration, 1),
                (DifferenceKind::Parameter, 1),
                (DifferenceKind::Name, 4)
            ]
        );
        // `x`/`y` are parameters of their own member. The local, the
        // path-like reference and a path that repeats the member's own name
        // are not, so they are not known renames.
        assert_eq!(evidence.undeclared_name_positions, 3);
        let identical = evaluate(&[&a, &a], &texts);
        assert_eq!(identical.undeclared_name_positions, 0);
    }

    #[test]
    fn literal_categories_and_roles_decide_the_difference_kind() {
        use LiteralCategory::{Number, String};
        let mut texts = Texts::default();
        let a = slots(
            &mut texts,
            &[
                ("1", TokenRole::Literal(Number)),
                ("2", TokenRole::Literal(Number)),
                ("f", TokenRole::Callee),
                ("n", TokenRole::Member),
            ],
        );
        let b = slots(
            &mut texts,
            &[
                ("\"1\"", TokenRole::Literal(String)),
                ("3", TokenRole::Literal(Number)),
                ("g", TokenRole::Callee),
                ("m", TokenRole::Unclassified),
            ],
        );
        let evidence = evaluate(&[&a, &b], &texts);
        let kinds: Vec<DifferenceKind> = evidence.differences.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [
                DifferenceKind::LiteralCategory,
                DifferenceKind::LiteralValue,
                DifferenceKind::Callee,
                DifferenceKind::MixedRole,
            ]
        );
        assert_eq!(
            evidence.differences[0].values,
            ["1 (number)", "\"1\" (string)"]
        );
        let codes: Vec<WarningCode> = evidence.warnings.iter().map(|w| w.code).collect();
        assert_eq!(
            codes,
            [
                WarningCode::CalleeNamesDiffer,
                WarningCode::LiteralCategoriesDiffer,
                WarningCode::LiteralValuesDiffer,
            ]
        );
    }

    #[test]
    fn listing_is_capped_but_counts_are_not_and_long_text_is_cut() {
        let mut texts = Texts::default();
        let long = "y".repeat(MAX_DIFFERENCE_TEXT_CHARS + 5);
        let count = MAX_LISTED_DIFFERENCES + 3;
        let a: Vec<(String, TokenRole)> = (0..count)
            .map(|i| (format!("a{i}"), TokenRole::Unclassified))
            .collect();
        let mut b = a.clone();
        for (i, slot) in b.iter_mut().enumerate() {
            slot.0 = if i == 0 {
                long.clone()
            } else {
                format!("b{i}")
            };
        }
        let first = slots(&mut texts, &as_refs(&a));
        let second = slots(&mut texts, &as_refs(&b));
        let evidence = evaluate(&[&first, &second], &texts);
        assert_eq!(evidence.distinct_differences, count);
        assert_eq!(evidence.differences.len(), MAX_LISTED_DIFFERENCES);
        assert_eq!(
            evidence.differing_positions,
            [(DifferenceKind::Name, count)]
        );
        let shown = &evidence.differences[0].values[1];
        assert_eq!(shown.chars().count(), MAX_DIFFERENCE_TEXT_CHARS + 1);
        assert!(shown.ends_with('…'));
    }

    fn context(decorators: &[&str], enclosing: &[&str]) -> MemberContext {
        MemberContext {
            decorators: decorators.iter().map(|d| (*d).to_string()).collect(),
            enclosing: enclosing.iter().map(|e| (*e).to_string()).collect(),
        }
    }

    #[test]
    fn context_aspects_are_compared_separately_and_shown_once_when_shared() {
        let plain = context(&[], &["class"]);
        let cached = context(&["@cache"], &["class"]);
        let (same, warnings) = evaluate_context(&[Some(&plain), Some(&plain)]);
        assert_eq!(same.status, ContextStatus::Same);
        assert!(warnings.is_empty());
        assert_eq!(same.aspects[0].values, ["(none)"]);
        assert_eq!(same.aspects[1].values, ["class"]);

        let (differs, warnings) = evaluate_context(&[Some(&plain), Some(&cached), Some(&plain)]);
        assert_eq!(differs.status, ContextStatus::Differs);
        assert_eq!(
            differs.aspects[0],
            ContextComparison {
                aspect: ContextAspect::Decorators,
                differs: true,
                values: vec!["(none)".into(), "@cache".into(), "(none)".into()],
            }
        );
        assert!(!differs.aspects[1].differs);
        let codes: Vec<WarningCode> = warnings.iter().map(|w| w.code).collect();
        assert_eq!(codes, [WarningCode::DecoratorsDiffer]);

        let top = context(&[], &[]);
        let nested = context(&[], &["#[cfg(test)] mod", "fn"]);
        let (_, warnings) = evaluate_context(&[Some(&top), Some(&nested)]);
        assert_eq!(warnings[0].code, WarningCode::EnclosingContextDiffers);
        assert!(
            warnings[0]
                .detail
                .ends_with("(top level) / #[cfg(test)] mod > fn"),
            "{}",
            warnings[0].detail
        );
    }

    fn body(statements: usize, flat_calls: usize) -> Option<BodyShape> {
        Some(BodyShape {
            statements,
            flat_calls,
        })
    }

    #[test]
    fn only_groups_whose_every_member_is_call_only_are_demoted() {
        let demote = TrivialCallPolicy::Demote;
        let all = evaluate_priority(&[body(6, 6), body(6, 6)], demote);
        assert_eq!(
            (all.level, all.call_shape),
            (PriorityLevel::Demoted, CallShape::CallOnly)
        );
        assert!(all.reason.contains("(6 statements"), "{}", all.reason);
        assert!(all.reason.contains("callee names are not considered"));
        let single = evaluate_priority(&[body(1, 1), body(1, 1)], demote);
        assert!(single.reason.contains("(1 statement,"), "{}", single.reason);

        let kept = evaluate_priority(&[body(6, 6), body(7, 7)], TrivialCallPolicy::Keep);
        assert_eq!(
            (kept.level, kept.call_shape),
            (PriorityLevel::Normal, CallShape::CallOnly)
        );
        assert!(kept.reason.contains("(6-7 statements"), "{}", kept.reason);
        assert!(kept
            .reason
            .ends_with("kept in rank order by the keep policy"));

        let mixed = evaluate_priority(&[body(6, 6), body(6, 5), body(6, 6)], demote);
        assert_eq!(
            (mixed.level, mixed.call_shape),
            (PriorityLevel::Normal, CallShape::Mixed)
        );
        assert!(
            mixed.reason.starts_with("2 of 3 members"),
            "{}",
            mixed.reason
        );

        let other = evaluate_priority(&[body(6, 5), body(0, 0)], demote);
        assert_eq!(
            (other.level, other.call_shape),
            (PriorityLevel::Normal, CallShape::Other)
        );

        for bodies in [vec![body(6, 6), None], vec![body(6, 6)], Vec::new()] {
            let unknown = evaluate_priority(&bodies, demote);
            assert_eq!(
                (unknown.level, unknown.call_shape),
                (PriorityLevel::Normal, CallShape::NotEvaluated)
            );
            assert!(!unknown.reason.is_empty());
        }
    }

    #[test]
    fn missing_context_is_reported_not_compared() {
        let plain = context(&[], &[]);
        for members in [vec![Some(&plain), None], vec![Some(&plain)], Vec::new()] {
            let (evidence, warnings) = evaluate_context(&members);
            assert_eq!(evidence.status, ContextStatus::NotEvaluated);
            assert!(evidence.aspects.is_empty());
            assert_eq!(warnings[0].code, WarningCode::ContextUnavailable);
        }
    }

    #[test]
    fn misaligned_members_are_reported_not_compared() {
        let mut texts = Texts::default();
        let a = slots(&mut texts, &names(&["x", "y"]));
        let b = slots(&mut texts, &names(&["x"]));
        for members in [vec![&a[..], &b[..]], vec![&a[..]], Vec::new()] {
            let evidence = evaluate(&members, &texts);
            assert_eq!(evidence.identifier_pattern, IdentifierPattern::NotEvaluated);
            assert_eq!(evidence.warnings[0].code, WarningCode::AlignmentUnavailable);
            assert!(evidence.differences.is_empty());
        }
    }
}
