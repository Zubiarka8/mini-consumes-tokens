//! Generated bounded-domain invariants; no additional property-test dependency.
use super::*;

#[test]
fn generated_nested_callables_keep_nonoverlapping_members_and_stable_groups() {
    for copies in 2..=8 {
        for padding in [0, 1, 3] {
            let root = tempfile::tempdir().unwrap();
            let mut source = String::new();
            for id in 0..copies {
                source.push_str(&"\n".repeat(padding));
                source.push_str(&format!("fn outer_{id}(value: i64) -> i64 {{\n fn inner_{id}(x: i64) -> i64 {{\n let mut total = x;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 {{ total -= 3; }}\n total += 8;\n total\n }}\n inner_{id}(value)\n}}\n"));
            }
            fs::write(root.path().join("nested.rs"), source).unwrap();
            index(root.path());
            let sources = mct_languages::build_token_sources();
            let options = mct_index::DuplicateOptions::default();
            let server = Server::start(root.path(), true);
            let site = server.site.borrow();
            let report =
                mct_index::find_duplicate_groups(&site.index, &sources, options.clone()).unwrap();
            assert_eq!(report.groups.len(), 2, "copies={copies}, padding={padding}");
            for group in &report.groups {
                assert_eq!(group.members.len(), copies);
                for (id, member) in group.members.iter().enumerate() {
                    for other in &group.members[id + 1..] {
                        assert!(
                            member.path != other.path
                                || member.end_line < other.start_line
                                || other.end_line < member.start_line,
                            "overlapping members: {member:?} / {other:?}"
                        );
                    }
                }
            }
            let again = mct_index::find_duplicate_groups(&site.index, &sources, options).unwrap();
            assert_eq!(report, again);
        }
    }
}

#[test]
fn generated_row_counts_match_unique_paths_and_preview_accounting() {
    let root = project();
    let server = Server::start(root.path(), true);
    let template = server.site.borrow().duplicates.groups[0].members[0].clone();
    // Every partition of eight locations between a shared path and distinct paths.
    for mask in 0u32..256 {
        let mut members: Vec<_> = (0..8)
            .map(|id| {
                let mut member = template.clone();
                member.path = if mask & (1 << id) == 0 {
                    "shared.rs".to_string()
                } else {
                    format!("other/member_{id}.rs")
                };
                member.symbol_name = format!("member_{id}");
                member.start_line = id * 20 + 1;
                member.end_line = id * 20 + 10;
                member
            })
            .collect();
        members.sort_by(|a, b| (&a.path, a.start_line).cmp(&(&b.path, b.start_line)));
        let unique = members
            .iter()
            .map(|member| member.path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        server.site.borrow_mut().duplicates.groups[0].members = members;
        let row = server.json("api/findings?type=duplicate")["items"][0].clone();
        assert_eq!(row["memberCount"], 8);
        assert_eq!(row["fileCount"], unique, "mask={mask}");
        assert_eq!(
            row["members"].as_array().unwrap().len() as u64
                + row["membersOmitted"].as_u64().unwrap(),
            8
        );
    }
}
