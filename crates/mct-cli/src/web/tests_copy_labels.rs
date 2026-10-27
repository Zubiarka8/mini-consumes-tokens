//! Copy-label regressions through real HTTP handlers, dashboard SQLite and reindexing.
use super::*;

fn copies() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/a.rs"),
        format!("pub fn alpha{BODY}\npub fn beta{BODY}"),
    )
    .unwrap();
    index(root.path());
    root
}

fn item(server: &Server) -> serde_json::Value {
    api_json(&server.get("api/duplicates"))["items"][0].clone()
}

fn detail(server: &Server, id: &str) -> serde_json::Value {
    api_json(&server.get(&format!("api/finding?kind=duplicate&id={id}")))
}

fn label(server: &Server, item: &serde_json::Value, at: usize, status: &str, note: &str) -> String {
    let member = &item["members"][at];
    let mut body = serde_json::json!({"id": item["identity"], "path": member["path"], "status": status, "note": note});
    // Exercise the old path contract on a pre-fix server as well, demonstrating
    // the actual overwrite rather than merely a missing new API field.
    if let Some(key) = member["member_key"].as_str() {
        body["member_key"] = key.into();
        let current = detail(server, item["identity"].as_str().unwrap());
        body["expected_revision"] = current["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["member_key"] == key)
            .unwrap()["revision"]
            .clone();
    }
    server.write("api/file-review", &body)
}

#[test]
fn copy_labels_two_copies_in_one_file_keep_independent_states_and_notes() {
    let root = copies();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    assert_eq!(group["members"].as_array().unwrap().len(), 2);
    assert_eq!(group["members"][0]["path"], group["members"][1]["path"]);
    for (at, status, note) in [
        (0, "accepted", "keep alpha"),
        (1, "rejected", "remove beta"),
    ] {
        let response = label(&server, &group, at, status, note);
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
    let reloaded = Server::start(root.path(), true);
    let state = detail(&reloaded, group["identity"].as_str().unwrap());
    let rows = state["files"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        2,
        "two copies must not overwrite the same path row: {state}"
    );
    for (at, status, note) in [
        (0, "accepted", "keep alpha"),
        (1, "rejected", "remove beta"),
    ] {
        let row = rows
            .iter()
            .find(|row| row["member_key"] == group["members"][at]["member_key"])
            .unwrap();
        assert_eq!(row["status"], status);
        assert_eq!(row["note"], note);
        assert_eq!(row["code_changed"], false);
    }
    assert_eq!(state["state"]["status"], "pending");
}

#[test]
fn copy_labels_warn_after_code_edit_and_reindex_without_changing_the_label() {
    let root = project();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let id = group["identity"].as_str().unwrap();
    let response = label(&server, &group, 0, "rejected", "reviewed old code");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let before_state = detail(&server, id)["state"].clone();
    let path = group["members"][0]["path"].as_str().unwrap();
    let original = fs::read_to_string(root.path().join(path)).unwrap();
    fs::write(
        root.path().join(path),
        original.replace("total += 1", "total += 9"),
    )
    .unwrap();
    index(root.path());
    let reloaded = Server::start(root.path(), true);
    assert_eq!(item(&reloaded)["identity"], id);
    let state = detail(&reloaded, id);
    assert_eq!(state["files"][0]["status"], "rejected");
    assert_eq!(state["files"][0]["note"], "reviewed old code");
    assert_eq!(
        state["files"][0]["code_changed"], true,
        "the saved label must warn after its code changes: {state}"
    );
    assert_eq!(
        state["state"], before_state,
        "the informational warning must not change a pending finding"
    );
    fs::write(root.path().join(path), original).unwrap();
    index(root.path());
    let reverted = Server::start(root.path(), true);
    assert_eq!(detail(&reverted, id)["files"][0]["code_changed"], false);
}

#[test]
fn copy_labels_api_rejects_forged_members_hashes_and_stale_copy_revisions() {
    let root = copies();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let mut body = serde_json::json!({"id":group["identity"], "path":group["members"][0]["path"],
        "status":"accepted", "expected_revision":0});
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 409"));
    body["member_key"] = "f".repeat(64).into();
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 404"));
    body["member_key"] = group["members"][0]["member_key"].clone();
    body["code_hash"] = "a".repeat(64).into();
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 400"));
    body.as_object_mut().unwrap().remove("code_hash");
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 200"));
    // Saving A must not consume B's independent initial revision.
    body["member_key"] = group["members"][1]["member_key"].clone();
    body["status"] = "rejected".into();
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 200"));
    body["member_key"] = group["members"][0]["member_key"].clone();
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 409"));
    let path = group["members"][0]["path"].as_str().unwrap();
    let original = fs::read_to_string(root.path().join(path)).unwrap();
    fs::write(
        root.path().join(path),
        format!("{original}\n// not indexed\n"),
    )
    .unwrap();
    body["expected_revision"] = 1.into();
    assert!(server
        .write("api/file-review", &body)
        .starts_with("HTTP/1.1 409"));
    fs::write(root.path().join(path), original).unwrap();
    assert_eq!(
        detail(&server, group["identity"].as_str().unwrap())["files"][0]["code_changed"],
        false
    );
}

#[test]
fn copy_labels_same_named_methods_in_distinct_parents_keep_labels_after_moves() {
    let root = copies();
    let first = format!("struct One;\nimpl One {{ pub fn new{BODY} }}\n");
    let second = format!("struct Two;\nimpl Two {{ pub fn new{BODY} }}\n");
    fs::write(root.path().join("src/a.rs"), format!("{first}{second}")).unwrap();
    index(root.path());
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let members = group["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["symbol"], members[1]["symbol"]);
    assert_ne!(members[0]["member_key"], members[1]["member_key"]);
    assert!(label(&server, &group, 0, "accepted", "keep One").starts_with("HTTP/1.1 200"));
    assert!(label(&server, &group, 1, "rejected", "remove Two").starts_with("HTTP/1.1 200"));
    fs::write(root.path().join("src/a.rs"), format!("{second}\n{first}")).unwrap();
    index(root.path());
    let moved = Server::start(root.path(), true);
    assert_eq!(item(&moved)["identity"], group["identity"]);
    let state = detail(&moved, group["identity"].as_str().unwrap());
    for (at, note) in [(0, "keep One"), (1, "remove Two")] {
        let row = state["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["member_key"] == members[at]["member_key"])
            .unwrap();
        assert_eq!(row["note"], note);
        assert_eq!(row["code_changed"], false);
        assert_eq!(row["code_unavailable"], false);
    }
}

#[test]
fn copy_labels_survive_unchanged_reindex_and_unavailable_source_is_explicit() {
    let root = project();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let id = group["identity"].as_str().unwrap();
    assert!(
        label(&server, &group, 0, "rejected", "retain independently").starts_with("HTTP/1.1 200")
    );
    let saved = detail(&server, id);
    index(root.path());
    let reopened = Server::start(root.path(), true);
    assert_eq!(detail(&reopened, id)["files"], saved["files"]);
    assert_eq!(
        item(&reopened)["identity"],
        id,
        "rejecting a copy does not hide its group"
    );
    let path = root
        .path()
        .join(group["members"][0]["path"].as_str().unwrap());
    let original = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let unavailable = detail(&reopened, id);
    assert_eq!(unavailable["files"][0]["code_unavailable"], true);
    assert_eq!(unavailable["files"][0]["note"], "retain independently");
    assert!(label(&reopened, &group, 0, "accepted", "must not replace").starts_with("HTTP/1.1 409"));
    fs::write(path, original).unwrap();
    assert_eq!(detail(&reopened, id)["files"], saved["files"]);
}

#[test]
fn copy_labels_are_retained_without_transfer_after_rename_membership_change_and_removal() {
    for change in [
        "rename_file",
        "rename_symbol",
        "add_member",
        "remove_member",
        "remove_group",
    ] {
        let root = project();
        if change == "remove_member" {
            fs::write(root.path().join("src/c.rs"), format!("pub fn gamma{BODY}")).unwrap();
            index(root.path());
        }
        let server = Server::start(root.path(), true);
        let group = item(&server);
        let id = group["identity"].as_str().unwrap();
        assert!(label(&server, &group, 0, "rejected", "old copy note").starts_with("HTTP/1.1 200"));
        let old_rows = detail(&server, id)["retained_files"].clone();
        let a = root.path().join("src/a.rs");
        match change {
            "rename_file" => fs::rename(&a, root.path().join("src/renamed.rs")).unwrap(),
            "rename_symbol" => fs::write(&a, format!("pub fn renamed{BODY}")).unwrap(),
            "add_member" => {
                fs::write(root.path().join("src/c.rs"), format!("pub fn gamma{BODY}")).unwrap()
            }
            "remove_member" => fs::remove_file(&a).unwrap(),
            "remove_group" => {
                fs::remove_file(&a).unwrap();
                fs::remove_file(root.path().join("src/b.rs")).unwrap();
            }
            _ => unreachable!(),
        }
        index(root.path());
        let reopened = Server::start(root.path(), true);
        let retired = detail(&reopened, id);
        assert_eq!(retired["current"], false, "{change}: {retired}");
        assert_eq!(old_rows.as_array().unwrap().len(), 1);
        assert_eq!(retired["retained_files"], old_rows, "{change}");
        let list = api_json(&reopened.get("api/duplicates"));
        for current in list["items"].as_array().unwrap() {
            let current = detail(&reopened, current["identity"].as_str().unwrap());
            assert!(
                current["files"].as_array().unwrap().is_empty(),
                "{change}: unsafe transfer {current}"
            );
            assert_eq!(current["state"]["status"], "pending");
        }
    }
}

#[test]
fn embedded_http_shell_delivers_bulk_controls_and_scope_help() {
    let root = project();
    let server = Server::start(root.path(), true);
    let response = server.get("app?view=editor&id=example");
    assert!(response.starts_with("HTTP/1.1 200"));
    let header = response
        .split("<header")
        .nth(1)
        .unwrap()
        .split("</header>")
        .next()
        .unwrap();
    assert!(header.contains("@input=\"headerSearch\""));
    for action in ["openAll", "closeAll"] {
        assert!(header.contains(&format!("type=\"button\" @click=\"{action}\"")));
    }
    assert!(response.contains("x-text=\"L.fileHint\""));
    assert!(response.contains("x-text=\"L.draftHint\""));
    assert!(response.contains("x-text=\"L.reindexHint\""));
    let script = server.get("spa.js");
    assert!(script.starts_with("HTTP/1.1 200"));
    assert!(script.contains("Abrir todos") && script.contains("Cerrar todos"));
}

/// Phase 8.1: persisted dashboard copy labels and group decisions agree with
/// the real MCP dispatcher and CLI report, without exposing or rewriting notes.
#[test]
fn duplicate_mcp_matches_dashboard_and_cli_without_conflating_copy_decisions() {
    use clap::Parser as _;
    let root = copies();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let id = group["identity"].as_str().unwrap();
    for (at, status, note) in [
        (0, "rejected", "private alpha"),
        (1, "accepted", "private beta"),
    ] {
        assert!(label(&server, &group, at, status, note).starts_with("HTTP/1.1 200"));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mcp = || {
        let index = mct_index::Index::open_existing_read_only(
            root.path(),
            &root.path().join(".mct-index/index.sqlite3"),
            mct_index::ExcludeSet::default(),
        )
        .unwrap();
        let server = mct_mcp_server::server::MctServer::new(index, mct_languages::build_registry());
        let result = runtime
            .block_on(server.call_read_only_tool("find_duplicate_code", None))
            .unwrap();
        result.content[0].as_text().unwrap().text.clone()
    };
    #[derive(clap::Parser)]
    struct DuplicatesCli {
        #[command(flatten)]
        args: crate::duplicates::Args,
    }
    let output = root.path().join("report.toon");
    let cli = || {
        let argv = vec!["duplicates", "--output", output.to_str().unwrap()];
        crate::duplicates::run(
            root.path(),
            DuplicatesCli::try_parse_from(argv).unwrap().args,
        )
        .unwrap();
        fs::read_to_string(&output).unwrap()
    };
    let before = detail(&server, id);
    assert!(mcp().contains("actionable: 1"));
    assert!(cli().contains("total_groups: 1"));
    assert_eq!(
        detail(&server, id),
        before,
        "readers must preserve revisions, comments and copy notes"
    );
    for decision in ["false_positive", "ignored", "confirmed"] {
        let response=server.write("api/review",&serde_json::json!({"kind":"duplicate","id":id,"action":decision,"reason":"private reason"}));
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let before = detail(&server, id);
        let result = mcp();
        let suppressed = decision != "confirmed";
        assert!(
            result.contains(if suppressed {
                "status: all_suppressed"
            } else {
                "status: groups"
            }),
            "{result}"
        );
        assert!(cli().contains(if suppressed {
            "total_groups: 0"
        } else {
            "total_groups: 1"
        }));
        assert!(!result.contains("private"));
        assert_eq!(detail(&server, id), before);
        index(root.path());
        assert!(mcp().contains(if suppressed {
            "status: all_suppressed"
        } else {
            "status: groups"
        }));
        // Refresh only this isolated server after reindex; normal user writes
        // still need the fresh served snapshot and optimistic revision.
        let csrf = server.site.borrow().csrf.clone();
        let response = server.form(
            "reload",
            &format!("csrf={csrf}"),
            "Origin: http://127.0.0.1:8787\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 303"), "{response}");
    }
}

#[test]
fn duplicate_mcp_retrieval_and_verified_backup_workloads_preserve_user_data() {
    let root = copies();
    let server = Server::start(root.path(), true);
    let group = item(&server);
    let id = group["identity"].as_str().unwrap();
    assert!(label(&server, &group, 0, "rejected", "private note").starts_with("HTTP/1.1 200"));
    let before = detail(&server, id);
    let destination = tempfile::tempdir().unwrap();
    let source = root.path().canonicalize().unwrap();
    let backup_dir = destination.path().canonicalize().unwrap().join("snapshots");
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_gate = std::sync::Arc::clone(&gate);
    let worker = std::thread::spawn(move || {
        worker_gate.wait();
        for _ in 0..3 {
            let bundle = super::super::backup::create(&source, Some(&backup_dir)).unwrap();
            super::super::backup::verify_restore(&bundle).unwrap();
        }
    });
    let index = mct_index::Index::open_existing_read_only(
        root.path(),
        &root.path().join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let mcp = mct_mcp_server::server::MctServer::new(index, mct_languages::build_registry());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    gate.wait();
    for _ in 0..3 {
        let result = runtime
            .block_on(mcp.call_read_only_tool("find_duplicate_code", None))
            .unwrap();
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("actionable: 1"));
    }
    worker.join().unwrap();
    assert_eq!(detail(&server, id), before);
}
