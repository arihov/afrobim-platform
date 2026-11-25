// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The MCP server loop: JSON-RPC 2.0 over stdio, one message per line.
//!
//! Framed as the C# SignCore server (the official MCP C# SDK) frames it:
//!
//! * `initialize` answers with the client's protocol version when the server knows it,
//!   otherwise with the latest one it knows, plus `capabilities`, `serverInfo` and
//!   `instructions`.
//! * `ping` answers `{}`; `tools/list` answers `{"tools": [...]}`.
//! * `tools/call` answers `{"content":[{"type":"text","text":"<compact JSON>"}]}`. A tool that
//!   fails answers the same shape with `"isError": true` and the text
//!   `An error occurred invoking '<tool>': <reason>`; an argument the tool does not declare is
//!   refused the same way. An unknown tool is a protocol error (-32602).
//! * An unknown method is -32601; a message that is not a JSON-RPC request is -32700.
//! * Notifications (no `id`) are never answered.
//!
//! Standard output carries protocol messages only; anything else goes to standard error.

use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::{json, Map, Value};

use crate::registry::{self, Args, ToolSpec};
use crate::tools::Tools;

pub const SERVER_NAME: &str = "afrobim";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The protocol revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "AfroBIM lays out road signs to the UK Traffic Signs Manual Chapter 7 (default pack UK-TSM-2019). \
Find signs with search_catalog, preview with layout_sign, check a schedule CSV with validate_schedule, and write one DXF per sign \
with export_dxf. To build a model the AfroBIM desktop app can open, create an .afrobim file with document_new, add signs with \
document_add_sign, review it with document_list and write it to a single DXF with document_export_dxf.";

pub const PARSE_ERROR: i64 = -32700;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

pub struct Server {
    tools: Tools,
    specs: Vec<ToolSpec>,
}

fn result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

fn tool_result(text: String, is_error: bool) -> Value {
    let mut r = json!({ "content": [{ "type": "text", "text": text }] });
    if is_error {
        r["isError"] = Value::Bool(true);
    }
    r
}

impl Server {
    pub fn new(tools: Tools) -> Server {
        Server {
            tools,
            specs: registry::all(),
        }
    }

    /// Reads requests from `input` until it ends, writing one response line per request.
    pub fn serve<R: BufRead, W: Write>(
        &mut self,
        mut input: R,
        mut output: W,
    ) -> std::io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            if input.read_until(b'\n', &mut line)? == 0 {
                return Ok(());
            }
            let text = String::from_utf8_lossy(&line);
            if let Some(response) = self.handle_line(text.trim()) {
                serde_json::to_writer(&mut output, &response)?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }

    /// One line in, at most one message out. Blank lines are skipped.
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        if line.is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(message)) => self.handle_message(&message),
            // Not JSON, or JSON that is not a message object (batches are not part of MCP).
            _ => Some(error(
                &Value::Null,
                PARSE_ERROR,
                "Failed to parse the JSON-RPC request.",
            )),
        }
    }

    fn handle_message(&mut self, message: &Map<String, Value>) -> Option<Value> {
        let id = message
            .get("id")
            .filter(|id| id.is_string() || id.is_number());
        let well_formed = message.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
        let method = message.get("method").and_then(Value::as_str);
        let (Some(method), true) = (method, well_formed) else {
            // A response to a request this server never sent is dropped; anything else that is
            // not a request or a notification is malformed.
            let is_response =
                well_formed && (message.contains_key("result") || message.contains_key("error"));
            return (!is_response).then(|| {
                error(
                    id.unwrap_or(&Value::Null),
                    PARSE_ERROR,
                    "Failed to parse the JSON-RPC request.",
                )
            });
        };
        // A notification is never answered, whatever it is.
        let id = id?;
        let params = message.get("params").filter(|p| !p.is_null());
        Some(match method {
            "initialize" => result(id, self.initialize(params)),
            "ping" => result(id, json!({})),
            "tools/list" => result(
                id,
                json!({ "tools": self.specs.iter().map(ToolSpec::descriptor).collect::<Vec<_>>() }),
            ),
            "tools/call" => match self.call_tool(params) {
                Ok(r) => result(id, r),
                Err(message) => error(id, INVALID_PARAMS, message),
            },
            other => error(
                id,
                METHOD_NOT_FOUND,
                format!("Method '{other}' is not available."),
            ),
        })
    }

    fn initialize(&self, params: Option<&Value>) -> Value {
        let requested = params
            .and_then(|p| p.get("protocolVersion"))
            .and_then(Value::as_str);
        let version = requested
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            "instructions": INSTRUCTIONS,
        })
    }

    /// `Err` is a protocol error (bad params, unknown tool); a tool that fails is an `Ok`
    /// result with `isError`, so the assistant can read the reason and try again.
    fn call_tool(&mut self, params: Option<&Value>) -> Result<Value, String> {
        let params = params
            .and_then(Value::as_object)
            .ok_or("tools/call needs a params object with the tool 'name'")?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or("tools/call needs the tool 'name'")?;
        let empty = Map::new();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(map)) => map,
            Some(_) => return Err("tools/call 'arguments' must be an object".to_string()),
        };
        let Some(spec) = self.specs.iter().find(|t| t.name == name) else {
            return Err(format!("Unknown tool: '{name}'"));
        };
        if let Some(refusal) = spec.unknown_argument(arguments) {
            return Ok(tool_result(refusal, true));
        }
        if !spec.read_only && self.tools.engine.settings.read_only() {
            return Ok(tool_result(
                format!(
                    "An error occurred invoking '{name}': refused: the server is in read-only mode (--read-only or {}), and {name} writes files",
                    crate::settings::READ_ONLY_ENV
                ),
                true,
            ));
        }
        let tools = &mut self.tools;
        // A panic inside the engine must not take the server (and the assistant's session) down.
        let outcome = catch_unwind(AssertUnwindSafe(|| (spec.call)(tools, &Args(arguments))));
        Ok(match outcome {
            Ok(Ok(text)) => tool_result(text, false),
            Ok(Err(e)) => tool_result(format!("An error occurred invoking '{name}': {e}"), true),
            Err(_) => tool_result(format!("An error occurred invoking '{name}'."), true),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn server() -> Server {
        Server::new(Tools::new(testing::settings()))
    }

    fn send(s: &mut Server, message: Value) -> Value {
        s.handle_line(&message.to_string()).expect("a response")
    }

    fn call(s: &mut Server, name: &str, arguments: Value) -> Value {
        send(
            s,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
    }

    #[test]
    fn initialize_echoes_a_known_protocol_version_and_offers_the_latest_otherwise() {
        let mut s = server();
        let init = |version: &str| json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
        let r = send(&mut s, init("2025-06-18"));
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            r["result"]["capabilities"],
            json!({ "tools": { "listChanged": false } })
        );
        assert_eq!(
            r["result"]["serverInfo"],
            json!({ "name": "afrobim", "version": SERVER_VERSION })
        );
        assert!(r["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("document_add_sign"));
        assert_eq!(
            send(&mut s, init("2024-11-05"))["result"]["protocolVersion"],
            "2024-11-05"
        );
        assert_eq!(
            send(&mut s, init("1999-01-01"))["result"]["protocolVersion"],
            "2025-11-25"
        );
        let bare = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize" }),
        );
        assert_eq!(bare["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn ping_answers_an_empty_result_and_keeps_the_id_as_sent() {
        let mut s = server();
        assert_eq!(
            send(
                &mut s,
                json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })
            ),
            json!({ "jsonrpc": "2.0", "id": 7, "result": {} })
        );
        assert_eq!(
            send(
                &mut s,
                json!({ "jsonrpc": "2.0", "id": "abc", "method": "ping" })
            )["id"],
            "abc"
        );
    }

    #[test]
    fn notifications_and_blank_lines_are_not_answered() {
        let mut s = server();
        assert_eq!(
            s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            None
        );
        assert_eq!(
            s.handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#
            ),
            None
        );
        assert_eq!(
            s.handle_line(r#"{"jsonrpc":"2.0","method":"no/such/notification"}"#),
            None
        );
        assert_eq!(
            s.handle_line(r#"{"jsonrpc":"2.0","method":"tools/list"}"#),
            None
        );
        assert_eq!(s.handle_line(""), None);
        // A response to a request this server never made is dropped.
        assert_eq!(
            s.handle_line(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#),
            None
        );
    }

    #[test]
    fn tools_list_describes_every_tool() {
        let mut s = server();
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {} }),
        );
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), registry::all().len());
        assert_eq!(tools[0]["name"], "list_packs");
        assert!(tools
            .iter()
            .all(|t| t["inputSchema"]["additionalProperties"] == false));
        assert_eq!(r["result"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn a_tool_result_is_one_text_block_of_json_without_is_error() {
        let mut s = server();
        let r = call(&mut s, "search_catalog", json!({ "query": "stack" }));
        assert!(r.get("error").is_none());
        assert!(r["result"].get("isError").is_none());
        let content = r["result"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        let payload: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(payload["pack"], "UK-TSM-2019");
        assert!(payload["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["key"] == "UK04.1"));
        // Arguments may be left out altogether, or be null.
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_packs" } }),
        );
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("{\"default_pack\":\"UK-TSM-2019\""));
        assert!(call(&mut s, "list_packs", Value::Null)["result"]
            .get("isError")
            .is_none());
    }

    #[test]
    fn tool_failures_are_results_with_is_error_and_the_reason() {
        let mut s = server();
        let r = call(&mut s, "layout_sign", json!({ "key": "NOPE" }));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(
            r["result"]["content"][0]["text"],
            "An error occurred invoking 'layout_sign': catalog sign page 'NOPE' not found in rule pack 'UK-TSM-2019'"
        );
        let r = call(&mut s, "search_catalog", json!({ "qery": "stack" }));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(
            r["result"]["content"][0]["text"],
            "unknown argument(s) for search_catalog: qery; accepted: pack, query, series"
        );
        let r = call(&mut s, "export_dxf", json!({}));
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("An error occurred invoking 'export_dxf'"));
    }

    #[test]
    fn protocol_errors_use_the_json_rpc_codes() {
        let mut s = server();
        let r = call(&mut s, "nope", json!({}));
        assert_eq!(
            r["error"],
            json!({ "code": -32602, "message": "Unknown tool: 'nope'" })
        );
        assert!(r.get("result").is_none());
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "arguments": {} } }),
        );
        assert_eq!(r["error"]["code"], -32602);
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call" }),
        );
        assert_eq!(r["error"]["code"], -32602);
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "list_packs", "arguments": [1] } }),
        );
        assert_eq!(r["error"]["code"], -32602);
        let r = send(
            &mut s,
            json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }),
        );
        assert_eq!(
            r["error"],
            json!({ "code": -32601, "message": "Method 'resources/list' is not available." })
        );
        assert_eq!(r["id"], 5);
    }

    #[test]
    fn malformed_messages_get_a_parse_error() {
        let mut s = server();
        let parse_error =
            json!({ "code": -32700, "message": "Failed to parse the JSON-RPC request." });
        let r = s.handle_line("this is not json").unwrap();
        assert_eq!(
            r,
            json!({ "jsonrpc": "2.0", "id": null, "error": parse_error })
        );
        assert_eq!(s.handle_line("[1,2]").unwrap()["error"], parse_error);
        assert_eq!(
            s.handle_line("{\"jsonrpc\":\"2.0\",\"id\":1,").unwrap()["error"],
            parse_error
        );
        // No "jsonrpc": the id is still echoed, as the C# server does.
        let r = s.handle_line(r#"{"id":13,"method":"ping"}"#).unwrap();
        assert_eq!((&r["id"], &r["error"]), (&json!(13), &parse_error));
        let r = s.handle_line(r#"{"jsonrpc":"2.0","id":14}"#).unwrap();
        assert_eq!((&r["id"], &r["error"]), (&json!(14), &parse_error));
        // The server keeps answering afterwards.
        assert_eq!(
            send(
                &mut s,
                json!({ "jsonrpc": "2.0", "id": 15, "method": "ping" })
            )["result"],
            json!({})
        );
    }

    #[test]
    fn serve_writes_one_line_per_request_and_stops_at_end_of_input() {
        let mut s = server();
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            "\r\n\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            "\u{fffd}garbage\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
        );
        let mut out = Vec::new();
        s.serve(input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(text.ends_with('\n'));
        assert_eq!(lines.len(), 3);
        assert_eq!(
            (&lines[0]["id"], &lines[1]["error"]["code"], &lines[2]["id"]),
            (&json!(1), &json!(-32700), &json!(2))
        );
    }

    #[test]
    fn a_missing_pack_is_reported_by_the_tools_while_the_protocol_still_works() {
        let settings = crate::settings::Settings::new(
            Err("rule pack packs/UK-TSM-2019.json was not found".into()),
            Err("no fonts".into()),
            std::path::Path::new("/"),
        );
        let mut s = Server::new(Tools::new(settings));
        assert_eq!(
            send(
                &mut s,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
            )["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            registry::all().len()
        );
        let r = call(&mut s, "list_packs", json!({}));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["content"][0]["text"], "An error occurred invoking 'list_packs': rule pack packs/UK-TSM-2019.json was not found");
    }
}
