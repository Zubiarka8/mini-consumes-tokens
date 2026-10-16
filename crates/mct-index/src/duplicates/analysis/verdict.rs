//! Explainable verdicts (ADR-010): one level per reported group, derived only
//! from the group's existing evidence and priority by a fixed rule table. No
//! score, weight or probability is involved, and nothing here reads source,
//! resolves a name or changes a group.
//!
//! Rule table, applied in [`VerdictReason`] order:
//!
//! | Evidence | Reason |
//! |---|---|
//! | identifier pattern not evaluated or alignment unavailable | `alignment-unavailable` |
//! | identifier pattern not one-to-one | `identifier-pattern-differs` |
//! | a renamed name that is not a parameter of its own member | `undeclared-names-differ` |
//! | a differing position whose members disagree on the role | `mixed-role-differences` |
//! | member / callee / type-name differences | `member-names-differ` / `callee-names-differ` / `type-names-differ` |
//! | primitive type spelling conflicts | `primitive-type-spellings-differ` |
//! | literal category / value / interpolated-string differences | `literal-categories-differ` / `literal-values-differ` / `interpolated-strings-differ` |
//! | context not evaluated / decorators differ / enclosing differs | `context-unavailable` / `decorators-differ` / `enclosing-context-differs` |
//! | every body only flat calls / only some / shape unknown | `call-only-bodies` / `call-shape-mixed` / `body-shape-not-evaluated` |
//!
//! Any of these makes the group `SUSPICIOUS`, listing every one that fired.
//! Otherwise the group is `REAL`: its identifiers are identical or a
//! one-to-one renaming limited to the declaration names and each member's
//! own parameters (and their uses),
//! its context is the same and its bodies are not only flat calls; a
//! docstring difference is listed but does not make it suspicious.
//! `FALSE_POSITIVE` is never produced: no available evidence is decisive
//! enough (see ADR-010 for the candidates and why each was rejected).

use super::super::{
    CallShape, ContextAspect, ContextStatus, DifferenceKind, GroupEvidence, GroupPriority,
    GroupVerdict, IdentifierPattern, VerdictLevel, VerdictReason, WarningCode,
};

/// The verdict of one group from its evidence and priority.
pub(super) fn classify(evidence: &GroupEvidence, priority: &GroupPriority) -> GroupVerdict {
    let mut suspicious = Vec::new();
    let differs = |kind: DifferenceKind| {
        evidence
            .differing_positions
            .iter()
            .any(|(k, count)| *k == kind && *count > 0)
    };

    let alignment_unavailable = evidence
        .warnings
        .iter()
        .any(|warning| warning.code == WarningCode::AlignmentUnavailable);
    match &evidence.identifier_pattern {
        IdentifierPattern::NotEvaluated => suspicious.push(VerdictReason::AlignmentUnavailable),
        _ if alignment_unavailable => suspicious.push(VerdictReason::AlignmentUnavailable),
        IdentifierPattern::NotOneToOne(_) => {
            suspicious.push(VerdictReason::IdentifierPatternDiffers);
        }
        IdentifierPattern::Identical | IdentifierPattern::OneToOne => {}
    }
    if evidence.undeclared_name_positions > 0 {
        suspicious.push(VerdictReason::UndeclaredNamesDiffer);
    }
    for (kind, reason) in [
        (
            DifferenceKind::MixedRole,
            VerdictReason::MixedRoleDifferences,
        ),
        (DifferenceKind::Member, VerdictReason::MemberNamesDiffer),
        (DifferenceKind::Callee, VerdictReason::CalleeNamesDiffer),
        (DifferenceKind::Type, VerdictReason::TypeNamesDiffer),
    ] {
        if differs(kind) {
            suspicious.push(reason);
        }
    }
    if evidence.types.primitive_spelling_conflicts > 0 {
        suspicious.push(VerdictReason::PrimitiveTypeSpellingsDiffer);
    }
    for (kind, reason) in [
        (
            DifferenceKind::LiteralCategory,
            VerdictReason::LiteralCategoriesDiffer,
        ),
        (
            DifferenceKind::LiteralValue,
            VerdictReason::LiteralValuesDiffer,
        ),
        (
            DifferenceKind::InterpolatedString,
            VerdictReason::InterpolatedStringsDiffer,
        ),
    ] {
        if differs(kind) {
            suspicious.push(reason);
        }
    }
    match evidence.context.status {
        ContextStatus::NotEvaluated => suspicious.push(VerdictReason::ContextUnavailable),
        ContextStatus::Same => {}
        ContextStatus::Differs => {
            let before = suspicious.len();
            for (aspect, reason) in [
                (ContextAspect::Decorators, VerdictReason::DecoratorsDiffer),
                (
                    ContextAspect::Enclosing,
                    VerdictReason::EnclosingContextDiffers,
                ),
            ] {
                if evidence
                    .context
                    .aspects
                    .iter()
                    .any(|row| row.aspect == aspect && row.differs)
                {
                    suspicious.push(reason);
                }
            }
            // A difference without a differing row cannot be explained.
            if suspicious.len() == before {
                suspicious.push(VerdictReason::ContextUnavailable);
            }
        }
    }
    match priority.call_shape {
        CallShape::CallOnly => suspicious.push(VerdictReason::CallOnlyBodies),
        CallShape::Mixed => suspicious.push(VerdictReason::CallShapeMixed),
        CallShape::NotEvaluated => suspicious.push(VerdictReason::BodyShapeNotEvaluated),
        CallShape::Other => {}
    }

    if !suspicious.is_empty() {
        suspicious.sort_unstable();
        suspicious.dedup();
        return GroupVerdict {
            level: VerdictLevel::Suspicious,
            reasons: suspicious,
        };
    }
    let mut reasons = vec![
        if evidence.identifier_pattern == IdentifierPattern::Identical {
            VerdictReason::IdenticalTokens
        } else {
            VerdictReason::ConsistentRenaming
        },
        VerdictReason::SameContext,
    ];
    if differs(DifferenceKind::Docstring) {
        reasons.push(VerdictReason::DocstringsDiffer);
    }
    GroupVerdict {
        level: VerdictLevel::Real,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::super::super::{ContextComparison, ContextEvidence, EvidenceWarning, TypeEvidence};
    use super::*;

    /// Evidence of a group with a consistent renaming, the same context and
    /// a body that is not only flat calls: the one shape that is `REAL`.
    fn clean() -> (GroupEvidence, GroupPriority) {
        let evidence = GroupEvidence {
            identifier_pattern: IdentifierPattern::OneToOne,
            differing_positions: vec![
                (DifferenceKind::Declaration, 1),
                (DifferenceKind::Parameter, 2),
                (DifferenceKind::Name, 3),
            ],
            distinct_differences: 3,
            context: ContextEvidence {
                status: ContextStatus::Same,
                aspects: vec![
                    ContextComparison {
                        aspect: ContextAspect::Decorators,
                        differs: false,
                        values: vec!["(none)".into()],
                    },
                    ContextComparison {
                        aspect: ContextAspect::Enclosing,
                        differs: false,
                        values: vec!["(top level)".into()],
                    },
                ],
            },
            ..GroupEvidence::default()
        };
        let priority = GroupPriority {
            call_shape: CallShape::Other,
            ..GroupPriority::default()
        };
        (evidence, priority)
    }

    fn reasons(verdict: &GroupVerdict) -> Vec<&'static str> {
        verdict
            .reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect()
    }

    #[test]
    fn consistent_renaming_in_the_same_context_is_real() {
        let (evidence, priority) = clean();
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Real);
        assert_eq!(reasons(&verdict), ["consistent-renaming", "same-context"]);
    }

    #[test]
    fn identical_tokens_are_real_and_say_so() {
        let (mut evidence, priority) = clean();
        evidence.identifier_pattern = IdentifierPattern::Identical;
        evidence.differing_positions.clear();
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Real);
        assert_eq!(reasons(&verdict), ["identical-tokens", "same-context"]);
    }

    #[test]
    fn docstring_differences_are_listed_without_suspicion() {
        let (mut evidence, priority) = clean();
        evidence
            .differing_positions
            .push((DifferenceKind::Docstring, 1));
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Real);
        assert_eq!(
            reasons(&verdict),
            ["consistent-renaming", "same-context", "docstrings-differ"]
        );
    }

    #[test]
    fn equal_type_spellings_alone_do_not_make_a_group_suspicious() {
        let (mut evidence, priority) = clean();
        evidence.types = TypeEvidence {
            compared_positions: 4,
            ..TypeEvidence::default()
        };
        assert_eq!(classify(&evidence, &priority).level, VerdictLevel::Real);
    }

    #[test]
    fn every_difference_kind_the_detector_cannot_judge_is_suspicious() {
        for (kind, expected) in [
            (DifferenceKind::MixedRole, "mixed-role-differences"),
            (DifferenceKind::Member, "member-names-differ"),
            (DifferenceKind::Callee, "callee-names-differ"),
            (DifferenceKind::Type, "type-names-differ"),
            (DifferenceKind::LiteralCategory, "literal-categories-differ"),
            (DifferenceKind::LiteralValue, "literal-values-differ"),
            (
                DifferenceKind::InterpolatedString,
                "interpolated-strings-differ",
            ),
        ] {
            let (mut evidence, priority) = clean();
            evidence.differing_positions.push((kind, 1));
            let verdict = classify(&evidence, &priority);
            assert_eq!(verdict.level, VerdictLevel::Suspicious, "{kind:?}");
            assert_eq!(reasons(&verdict), [expected], "{kind:?}");
        }
    }

    #[test]
    fn a_zero_count_is_not_a_difference() {
        let (mut evidence, priority) = clean();
        evidence
            .differing_positions
            .push((DifferenceKind::Callee, 0));
        assert_eq!(classify(&evidence, &priority).level, VerdictLevel::Real);
    }

    #[test]
    fn identifier_pattern_rules() {
        let (mut evidence, priority) = clean();
        evidence.identifier_pattern = IdentifierPattern::NotOneToOne("x".into());
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["identifier-pattern-differs"]
        );

        evidence.identifier_pattern = IdentifierPattern::NotEvaluated;
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Suspicious);
        assert_eq!(reasons(&verdict), ["alignment-unavailable"]);

        // The warning wins over a pattern that a failed alignment left behind.
        evidence.identifier_pattern = IdentifierPattern::OneToOne;
        evidence.warnings.push(EvidenceWarning {
            code: WarningCode::AlignmentUnavailable,
            detail: String::new(),
        });
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["alignment-unavailable"]
        );
    }

    #[test]
    fn renamed_names_a_member_does_not_declare_are_suspicious() {
        let (mut evidence, priority) = clean();
        evidence.undeclared_name_positions = 1;
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Suspicious);
        assert_eq!(reasons(&verdict), ["undeclared-names-differ"]);
    }

    #[test]
    fn primitive_spelling_conflicts_are_suspicious_with_unknown_compatibility() {
        let (mut evidence, priority) = clean();
        evidence.differing_positions.push((DifferenceKind::Type, 1));
        evidence.types.compared_positions = 1;
        evidence.types.primitive_spelling_conflicts = 1;
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Suspicious);
        assert_eq!(
            reasons(&verdict),
            ["type-names-differ", "primitive-type-spellings-differ"]
        );
    }

    #[test]
    fn context_rules() {
        let (mut evidence, priority) = clean();
        evidence.context.status = ContextStatus::Differs;
        evidence.context.aspects[0].differs = true;
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["decorators-differ"]
        );
        evidence.context.aspects[1].differs = true;
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["decorators-differ", "enclosing-context-differs"]
        );
        evidence.context.aspects[0].differs = false;
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["enclosing-context-differs"]
        );
        // A difference that no row explains is treated as unknown context.
        evidence.context.aspects.clear();
        assert_eq!(
            reasons(&classify(&evidence, &priority)),
            ["context-unavailable"]
        );
        evidence.context = ContextEvidence::default();
        let verdict = classify(&evidence, &priority);
        assert_eq!(verdict.level, VerdictLevel::Suspicious);
        assert_eq!(reasons(&verdict), ["context-unavailable"]);
    }

    #[test]
    fn call_shape_rules_do_not_depend_on_the_ranking_policy() {
        for (shape, expected) in [
            (CallShape::CallOnly, Some("call-only-bodies")),
            (CallShape::Mixed, Some("call-shape-mixed")),
            (CallShape::NotEvaluated, Some("body-shape-not-evaluated")),
            (CallShape::Other, None),
        ] {
            for level in [
                super::super::super::PriorityLevel::Normal,
                super::super::super::PriorityLevel::Demoted,
            ] {
                let (evidence, mut priority) = clean();
                priority.call_shape = shape;
                priority.level = level;
                let verdict = classify(&evidence, &priority);
                match expected {
                    Some(code) => {
                        assert_eq!(verdict.level, VerdictLevel::Suspicious);
                        assert_eq!(reasons(&verdict), [code]);
                    }
                    None => assert_eq!(verdict.level, VerdictLevel::Real),
                }
            }
        }
    }

    #[test]
    fn every_fired_rule_is_listed_once_in_a_stable_order() {
        let (mut evidence, mut priority) = clean();
        evidence.identifier_pattern = IdentifierPattern::NotOneToOne("x".into());
        evidence.differing_positions = vec![
            (DifferenceKind::Member, 2),
            (DifferenceKind::Callee, 1),
            (DifferenceKind::LiteralValue, 3),
        ];
        evidence.context = ContextEvidence::default();
        priority.call_shape = CallShape::CallOnly;
        let verdict = classify(&evidence, &priority);
        assert_eq!(
            reasons(&verdict),
            [
                "identifier-pattern-differs",
                "member-names-differ",
                "callee-names-differ",
                "literal-values-differ",
                "context-unavailable",
                "call-only-bodies",
            ]
        );
        assert_eq!(verdict, classify(&evidence, &priority));
    }

    /// Exhaustive over the evidence dimensions the rules read: `REAL` exactly
    /// when no suspicion rule fires, and never `FALSE_POSITIVE`.
    #[test]
    fn no_evidence_combination_is_classified_as_a_false_positive() {
        let patterns = [
            IdentifierPattern::NotEvaluated,
            IdentifierPattern::Identical,
            IdentifierPattern::OneToOne,
            IdentifierPattern::NotOneToOne("x".into()),
        ];
        let kinds = [
            DifferenceKind::Declaration,
            DifferenceKind::Parameter,
            DifferenceKind::Name,
            DifferenceKind::Member,
            DifferenceKind::Callee,
            DifferenceKind::Type,
            DifferenceKind::LiteralValue,
            DifferenceKind::LiteralCategory,
            DifferenceKind::InterpolatedString,
            DifferenceKind::Docstring,
            DifferenceKind::MixedRole,
        ];
        let harmless = |kind: &DifferenceKind| {
            matches!(
                kind,
                DifferenceKind::Declaration
                    | DifferenceKind::Parameter
                    | DifferenceKind::Name
                    | DifferenceKind::Docstring
            )
        };
        let statuses = [
            ContextStatus::NotEvaluated,
            ContextStatus::Same,
            ContextStatus::Differs,
        ];
        let shapes = [
            CallShape::NotEvaluated,
            CallShape::Other,
            CallShape::CallOnly,
            CallShape::Mixed,
        ];
        let mut real = 0;
        for pattern in &patterns {
            for kind in kinds.iter().map(Some).chain([None]) {
                for status in statuses {
                    for shape in shapes {
                        for conflicts in [0, 1] {
                            for undeclared in [0, 1] {
                                let (mut evidence, mut priority) = clean();
                                evidence.undeclared_name_positions = undeclared;
                                evidence.identifier_pattern = pattern.clone();
                                evidence.differing_positions =
                                    kind.map(|k| (*k, 1)).into_iter().collect();
                                evidence.context.status = status;
                                if status == ContextStatus::Differs {
                                    evidence.context.aspects[1].differs = true;
                                }
                                evidence.types.primitive_spelling_conflicts = conflicts;
                                priority.call_shape = shape;
                                let verdict = classify(&evidence, &priority);
                                assert_ne!(verdict.level, VerdictLevel::FalsePositive);
                                let expect_real = matches!(
                                    pattern,
                                    IdentifierPattern::Identical | IdentifierPattern::OneToOne
                                ) && kind.is_none_or(harmless)
                                    && status == ContextStatus::Same
                                    && shape == CallShape::Other
                                    && conflicts == 0
                                    && undeclared == 0;
                                assert_eq!(
                                verdict.level == VerdictLevel::Real,
                                expect_real,
                                "{verdict:?} for {pattern:?} {kind:?} {status:?} {shape:?} {conflicts} {undeclared}"
                            );
                                assert!(!verdict.reasons.is_empty());
                                real += usize::from(expect_real);
                            }
                        }
                    }
                }
            }
        }
        // Identical/one-to-one x (no difference + four harmless kinds).
        assert_eq!(real, 2 * 5);
    }
}
