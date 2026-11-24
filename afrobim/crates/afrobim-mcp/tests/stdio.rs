//! End to end: starts the built `afrobim-mcp` binary as a child process and talks to it over
//! stdio the way an MCP client does, one JSON-RPC message per line. Mirrors the C#
//! `McpServerStdioTests` and goes further: a layout, DXF exports, a schedule check, a document
//! built and exported, and the protocol's error paths.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// A fresh folder under Cargo's per-target temporary directory.
fn temp_dir(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("stdio-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    /// Starts the server in `cwd` with the given flags and environment. `AFROBIM_PACK`,
    /// `AFROBIM_FONTS`, `AFROBIM_ROOT` and `AFROBIM_READ_ONLY` are cleared first so the developer's own environment cannot leak in.
    fn start(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Client {
        let mut command = Command::new(env!("CARGO_BIN_EXE_afrobim-mcp"));
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("AFROBIM_PACK")
            .env_remove("AFROBIM_FONTS")
            .env_remove("AFROBIM_ROOT")
            .env_remove("AFROBIM_READ_ONLY");
        command.envs(env.iter().copied());
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Client {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    fn write_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn read_message(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.stdout.read_line(&mut line).unwrap() > 0,
            "the server closed its output"
        );
        assert!(line.ends_with('\n'), "every message ends with a newline");
        assert!(!line.trim_end().contains('\n'), "one message per line");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout carried something that is not JSON ({e}): {line}"))
    }

    /// Sends a request and returns its response, checking the JSON-RPC envelope.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if !params.is_null() {
            message["params"] = params;
        }
        self.write_line(&message.to_string());
        let response = self.read_message();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], id, "responses come back in request order");
        assert!(
            response.get("result").is_some() != response.get("error").is_some(),
            "exactly one of result and error: {response}"
        );
        response
    }

    fn notify(&mut self, method: &str) {
        self.write_line(&json!({ "jsonrpc": "2.0", "method": method }).to_string());
    }

    fn initialize(&mut self) -> Value {
        let params = json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "stdio-test", "version": "1" } });
        let response = self.request("initialize", params);
        self.notify("notifications/initialized");
        response["result"].clone()
    }

    fn call_raw(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    /// Calls a tool that must succeed and returns its JSON payload.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.call_raw(name, arguments);
        let result = &response["result"];
        assert!(result.get("isError").is_none(), "{name} failed: {result}");
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap()
    }

    /// Calls a tool that must fail and returns the error text.
    fn call_failing(&mut self, name: &str, arguments: Value) -> String {
        let response = self.call_raw(name, arguments);
        let result = &response["result"];
        assert_eq!(
            result["isError"], true,
            "{name} should have failed: {result}"
        );
        result["content"][0]["text"].as_str().unwrap().to_string()
    }

    /// Closes the server's input and returns its exit code and everything it wrote to stderr.
    fn finish(mut self) -> (Option<i32>, String) {
        drop(self.stdin.take());
        let output = self.child.wait_with_output().unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

#[test]
fn a_whole_session_over_stdio() {
    let tmp = temp_dir("session");
    // Started from a folder outside the checkout: the pack and fonts are found from the
    // executable, as they are when Claude Desktop launches the server.
    let mut c = Client::start(&tmp, &[], &[]);

    // ─── handshake ────────────────────────────────────────────────────────
    let init = c.initialize();
    assert_eq!(init["protocolVersion"], "2025-06-18");
    assert_eq!(init["serverInfo"]["name"], "afrobim");
    assert_eq!(init["capabilities"]["tools"]["listChanged"], false);
    assert!(init["instructions"]
        .as_str()
        .unwrap()
        .contains("search_catalog"));
    assert_eq!(c.request("ping", Value::Null)["result"], json!({}));

    // ─── tools/list ───────────────────────────────────────────────────────
    let list = c.request("tools/list", Value::Null);
    let tools = list["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "document_add_sign",
            "document_export_dxf",
            "document_list",
            "document_new",
            "document_remove_element",
            "document_save",
            "export_dxf",
            "layout_sign",
            "list_packs",
            "search_catalog",
            "validate_schedule"
        ]
    );
    for t in tools {
        assert_eq!(
            t["inputSchema"]["additionalProperties"], false,
            "{}",
            t["name"]
        );
        assert!(!t["description"].as_str().unwrap().is_empty());
        assert!(t["annotations"]["readOnlyHint"].is_boolean());
    }

    // ─── catalog ──────────────────────────────────────────────────────────
    let packs = c.call("list_packs", json!({}));
    assert_eq!(packs["default_pack"], "UK-TSM-2019");
    assert!(packs["packs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "UK-TSM-2019" && p["is_default"] == true && p["fonts_found"] == true));

    let found = c.call("search_catalog", json!({ "query": "stack" }));
    assert_eq!(found["pack"], "UK-TSM-2019");
    let keys: Vec<&str> = found["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"UK04.1"));
    assert_eq!(found["count"], keys.len());

    let refused = c.call_failing("search_catalog", json!({ "qery": "stack" }));
    assert_eq!(
        refused,
        "unknown argument(s) for search_catalog: qery; accepted: pack, query, series"
    );

    // ─── layout ───────────────────────────────────────────────────────────
    let laid = c.call("layout_sign", json!({ "key": "UK04.1" }));
    assert_eq!(laid["code"], "UK04.1");
    assert_eq!(laid["kind"], "stack");
    assert_eq!(
        (laid["width_mm"].as_f64(), laid["height_mm"].as_f64()),
        (Some(2111.9), Some(1125.0))
    );
    assert_eq!(
        (laid["sw_mm"].as_f64(), laid["x_height_mm"].as_f64()),
        (Some(31.25), Some(125.0))
    );
    assert_eq!(
        laid["primitive_counts"],
        json!({ "PolyFill": 5, "TextOutline": 2 })
    );
    assert_eq!(laid["has_errors"], false);
    let svg = laid["svg"].as_str().unwrap();
    assert!(
        svg.starts_with("<svg") && svg.ends_with("</svg>") && svg.contains("fill-rule=\"evenodd\"")
    );

    let seven: Vec<String> = (1..=7).map(|i| format!("Town{i}:Ahead:{i}")).collect();
    let crowded = c.call(
        "layout_sign",
        json!({ "schedule_row": format!("ADS-7,stack,64,primary,green,{}", seven.join(";")) }),
    );
    assert_eq!(crowded["has_errors"], true);
    let diagnostic = &crowded["diagnostics"][0];
    assert_eq!(
        (diagnostic["severity"].as_str(), diagnostic["code"].as_str()),
        (Some("Error"), Some("MAX_DESTINATIONS"))
    );
    assert_eq!(diagnostic["clause"], "UK TSM Chapter 7 Appendix D");

    let unknown_key = c.call_failing("layout_sign", json!({ "key": "NOPE" }));
    assert_eq!(unknown_key, "An error occurred invoking 'layout_sign': catalog sign page 'NOPE' not found in rule pack 'UK-TSM-2019'");

    // ─── DXF export to a temporary folder ─────────────────────────────────
    let out_dir = tmp.join("dxf");
    let exported = c.call(
        "export_dxf",
        json!({ "out_dir": out_dir, "signs": ["UK04.1", "UK06.2", "NOPE"] }),
    );
    let files = exported["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(exported["failures"][0]["source"], "NOPE");
    assert_eq!(exported["error_count"], 1);
    for f in files {
        let path = Path::new(f["path"].as_str().unwrap());
        assert_eq!(path.parent(), Some(out_dir.as_path()));
        let dxf = std::fs::read_to_string(path).unwrap();
        assert!(dxf.contains("AC1009") && dxf.ends_with("0\nEOF\n"));
    }
    assert_eq!(std::fs::read_dir(&out_dir).unwrap().count(), 2);

    // ─── schedule check (a path relative to the project root, here the working directory) ──
    std::fs::copy(
        repo_root().join("examples/uk-directional.csv"),
        tmp.join("uk-directional.csv"),
    )
    .unwrap();
    let checked = c.call(
        "validate_schedule",
        json!({ "csv_path": "uk-directional.csv" }),
    );
    assert_eq!(
        (
            checked["valid"].as_bool(),
            checked["rows"].as_u64(),
            checked["error_count"].as_u64()
        ),
        (Some(true), Some(20), Some(0))
    );
    assert!(Path::new(checked["csv_path"].as_str().unwrap()).is_absolute());

    // ─── a document the desktop app can open ──────────────────────────────
    let doc = tmp.join("model/junction.afrobim");
    assert_eq!(
        c.call("document_new", json!({ "path": doc }))["standard"],
        "UK-TSM-2019"
    );
    let first = c.call(
        "document_add_sign",
        json!({ "path": doc, "key": "UK04.1", "mark": "ADS-001" }),
    );
    assert_eq!(
        (
            first["id"].as_u64(),
            first["mark"].as_str(),
            first["width_mm"].as_f64()
        ),
        (Some(1), Some("ADS-001"), Some(2111.9))
    );
    let second = c.call(
        "document_add_sign",
        json!({ "path": doc, "schedule_row": "ADS-001,direction,64,primary,green,Lampton:Left:0:3", "properties": { "Drawing": "TS-101" } }),
    );
    assert_eq!(second["mark"], "ADS-002");
    assert!(second["x_mm"].as_f64().unwrap() > first["width_mm"].as_f64().unwrap());
    let listed = c.call("document_list", json!({ "path": doc }));
    assert_eq!(listed["count"], 2);
    assert_eq!(listed["elements"][1]["properties"]["Drawing"], "TS-101");
    assert_eq!(listed["elements"][1]["definition"]["kind"], "DirectionSign");
    let saved = c.call("document_save", json!({ "path": doc }));
    assert_eq!(
        (
            saved["signs_checked"].as_u64(),
            saved["error_count"].as_u64()
        ),
        (Some(2), Some(0))
    );
    let model_dxf = tmp.join("model/junction.dxf");
    let written = c.call(
        "document_export_dxf",
        json!({ "path": doc, "out_path": model_dxf }),
    );
    assert_eq!(written["written"].as_array().unwrap().len(), 2);
    assert!(std::fs::read_to_string(&model_dxf)
        .unwrap()
        .ends_with("0\nEOF\n"));
    assert_eq!(
        c.call(
            "document_remove_element",
            json!({ "path": doc, "mark": "ADS-002" })
        )["elements"],
        1
    );
    // The file on disk is what the kernel reads back.
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(&doc).unwrap()).unwrap();
    assert_eq!(
        (
            on_disk["format"].as_str(),
            on_disk["units"].as_str(),
            on_disk["standard"].as_str()
        ),
        (Some("afrobim"), Some("mm"), Some("UK-TSM-2019"))
    );
    assert_eq!(on_disk["elements"][0]["category"], "sign");
    assert_eq!(on_disk["elements"][0]["name"], "ADS-001");

    // ─── malformed input, unknown tool, unknown method ────────────────────
    c.write_line("this is not json");
    let malformed = c.read_message();
    assert_eq!(malformed["id"], Value::Null);
    assert_eq!(
        malformed["error"],
        json!({ "code": -32700, "message": "Failed to parse the JSON-RPC request." })
    );
    c.write_line(r#"{"id":99,"method":"ping"}"#);
    let no_version = c.read_message();
    assert_eq!(
        (
            no_version["id"].as_u64(),
            no_version["error"]["code"].as_i64()
        ),
        (Some(99), Some(-32700))
    );

    let unknown_tool = c.call_raw("nope", json!({}));
    assert_eq!(
        unknown_tool["error"],
        json!({ "code": -32602, "message": "Unknown tool: 'nope'" })
    );
    let unknown_method = c.request("resources/list", Value::Null);
    assert_eq!(
        unknown_method["error"],
        json!({ "code": -32601, "message": "Method 'resources/list' is not available." })
    );

    // The server is still answering, and stops cleanly when its input closes.
    assert_eq!(c.request("ping", Value::Null)["result"], json!({}));
    let (code, stderr) = c.finish();
    assert_eq!(code, Some(0));
    // Standard error holds only the start-up log: the project root and the read-only state.
    assert!(
        stderr
            .lines()
            .all(|l| l.starts_with("afrobim-mcp: project root:")
                || l == "afrobim-mcp: read-only mode: off"),
        "nothing but the start-up log is expected on a healthy run: {stderr}"
    );
}

#[test]
fn flags_and_environment_choose_the_pack_and_the_fonts() {
    let tmp = temp_dir("flags");
    let root = repo_root();
    let pack = root.join("packs/UK-TSM-2019.json");
    let fonts = root.join("fonts");
    let empty = tmp.join("no-fonts");
    std::fs::create_dir_all(&empty).unwrap();

    // Flags name both.
    let mut c = Client::start(
        &tmp,
        &[
            "--pack",
            pack.to_str().unwrap(),
            &format!("--fonts={}", fonts.display()),
        ],
        &[],
    );
    c.initialize();
    let packs = c.call("list_packs", json!({}));
    assert_eq!(packs["fonts_dir"], fonts.to_str().unwrap());
    assert_eq!(
        c.call("layout_sign", json!({ "key": "UK06.2" }))["width_mm"].as_f64(),
        Some(1518.5)
    );
    assert_eq!(c.finish().0, Some(0));

    // The environment names a fonts folder without the fonts: the catalog still works, and
    // laying out says what is missing.
    let mut c = Client::start(
        &tmp,
        &[],
        &[
            ("AFROBIM_FONTS", empty.to_str().unwrap()),
            ("AFROBIM_PACK", pack.to_str().unwrap()),
        ],
    );
    c.initialize();
    assert!(
        c.call("search_catalog", json!({}))["count"]
            .as_u64()
            .unwrap()
            > 10
    );
    assert_eq!(
        c.call("list_packs", json!({}))["packs"][0]["fonts_found"],
        false
    );
    let error = c.call_failing("layout_sign", json!({ "key": "UK04.1" }));
    assert!(
        error.contains("Transport fonts not found") && error.contains("AFROBIM_FONTS"),
        "{error}"
    );
    assert_eq!(c.finish().0, Some(0));

    // A flag overrides the environment.
    let mut c = Client::start(
        &tmp,
        &["--fonts", fonts.to_str().unwrap()],
        &[("AFROBIM_FONTS", empty.to_str().unwrap())],
    );
    c.initialize();
    assert_eq!(
        c.call("layout_sign", json!({ "key": "UK04.1" }))["has_errors"],
        false
    );
    assert_eq!(c.finish().0, Some(0));
}

#[test]
fn a_missing_pack_does_not_stop_the_server_and_the_tools_say_what_to_fix() {
    let tmp = temp_dir("missing");
    let mut c = Client::start(&tmp, &["--pack", "no-such-pack.json"], &[]);
    assert_eq!(c.initialize()["serverInfo"]["name"], "afrobim");
    assert_eq!(
        c.request("tools/list", Value::Null)["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        11
    );
    let error = c.call_failing("search_catalog", json!({}));
    assert!(
        error.starts_with("An error occurred invoking 'search_catalog': rule pack not found:")
            && error.contains("--pack"),
        "{error}"
    );
    let (code, stderr) = c.finish();
    assert_eq!(code, Some(0));
    assert!(stderr.contains("warning: rule pack not found"), "{stderr}");
}

#[test]
fn help_version_and_bad_flags_do_not_start_a_session() {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_afrobim-mcp"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };

    let version = run(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("afrobim-mcp {}", env!("CARGO_PKG_VERSION"))
    );

    let help = run(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--pack")
            && String::from_utf8_lossy(&help.stdout).contains("AFROBIM_FONTS")
    );

    let bad = run(&["--nope"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty(), "stdout is reserved for the protocol");
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown argument '--nope'"));
}

/// The server's own start-up lines on standard error.
fn log_of(stderr: &str) -> String {
    stderr.lines().collect::<Vec<_>>().join("\n")
}

/// Copies the example schedule to `to`.
fn copy_example_schedule(to: &Path) {
    std::fs::copy(repo_root().join("examples/uk-directional.csv"), to).unwrap();
}

#[test]
fn the_root_defaults_to_the_working_directory_and_the_log_says_so() {
    let tmp = temp_dir("root-default");
    let mut c = Client::start(&tmp, &[], &[]);
    c.initialize();
    let (code, stderr) = c.finish();
    assert_eq!(code, Some(0));
    let log = log_of(&stderr);
    assert!(
        log.contains("project root:")
            && log.contains(tmp.to_str().unwrap())
            && log.contains("the working directory is used"),
        "{log}"
    );
    assert!(log.contains("read-only mode: off"), "{log}");
}

#[test]
fn every_tool_path_is_confined_to_the_root_given_by_flag_or_environment() {
    let tmp = temp_dir("confined");
    let project = tmp.join("project");
    let outside = tmp.join("outside");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    copy_example_schedule(&outside.join("secret.csv"));
    copy_example_schedule(&project.join("ok.csv"));

    // The working directory is not the root: the flag decides, and relative paths follow it.
    let mut c = Client::start(&outside, &["--root", project.to_str().unwrap()], &[]);
    c.initialize();
    assert_eq!(
        c.call("validate_schedule", json!({ "csv_path": "ok.csv" }))["valid"],
        true
    );
    let root_name = project.to_str().unwrap();
    for (tool, args) in [
        (
            "validate_schedule",
            json!({ "csv_path": "../outside/secret.csv" }),
        ),
        (
            "validate_schedule",
            json!({ "csv_path": outside.join("secret.csv") }),
        ),
        ("document_new", json!({ "path": outside.join("m.afrobim") })),
        (
            "export_dxf",
            json!({ "out_dir": outside.join("dxf"), "signs": ["UK04.1"] }),
        ),
        ("document_new", json!({ "path": "../outside/m.afrobim" })),
    ] {
        let error = c.call_failing(tool, args);
        assert!(
            error.contains("outside the project root") && error.contains(root_name),
            "{tool}: {error}"
        );
    }
    assert!(!outside.join("m.afrobim").exists() && !outside.join("dxf").exists());
    let (code, stderr) = c.finish();
    assert_eq!(code, Some(0));
    assert!(log_of(&stderr).contains("(from --root)"), "{stderr}");

    // The environment variable does the same, and the flag wins over it.
    let mut c = Client::start(
        &outside,
        &[],
        &[("AFROBIM_ROOT", project.to_str().unwrap())],
    );
    c.initialize();
    assert!(c
        .call_failing("document_new", json!({ "path": "../outside/m.afrobim" }))
        .contains("outside the project root"));
    let (_, stderr) = c.finish();
    assert!(log_of(&stderr).contains("(from AFROBIM_ROOT)"), "{stderr}");

    let mut c = Client::start(
        &outside,
        &["--root", outside.to_str().unwrap()],
        &[("AFROBIM_ROOT", project.to_str().unwrap())],
    );
    c.initialize();
    assert_eq!(
        c.call("validate_schedule", json!({ "csv_path": "secret.csv" }))["valid"],
        true
    );
    c.finish();

    // A root that does not exist stops the server with a message.
    let bad = Command::new(env!("CARGO_BIN_EXE_afrobim-mcp"))
        .args(["--root", tmp.join("nowhere").to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("project root is not a folder"));
}

#[cfg(unix)]
#[test]
fn a_link_pointing_outside_the_root_is_refused_over_stdio() {
    let tmp = temp_dir("confined-link");
    let project = tmp.join("project");
    let outside = tmp.join("outside");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    copy_example_schedule(&outside.join("secret.csv"));
    std::os::unix::fs::symlink(outside.join("secret.csv"), project.join("link.csv")).unwrap();
    std::os::unix::fs::symlink(&outside, project.join("linked-dir")).unwrap();

    let mut c = Client::start(&project, &[], &[]);
    c.initialize();
    for (tool, args) in [
        ("validate_schedule", json!({ "csv_path": "link.csv" })),
        (
            "validate_schedule",
            json!({ "csv_path": "linked-dir/secret.csv" }),
        ),
        ("document_new", json!({ "path": "linked-dir/m.afrobim" })),
        (
            "export_dxf",
            json!({ "out_dir": "linked-dir/dxf", "signs": ["UK04.1"] }),
        ),
    ] {
        let error = c.call_failing(tool, args);
        assert!(
            error.contains("outside the project root"),
            "{tool}: {error}"
        );
    }
    assert!(!outside.join("m.afrobim").exists() && !outside.join("dxf").exists());
    c.finish();
}

#[test]
fn read_only_mode_refuses_every_write_tool_and_still_reads() {
    let tmp = temp_dir("read-only");
    copy_example_schedule(&tmp.join("s.csv"));

    // Make a document first, with a server that may write.
    let mut c = Client::start(&tmp, &[], &[]);
    c.initialize();
    c.call("document_new", json!({ "path": "m.afrobim" }));
    c.call(
        "document_add_sign",
        json!({ "path": "m.afrobim", "key": "UK04.1", "mark": "A" }),
    );
    c.finish();
    let before = std::fs::read(tmp.join("m.afrobim")).unwrap();

    // The flag, and then the environment variable, each switch the writes off.
    for (args, env) in [
        (vec!["--read-only"], vec![]),
        (vec![], vec![("AFROBIM_READ_ONLY", "1")]),
    ] {
        let mut c = Client::start(&tmp, &args, &env);
        c.initialize();
        for (tool, arguments) in [
            (
                "export_dxf",
                json!({ "out_dir": "dxf", "signs": ["UK04.1"] }),
            ),
            ("document_new", json!({ "path": "n.afrobim" })),
            (
                "document_new",
                json!({ "path": "m.afrobim", "overwrite": true }),
            ),
            (
                "document_add_sign",
                json!({ "path": "m.afrobim", "key": "UK04.1" }),
            ),
            (
                "document_remove_element",
                json!({ "path": "m.afrobim", "mark": "A" }),
            ),
            ("document_save", json!({ "path": "m.afrobim" })),
            (
                "document_save",
                json!({ "path": "m.afrobim", "save_as": "copy.afrobim" }),
            ),
            (
                "document_export_dxf",
                json!({ "path": "m.afrobim", "out_path": "m.dxf" }),
            ),
        ] {
            let error = c.call_failing(tool, arguments);
            assert!(error.contains("read-only mode"), "{tool}: {error}");
        }
        // Reads are untouched.
        let listed = c.call("document_list", json!({ "path": "m.afrobim" }));
        assert_eq!(listed["elements"].as_array().unwrap().len(), 1);
        assert_eq!(
            c.call("validate_schedule", json!({ "csv_path": "s.csv" }))["valid"],
            true
        );
        assert!(
            c.call("search_catalog", json!({}))["count"]
                .as_u64()
                .unwrap()
                > 10
        );
        let (code, stderr) = c.finish();
        assert_eq!(code, Some(0));
        assert!(log_of(&stderr).contains("read-only mode: on"), "{stderr}");
    }
    assert_eq!(std::fs::read(tmp.join("m.afrobim")).unwrap(), before);
    for name in ["dxf", "n.afrobim", "copy.afrobim", "m.dxf"] {
        assert!(!tmp.join(name).exists(), "{name} must not exist");
    }

    // A value that is not a yes or a no stops the server rather than leaving it writable.
    let bad = Command::new(env!("CARGO_BIN_EXE_afrobim-mcp"))
        .env("AFROBIM_READ_ONLY", "ture")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("AFROBIM_READ_ONLY must be"));
}

#[test]
fn a_normal_path_inside_the_root_works_for_every_kind_of_tool() {
    let tmp = temp_dir("inside");
    let mut c = Client::start(&tmp, &["--root", tmp.to_str().unwrap()], &[]);
    c.initialize();
    c.call("document_new", json!({ "path": "sub/m.afrobim" }));
    c.call(
        "document_add_sign",
        json!({ "path": "sub/m.afrobim", "key": "UK04.1", "mark": "A" }),
    );
    let exported = c.call(
        "export_dxf",
        json!({ "out_dir": "dxf", "signs": ["UK04.1"] }),
    );
    assert_eq!(exported["files"].as_array().unwrap().len(), 1);
    c.call(
        "document_export_dxf",
        json!({ "path": "sub/m.afrobim", "out_path": "out/m.dxf" }),
    );
    assert!(tmp.join("out/m.dxf").is_file());
    c.finish();
}
