//! Duplicate row contracts, using the production detector and HTTP boundary.
use super::*;

#[test]
fn same_file_locations_are_counted_separately_and_all_names_are_visible() {
    let root = project();
    let source = fs::read_to_string(root.path().join("src/a.rs")).unwrap();
    fs::remove_file(root.path().join("src/b.rs")).unwrap();
    fs::write(
        root.path().join("src/a.rs"),
        format!(
            "{source}\n{}\n{}",
            source.replace("alpha", "beta"),
            source.replace("alpha", "gamma")
        ),
    )
    .unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let row = server.json("api/findings?type=duplicate")["items"][0].clone();
    assert_eq!(row["memberCount"], 3);
    assert_eq!(row["fileCount"], 1);
    assert_eq!(row["scope"], "same-file");
    assert_eq!(row["title"], "alpha ≈ beta ≈ gamma");
    assert_eq!(row["membersOmitted"], 0);
}

#[test]
fn bounded_preview_names_every_shown_member_and_detail_keeps_the_whole_group() {
    let root = project();
    let source = fs::read_to_string(root.path().join("src/a.rs")).unwrap();
    for id in 0..8 {
        fs::write(
            root.path().join(format!("src/member_{id}.rs")),
            source.replace("alpha", &format!("member_{id}")),
        )
        .unwrap();
    }
    index(root.path());
    let server = Server::start(root.path(), true);
    let row = server.json("api/findings?type=duplicate&size=1")["items"][0].clone();
    assert_eq!(row["memberCount"], 10);
    assert_eq!(row["fileCount"], 10);
    assert_eq!(row["scope"], "cross-file");
    assert_eq!(row["membersOmitted"], 4);
    assert_eq!(row["members"].as_array().unwrap().len(), 6);
    let title = row["title"].as_str().unwrap();
    for member in row["members"].as_array().unwrap() {
        assert!(title.contains(member["name"].as_str().unwrap()), "{title}");
    }
    assert!(title.ends_with(" (+4)"), "{title}");
    let key = row["key"].as_str().unwrap();
    let detail = server.json(&format!("api/finding?key={key}"));
    assert_eq!(detail["members"].as_array().unwrap().len(), 10);
    assert_eq!(detail["membersOmitted"], 0);
    assert_eq!(detail["memberCount"], row["memberCount"]);
    assert_eq!(detail["fileCount"], row["fileCount"]);
    assert!(server
        .decide(key, Some("dismissed"), "keep separate")
        .starts_with("HTTP/1.1 200"));
    let summary = server.json("api/summary");
    let event = summary["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["finding"]["key"] == key)
        .unwrap();
    assert_eq!(event["finding"]["title"], row["title"]);
    index(root.path());
    let reloaded = Server::start(root.path(), true);
    let current = reloaded.json("api/findings?type=duplicate&size=1")["items"][0].clone();
    assert_eq!(current["key"], row["key"]);
    assert_eq!(current["number"], row["number"]);
    assert_eq!(current["state"], "dismissed");
}

#[test]
#[ignore = "manual duplicate row timing, not a CI latency assertion"]
fn measure_large_duplicate_rows() {
    let root = project();
    let server = Server::start(root.path(), true);
    {
        let mut site = server.site.borrow_mut();
        let template = site.duplicates.groups[0].clone();
        let key = site.review_keys[0].clone();
        site.duplicates.groups.clear();
        site.review_keys.clear();
        for group_id in 0..100 {
            let mut group = template.clone();
            group.members = (0..1000)
                .map(|id| {
                    let mut member = template.members[0].clone();
                    member.path = format!("module_{:04}/file_{:04}.rs", id / 100, id / 2);
                    member.symbol_name = format!("member_{group_id}_{id}");
                    member
                })
                .collect();
            site.duplicates.groups.push(group);
            site.review_keys.push(key.clone());
        }
    }
    let mut samples = Vec::new();
    for _ in 0..11 {
        let start = std::time::Instant::now();
        let result = server.json("api/findings?type=duplicate&size=100");
        samples.push(start.elapsed().as_micros());
        assert_eq!(result["total"], 100);
        assert_eq!(result["items"][0]["memberCount"], 1000);
        assert_eq!(result["items"][0]["fileCount"], 500);
    }
    samples.sort();
    println!(
        "duplicate rows: 100 groups x 1000 members; median={}us min={}us max={}us",
        samples[5], samples[0], samples[10]
    );
}

#[test]
fn root_files_share_a_directory_and_top_level_directories_define_scope() {
    let root = project();
    let source = fs::read_to_string(root.path().join("src/a.rs")).unwrap();
    fs::remove_dir_all(root.path().join("src")).unwrap();
    for name in ["alpha", "beta"] {
        fs::write(
            root.path().join(format!("{name}.rs")),
            source.replace("alpha", name),
        )
        .unwrap();
    }
    index(root.path());
    assert_eq!(
        Server::start(root.path(), true).json("api/findings?type=duplicate")["items"][0]["scope"],
        "cross-file"
    );
    fs::create_dir(root.path().join("other")).unwrap();
    fs::rename(
        root.path().join("beta.rs"),
        root.path().join("other/beta.rs"),
    )
    .unwrap();
    index(root.path());
    assert_eq!(
        Server::start(root.path(), true).json("api/findings?type=duplicate")["items"][0]["scope"],
        "cross-module"
    );
}
