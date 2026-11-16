//! Parity of the two ways to run the repeated partial-block search: the real
//! `mct-cli duplicates --partial-blocks` binary and the real `mct-cli web`
//! server, on one index with equivalent options (the defaults).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "common/command.rs"]
mod command;
use command::mct;

/// One block as both reports state it: language, tokens, lines, and each
/// occurrence's path, first line, last line and function.
type Block = (String, u64, u64, Vec<(String, u64, u64, String)>);

struct Project(PathBuf);

impl Project {
    fn indexed(tag: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("mct-cli-parity-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, source) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        let output = mct(&[&"--root", &dir, &"init"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self(dir)
    }

    fn cli(&self, extra: &[&str]) -> String {
        let mut args: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![
            &"--root",
            &self.0,
            &"duplicates",
            &"--no-cache",
            &"--partial-blocks",
        ];
        args.extend(extra.iter().map(|arg| arg as &dyn AsRef<std::ffi::OsStr>));
        let output = mct(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A running `mct-cli web`, stopped when dropped.
struct Dashboard {
    child: Child,
    port: u16,
    /// Kept open: the server would stop at its next line on a closed pipe.
    _stdout: BufReader<std::process::ChildStdout>,
}

impl Dashboard {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mct-cli"))
            .arg("--root")
            .arg(root)
            .args(["web", "--port", "0", "--no-obsidian"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let port = stdout
            .by_ref()
            .lines()
            .find_map(|line| {
                let line = line.ok()?;
                let address = line.strip_prefix("Local project web: http://localhost:")?;
                address.trim_end_matches('/').parse().ok()
            })
            .expect("the dashboard printed its address");
        Self {
            child,
            port,
            _stdout: stdout,
        }
    }

    fn request(&self, head: &str, body: &str) -> serde_json::Value {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        write!(stream, "{head}\r\nHost: localhost:{}\r\n", self.port).unwrap();
        if body.is_empty() {
            stream.write_all(b"\r\n").unwrap();
        } else {
            write!(
                stream,
                "Origin: http://localhost:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                self.port,
                body.len()
            )
            .unwrap();
        }
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{head}: {response}");
        let (_, json) = response.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(json).unwrap()
    }

    fn get(&self, path: &str) -> serde_json::Value {
        self.request(&format!("GET /{path} HTTP/1.1"), "")
    }

    /// Starts the search as the page does and polls, as the page does, until
    /// it is done.
    fn partial_blocks(&self) -> serde_json::Value {
        let csrf = self.get("api/meta")["csrf"].as_str().unwrap().to_string();
        let body = serde_json::json!({ "csrf": csrf }).to_string();
        self.request("POST /api/partial-blocks HTTP/1.1", &body);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let state = self.get("api/partial-blocks?size=200");
            if state["status"] != "running" {
                assert_eq!(state["status"], "done", "{state}");
                return state;
            }
            assert!(Instant::now() < deadline, "the search did not finish");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn dashboard_blocks(state: &serde_json::Value) -> Vec<Block> {
    state["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| {
            let occurrences = block["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .map(|occurrence| {
                    (
                        occurrence["path"].as_str().unwrap().to_string(),
                        occurrence["start"].as_u64().unwrap(),
                        occurrence["end"].as_u64().unwrap(),
                        occurrence["function"].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            (
                block["language"].as_str().unwrap().to_string(),
                block["tokens"].as_u64().unwrap(),
                block["lines"].as_u64().unwrap(),
                occurrences,
            )
        })
        .collect()
}

/// The `partial_blocks[N]` list of a TOON report. The fixture paths and
/// function names need no TOON quoting.
fn cli_blocks(report: &str) -> Vec<Block> {
    let start = report
        .find("\npartial_blocks[")
        .expect("a partial_blocks list")
        + 1;
    let mut blocks: Vec<Block> = Vec::new();
    for line in report[start..].lines().skip(1) {
        let Some(line) = line.strip_prefix("  ") else {
            break;
        };
        let line = line.trim_start_matches("- ").trim();
        if let Some(language) = line.strip_prefix("language: ") {
            blocks.push((language.to_string(), 0, 0, Vec::new()));
        } else if let Some(tokens) = line.strip_prefix("tokens: ") {
            blocks.last_mut().unwrap().1 = tokens.parse().unwrap();
        } else if let Some(lines) = line.strip_prefix("lines: ") {
            blocks.last_mut().unwrap().2 = lines.parse().unwrap();
        } else if !line.starts_with("id: ") && !line.starts_with("occurrences[") {
            let fields: Vec<&str> = line.split(',').collect();
            assert_eq!(fields.len(), 4, "{line}");
            blocks.last_mut().unwrap().3.push((
                fields[0].to_string(),
                fields[1].parse().unwrap(),
                fields[2].parse().unwrap(),
                fields[3].to_string(),
            ));
        }
    }
    blocks
}

fn header<'a>(report: &'a str, name: &str) -> &'a str {
    report
        .lines()
        .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix(": "))
        .unwrap_or_else(|| panic!("no `{name}` in the report"))
}

const BLOCKS_RS: &str = include_str!("fixtures/duplicates/blocks.rs");
const BLOCKS_PY: &str = include_str!("fixtures/duplicates/blocks.py");

#[test]
fn the_dashboard_and_the_cli_report_the_same_blocks_at_the_same_locations() {
    let project = Project::indexed(
        "blocks",
        &[("src/blocks.rs", BLOCKS_RS), ("blocks.py", BLOCKS_PY)],
    );
    let dashboard = Dashboard::start(&project.0);
    let web = dashboard.partial_blocks();
    let cli = project.cli(&[]);

    let blocks = cli_blocks(&cli);
    assert_eq!(blocks.len(), 2, "{cli}");
    assert_eq!(dashboard_blocks(&web), blocks);
    assert_eq!(
        web["total"].to_string(),
        header(&cli, "partial_block_count")
    );
    // The same diagnostics, in the same words.
    assert_eq!(header(&cli, "budget_exhausted"), "false");
    assert_eq!(web["budget"]["exhausted"], serde_json::Value::Null);
    assert_eq!(web["indexCoverage"], header(&cli, "index_coverage"));
    for (field, name) in [
        ("functionsSearched", "functions_searched"),
        ("windowsUsed", "windows_used"),
        ("comparisonsUsed", "comparisons_used"),
        ("minTokens", "min_tokens"),
        ("maxBlocks", "max_blocks"),
    ] {
        // `min_tokens` appears first under `options`, with the same default.
        let cli_value = cli[cli.find("partial_blocks_budget:").unwrap()..]
            .lines()
            .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix(": "))
            .unwrap();
        assert_eq!(web["budget"][field].to_string(), cli_value, "{name}");
    }
    // The group report is the one both always gave.
    let groups = dashboard.get("api/findings?type=duplicate");
    assert_eq!(groups["total"].to_string(), header(&cli, "total_groups"));

    // A language filter of the CLI selects the same blocks the dashboard
    // lists for that language (no budget is exhausted here).
    let rust_only: Vec<Block> = dashboard_blocks(&web)
        .into_iter()
        .filter(|block| block.0 == "rust")
        .collect();
    assert_eq!(cli_blocks(&project.cli(&["--language", "rust"])), rust_only);
}

#[test]
fn both_report_zero_blocks_for_whole_function_copies_only() {
    let copies = BLOCKS_RS[BLOCKS_RS.find("fn copy_left").unwrap()..].to_string();
    let project = Project::indexed("none", &[("src/copies.rs", &copies)]);
    let dashboard = Dashboard::start(&project.0);
    let web = dashboard.partial_blocks();
    let cli = project.cli(&[]);
    assert_eq!(cli_blocks(&cli), []);
    assert_eq!(dashboard_blocks(&web), []);
    assert_eq!(web["total"], 0);
    assert_eq!(header(&cli, "partial_block_count"), "0");
    assert_eq!(header(&cli, "total_groups"), "1");
    assert_eq!(dashboard.get("api/findings?type=duplicate")["total"], 1);
}
