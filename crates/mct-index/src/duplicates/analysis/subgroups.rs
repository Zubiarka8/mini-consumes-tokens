//! Evidence-based subgroups of one group. Members already share a normalized
//! sequence; this partitions them by the aligned evidence that separates them
//! (see [`GroupSubgroups`] for the rule). It never changes grouping, never
//! resolves a name and never scores anything.

use std::collections::{BTreeSet, HashMap, HashSet};

use mct_core::TokenRole;

use super::super::{
    DifferenceKind, GroupSubgroups, Subgroup, SubgroupBasis, SubgroupStatus, MAX_LISTED_SUBGROUPS,
    MAX_LISTED_SUBGROUP_MEMBERS, MAX_SUBGROUP_VALUES,
};
use super::evidence::{cut, difference_kind, MemberInput, Slot, Texts};

/// Partitions a group from its members' inputs, in member order. A missing
/// input means the members cannot be aligned: not evaluated. Decorators and
/// enclosing constructs are not read: they stay in the `context` block.
pub(super) fn partition_inputs(members: &[Option<&MemberInput>], texts: &Texts) -> GroupSubgroups {
    let Some(inputs) = members.iter().copied().collect::<Option<Vec<_>>>() else {
        return GroupSubgroups::default();
    };
    let slots: Vec<&[Slot]> = inputs.iter().map(|input| input.slots.as_slice()).collect();
    partition(&slots, texts)
}

/// One member's value at one separating item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Value {
    /// Interned token text.
    Text(u32),
    /// A literal's role, which carries its category.
    Category(TokenRole),
    /// 1-based number of the member's identifier pattern, in member order.
    Pattern(usize),
}

/// An aligned position, or the identifier pattern, whose values differ
/// between members.
struct Item {
    basis: SubgroupBasis,
    /// One value per member, in member order.
    values: Vec<Value>,
}

/// Partitions `members` (one slot list per member, in member order) by the
/// separating items.
pub(super) fn partition(members: &[&[Slot]], texts: &Texts) -> GroupSubgroups {
    let first = members.first().copied().unwrap_or_default();
    if members.len() < 2 || members.iter().any(|slots| slots.len() != first.len()) {
        return GroupSubgroups::default();
    }
    let items = separating_items(members);

    // Classes in order of their first member; equal values put members together.
    let mut classes: Vec<Vec<usize>> = Vec::new();
    let mut class_of: HashMap<Vec<&Value>, usize> = HashMap::new();
    for member in 0..members.len() {
        let key: Vec<&Value> = items.iter().map(|item| &item.values[member]).collect();
        let next = classes.len();
        let class = *class_of.entry(key).or_insert(next);
        if class == next {
            classes.push(Vec::new());
        }
        classes[class].push(member);
    }

    if classes.len() < 2 {
        return GroupSubgroups {
            status: SubgroupStatus::Single,
            count: 1,
            ..GroupSubgroups::default()
        };
    }
    let all_distinct = classes.len() == members.len();
    let subgroups = if all_distinct {
        Vec::new()
    } else {
        classes
            .iter()
            .take(MAX_LISTED_SUBGROUPS)
            .enumerate()
            .map(|(index, class)| Subgroup {
                id: index + 1,
                size: class.len(),
                members: class
                    .iter()
                    .take(MAX_LISTED_SUBGROUP_MEMBERS)
                    .copied()
                    .collect(),
                values: items
                    .iter()
                    .take(MAX_SUBGROUP_VALUES)
                    .map(|item| {
                        let value = class.first().map(|&member| &item.values[member]);
                        format!("{}={}", item.basis.as_str(), show(value, texts))
                    })
                    .collect(),
            })
            .collect()
    };
    GroupSubgroups {
        status: SubgroupStatus::Split,
        basis: items
            .iter()
            .map(|item| item.basis)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        count: classes.len(),
        separating_items: items.len(),
        all_distinct,
        subgroups,
    }
}

/// Aligned positions in source order, then the identifier pattern.
fn separating_items(members: &[&[Slot]]) -> Vec<Item> {
    // Names each member binds in its own signature: its declaration name and
    // parameters. Spellings only; nothing is resolved.
    let bound: Vec<HashSet<u32>> = members
        .iter()
        .map(|slots| {
            slots
                .iter()
                .filter(|slot| {
                    !slot.literal
                        && matches!(slot.role, TokenRole::Declaration | TokenRole::Parameter)
                })
                .map(|slot| slot.text)
                .collect()
        })
        .collect();
    let mut items = Vec::new();
    let positions = members.first().map_or(0, |slots| slots.len());
    for position in 0..positions {
        let column: Vec<Slot> = members.iter().map(|slots| slots[position]).collect();
        if column.iter().all(|slot| slot.text == column[0].text) {
            continue;
        }
        let basis = match difference_kind(&column) {
            DifferenceKind::Member => SubgroupBasis::Member,
            DifferenceKind::Callee => SubgroupBasis::Callee,
            DifferenceKind::Type => SubgroupBasis::Type,
            DifferenceKind::LiteralCategory => {
                items.push(Item {
                    basis: SubgroupBasis::LiteralCategory,
                    values: column
                        .iter()
                        .map(|slot| Value::Category(slot.role))
                        .collect(),
                });
                continue;
            }
            _ => continue,
        };
        // A name every member binds itself (a called parameter, a recursive
        // call) is renaming, not a different target.
        if column
            .iter()
            .zip(&bound)
            .all(|(slot, names)| names.contains(&slot.text))
        {
            continue;
        }
        items.push(Item {
            basis,
            values: column.iter().map(|slot| Value::Text(slot.text)).collect(),
        });
    }

    // Identifier pattern: each spelling replaced by the order of its first
    // use. Equal patterns are exactly the consistent renamings of each other,
    // so this relation is transitive, unlike a pairwise check.
    let mut pattern_of: HashMap<Vec<usize>, usize> = HashMap::new();
    let patterns: Vec<usize> = members
        .iter()
        .map(|slots| {
            let mut first_use: HashMap<u32, usize> = HashMap::new();
            let shape: Vec<usize> = slots
                .iter()
                .filter(|slot| !slot.literal)
                .map(|slot| {
                    let next = first_use.len();
                    *first_use.entry(slot.text).or_insert(next)
                })
                .collect();
            let next = pattern_of.len() + 1;
            *pattern_of.entry(shape).or_insert(next)
        })
        .collect();
    if pattern_of.len() > 1 {
        items.push(Item {
            basis: SubgroupBasis::IdentifierPattern,
            values: patterns.into_iter().map(Value::Pattern).collect(),
        });
    }
    items
}

fn show(value: Option<&Value>, texts: &Texts) -> String {
    match value {
        Some(Value::Text(id)) => cut(texts.get(*id)),
        Some(Value::Category(role)) => role.as_str().to_string(),
        Some(Value::Pattern(number)) => format!("#{number}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use mct_core::LiteralCategory;

    use super::super::evidence::MemberContext;
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

    /// `def <name>(<param>): <param>.<member> = <callee>(<param>)`, as slots.
    fn member(texts: &mut Texts, name: &str, param: &str, member: &str, callee: &str) -> Vec<Slot> {
        slots(
            texts,
            &[
                (name, TokenRole::Declaration),
                (param, TokenRole::Parameter),
                (param, TokenRole::Unclassified),
                (member, TokenRole::Member),
                (callee, TokenRole::Callee),
                (param, TokenRole::Unclassified),
            ],
        )
    }

    fn run(members: &[Vec<Slot>], texts: &Texts) -> GroupSubgroups {
        let refs: Vec<&[Slot]> = members.iter().map(Vec::as_slice).collect();
        partition(&refs, texts)
    }

    #[test]
    fn renaming_only_never_splits() {
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "load_a", "user", "name", "fetch"),
            member(&mut texts, "load_b", "item", "name", "fetch"),
            member(&mut texts, "load_c", "row", "name", "fetch"),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.status, SubgroupStatus::Single);
        assert_eq!(result.count, 1);
        assert!(result.basis.is_empty() && result.subgroups.is_empty());

        // A called parameter and a recursive call are renamings too.
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "walk", "visit", "name", "visit"),
            member(&mut texts, "crawl", "see", "name", "see"),
        ];
        assert_eq!(run(&members, &texts).status, SubgroupStatus::Single);
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "fact", "n", "name", "fact"),
            member(&mut texts, "count", "k", "name", "count"),
        ];
        assert_eq!(run(&members, &texts).status, SubgroupStatus::Single);
    }

    #[test]
    fn a_callee_bound_by_only_some_members_still_separates() {
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "a", "visit", "name", "visit"),
            member(&mut texts, "b", "item", "name", "print"),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.status, SubgroupStatus::Split);
        // Calling the parameter also changes which spellings repeat.
        assert_eq!(
            result.basis,
            [SubgroupBasis::Callee, SubgroupBasis::IdentifierPattern]
        );
    }

    #[test]
    fn a_mixed_group_splits_into_two_families() {
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "a", "x", "user_id", "load_user"),
            member(&mut texts, "b", "y", "order_id", "load_order"),
            member(&mut texts, "c", "z", "user_id", "load_user"),
            member(&mut texts, "d", "w", "order_id", "load_order"),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.status, SubgroupStatus::Split);
        assert_eq!(result.basis, [SubgroupBasis::Member, SubgroupBasis::Callee]);
        assert_eq!((result.count, result.separating_items), (2, 2));
        assert!(!result.all_distinct);
        assert_eq!(
            result.subgroups,
            [
                Subgroup {
                    id: 1,
                    size: 2,
                    members: vec![0, 2],
                    values: vec!["member=user_id".into(), "callee=load_user".into()],
                },
                Subgroup {
                    id: 2,
                    size: 2,
                    members: vec![1, 3],
                    values: vec!["member=order_id".into(), "callee=load_order".into()],
                },
            ]
        );
    }

    #[test]
    fn three_way_split_combines_every_separating_kind() {
        use LiteralCategory::{Number, String};
        let mut texts = Texts::default();
        let make = |texts: &mut Texts, callee: &str, literal: (&str, LiteralCategory)| {
            slots(
                texts,
                &[
                    (callee, TokenRole::Callee),
                    (literal.0, TokenRole::Literal(literal.1)),
                ],
            )
        };
        let members = vec![
            make(&mut texts, "save", ("1", Number)),
            make(&mut texts, "save", ("\"1\"", String)),
            make(&mut texts, "send", ("2", Number)),
            make(&mut texts, "save", ("3", Number)),
            make(&mut texts, "send", ("4", Number)),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.status, SubgroupStatus::Split);
        assert_eq!(
            result.basis,
            [SubgroupBasis::Callee, SubgroupBasis::LiteralCategory]
        );
        let rows: Vec<(usize, Vec<usize>)> = result
            .subgroups
            .iter()
            .map(|row| (row.size, row.members.clone()))
            .collect();
        // Literal values (1 / 3) never separate; categories (number / string) do.
        assert_eq!(rows, [(2, vec![0, 3]), (1, vec![1]), (2, vec![2, 4])]);
        assert_eq!(
            result.subgroups[1].values,
            ["callee=save", "literal-category=string"]
        );
    }

    #[test]
    fn every_member_its_own_class_is_reported_as_all_distinct() {
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "a", "x", "name", "alpha"),
            member(&mut texts, "b", "x", "name", "beta"),
            member(&mut texts, "c", "x", "name", "gamma"),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.status, SubgroupStatus::Split);
        assert!(result.all_distinct);
        assert_eq!(result.count, 3);
        assert!(result.subgroups.is_empty());
    }

    #[test]
    fn listing_is_bounded_while_counts_and_sizes_cover_everything() {
        let mut texts = Texts::default();
        let classes = MAX_LISTED_SUBGROUPS + 3;
        let big = MAX_LISTED_SUBGROUP_MEMBERS + 5;
        let mut members = Vec::new();
        for _ in 0..big {
            members.push(member(&mut texts, "f", "x", "name", "c0"));
        }
        for class in 1..classes {
            for _ in 0..2 {
                members.push(member(&mut texts, "f", "x", "name", &format!("c{class}")));
            }
        }
        let long = "m".repeat(80);
        for slots in &mut members[..big] {
            slots[3].text = texts.id(&long);
        }
        let result = run(&members, &texts);
        assert_eq!(result.count, classes);
        assert_eq!(result.subgroups.len(), MAX_LISTED_SUBGROUPS);
        assert_eq!(result.subgroups[0].size, big);
        assert_eq!(
            result.subgroups[0].members.len(),
            MAX_LISTED_SUBGROUP_MEMBERS
        );
        assert!(result.subgroups[0].values[0].ends_with('…'));
        assert!(result
            .subgroups
            .iter()
            .all(|row| row.values.len() <= MAX_SUBGROUP_VALUES));
    }

    #[test]
    fn the_partition_is_deterministic_and_follows_member_order() {
        let mut texts = Texts::default();
        let members = vec![
            member(&mut texts, "a", "x", "name", "zeta"),
            member(&mut texts, "b", "x", "name", "alpha"),
            member(&mut texts, "c", "x", "name", "zeta"),
        ];
        let once = run(&members, &texts);
        assert_eq!(once, run(&members, &texts));
        // Ids follow the first member of each subgroup, not the text.
        assert_eq!(once.subgroups[0].values, ["callee=zeta"]);
        assert_eq!(once.subgroups[0].members, [0, 2]);
    }

    #[test]
    fn a_not_one_to_one_pattern_separates_while_consistent_renamings_stay_together() {
        // `y * x` against `y * y`: same shape under broad, different data flow.
        let mut texts = Texts::default();
        let shape = |texts: &mut Texts, names: [&str; 4]| {
            slots(
                texts,
                &[
                    (names[0], TokenRole::Declaration),
                    (names[1], TokenRole::Parameter),
                    (names[2], TokenRole::Unclassified),
                    (names[3], TokenRole::Unclassified),
                ],
            )
        };
        let members = vec![
            shape(&mut texts, ["scale", "x", "x", "x"]),
            shape(&mut texts, ["skew", "x", "y", "x"]),
            shape(&mut texts, ["grow", "v", "v", "v"]),
        ];
        let result = run(&members, &texts);
        assert_eq!(result.basis, [SubgroupBasis::IdentifierPattern]);
        assert_eq!(result.separating_items, 1);
        let rows: Vec<(Vec<usize>, Vec<String>)> = result
            .subgroups
            .iter()
            .map(|row| (row.members.clone(), row.values.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                (vec![0, 2], vec!["identifier-pattern=#1".to_string()]),
                (vec![1], vec!["identifier-pattern=#2".to_string()]),
            ]
        );
    }

    #[test]
    fn decorators_and_enclosing_constructs_never_separate() {
        let mut texts = Texts::default();
        let plain = MemberInput {
            slots: member(&mut texts, "a", "x", "name", "f"),
            context: Some(MemberContext::default()),
            body: None,
        };
        let cached = MemberInput {
            slots: member(&mut texts, "b", "y", "name", "f"),
            context: Some(MemberContext {
                decorators: vec!["@cache".into()],
                enclosing: vec!["#[cfg(test)] mod".into()],
            }),
            body: None,
        };
        let result = partition_inputs(&[Some(&plain), Some(&cached)], &texts);
        assert_eq!(result.status, SubgroupStatus::Single);
    }

    #[test]
    fn misaligned_or_missing_members_are_not_evaluated() {
        let mut texts = Texts::default();
        let a = member(&mut texts, "a", "x", "name", "f");
        let b = slots(&mut texts, &[("b", TokenRole::Declaration)]);
        for members in [vec![a.clone(), b], vec![a], Vec::new()] {
            let result = run(&members, &texts);
            assert_eq!(result, GroupSubgroups::default());
            assert_eq!(result.status, SubgroupStatus::NotEvaluated);
        }
        assert_eq!(
            partition_inputs(&[None, None], &texts).status,
            SubgroupStatus::NotEvaluated
        );
    }

    #[test]
    fn mixed_roles_names_and_literal_values_never_separate() {
        use LiteralCategory::Number;
        let mut texts = Texts::default();
        let a = slots(
            &mut texts,
            &[
                ("f", TokenRole::Callee),
                ("local", TokenRole::Unclassified),
                ("1", TokenRole::Literal(Number)),
            ],
        );
        let b = slots(
            &mut texts,
            &[
                ("g", TokenRole::Member),
                ("other", TokenRole::Unclassified),
                ("2", TokenRole::Literal(Number)),
            ],
        );
        assert_eq!(run(&[a, b], &texts).status, SubgroupStatus::Single);
    }
}
