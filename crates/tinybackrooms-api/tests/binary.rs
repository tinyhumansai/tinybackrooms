//! Runs the real `tinybackrooms-api` binary: startup, serving, bad
//! configuration, a taken port, and a clean exit on `Ctrl-C`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tinybackrooms-api");

fn start(env: &[(&str, &str)]) -> Child {
    Command::new(BIN)
        .env_remove("TINYBACKROOMS_ADDR")
        .env_remove("TINYBACKROOMS_BACKEND")
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Send one HTTP/1.1 request and return the whole response.
fn http(addr: &str, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn serves_the_api_and_exits_cleanly_on_interrupt() {
    let mut child = start(&[("TINYBACKROOMS_ADDR", "127.0.0.1:0")]);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let addr = line
        .trim()
        .strip_prefix("listening on http://")
        .expect(&line)
        .to_owned();

    assert!(http(&addr, "GET", "/healthz", "").starts_with("HTTP/1.1 200"));
    let body = r#"{"input":{"brief":"name a cafe"},"output":{"schema":{"type":"object",
        "required":["name"],"properties":{"name":{"type":"string"}}}},
        "config":{"variations":1,"generations":2,"items_per_variation":2}}"#;
    let reply = http(&addr, "POST", "/v1/generate", body);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains("\"items\""));

    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(child.wait().unwrap().success());
}

#[test]
fn refuses_an_invalid_configuration() {
    let child = start(&[("TINYBACKROOMS_ADDR", "not-an-address")]);
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid listen address"), "{stderr}");

    let output = start(&[("TINYBACKROOMS_BACKEND", "magic")])
        .wait_with_output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown backend")
    );
}

#[test]
fn refuses_a_port_that_is_already_taken() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let output = start(&[("TINYBACKROOMS_ADDR", &addr)])
        .wait_with_output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot bind")
    );
}

#[test]
fn the_openhuman_backend_needs_an_api_key() {
    let output = start(&[("TINYBACKROOMS_BACKEND", "openhuman")])
        .wait_with_output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("TINYBACKROOMS_OPENHUMAN_API_KEY must be set"),
        "{stderr}"
    );
}

#[cfg(not(feature = "openhuman"))]
#[test]
fn the_openhuman_backend_needs_the_feature() {
    let output = start(&[
        ("TINYBACKROOMS_BACKEND", "openhuman"),
        ("TINYBACKROOMS_OPENHUMAN_API_KEY", "k"),
    ])
    .wait_with_output()
    .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--features openhuman")
    );
}

/// HTTP request, engine, `OpenHuman` agents, typed-output tool, and a fake model.
#[cfg(feature = "openhuman")]
mod openhuman {
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    use super::*;

    /// A model that calls whichever typed tool it is offered, and otherwise talks.
    struct Model;

    fn completion(message: &Value, finish: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "fake", "object": "chat.completion", "created": 1_700_000_000, "model": "fake",
            "choices": [{"index": 0, "message": message, "finish_reason": finish}],
            "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9},
        }))
    }

    impl Respond for Model {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let last = body["messages"]
                .as_array()
                .and_then(|m| m.last())
                .cloned()
                .unwrap_or_default();
            if last["role"] == "tool" {
                return completion(
                    &json!({"role": "assistant", "content": "Submitted."}),
                    "stop",
                );
            }
            let tool = body["tools"]
                .as_array()
                .and_then(|tools| {
                    tools.iter().find_map(|t| {
                        t["function"]["name"]
                            .as_str()
                            .filter(|n| matches!(*n, "submit_items" | "rate_item"))
                    })
                })
                .map(str::to_owned);
            let Some(name) = tool else {
                return completion(
                    &json!({"role": "assistant", "content": "ls ideas/"}),
                    "stop",
                );
            };
            let call = json!({"id": "call-1", "type": "function", "function": {
                "name": name,
                "arguments": json!({"items": [{"name": "Salt Light"}, {"name": "Lamp Room"}]}).to_string(),
            }});
            completion(
                &json!({"role": "assistant", "content": null, "tool_calls": [call]}),
                "tool_calls",
            )
        }
    }

    #[test]
    fn serves_generate_through_openhuman_agents() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let provider = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(Model)
                .mount(&server)
                .await;
            server
        });
        let base = format!("{}/v1", provider.uri());
        let mut child = start(&[
            ("TINYBACKROOMS_ADDR", "127.0.0.1:0"),
            ("TINYBACKROOMS_BACKEND", "openhuman"),
            ("TINYBACKROOMS_OPENHUMAN_API_KEY", "fixture-key"),
            ("TINYBACKROOMS_OPENHUMAN_BASE_URL", &base),
            ("TINYBACKROOMS_OPENHUMAN_MODEL", "fake-model"),
            ("TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS", "30"),
        ]);
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let addr = line
            .trim()
            .strip_prefix("listening on http://")
            .expect(&line)
            .to_owned();

        let body = r#"{"input":{"brief":"name a cafe"},"output":{"schema":{"type":"object",
            "required":["name"],"properties":{"name":{"type":"string"}}}},
            "config":{"variations":1,"generations":2,"items_per_variation":2}}"#;
        let reply = http(&addr, "POST", "/v1/generate", body);
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert!(
            reply.contains("Salt Light") && reply.contains("Lamp Room"),
            "{reply}"
        );

        // Two conversation turns and one formatter call reached the model, plus
        // the tool-result turn the agent runtime makes after the typed call.
        let calls = runtime
            .block_on(provider.received_requests())
            .unwrap()
            .len();
        assert!(calls >= 3, "only {calls} model calls");

        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap();
        assert!(child.wait().unwrap().success());
    }
}
