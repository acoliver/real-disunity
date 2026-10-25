#[path = "support/http.rs"]
mod http;
#[path = "support/root.rs"]
mod root;
use http::{Server, exchange, root, snapshot};
use serde_json::{Value, json};
use std::{
    thread,
    time::{Duration, Instant},
};

const ROUTES: [&str; 2] = ["/api/run", "/api/stop"];
fn mutation_body() -> Value {
    json!({"fixture":"harness-only","accept":true,"id":"not-a-job"})
}
fn assert_idle(server: &Server) {
    let state: Value = serde_json::from_str(&server.get("/api/state").body).unwrap();
    assert_eq!(state["tasks"], json!([]));
    assert_eq!(state["runs"], json!([]));
}
fn rejected_without_effects(
    server: &Server,
    root: &std::path::Path,
    headers: &[(&str, &str)],
    status: u16,
) {
    let before = snapshot(root);
    let responses: Vec<_> = ROUTES
        .iter()
        .map(|route| exchange(&server.address, "POST", route, &mutation_body(), headers).status)
        .collect();
    assert_eq!(
        responses,
        vec![status; ROUTES.len()],
        "Run / Stop must reject before admission"
    );
    assert_idle(server);
    assert_eq!(
        snapshot(root),
        before,
        "rejected requests changed files or directories"
    );
}
#[test]
fn missing_launch_token_rejects_all_mutations_without_jobs_or_file_changes() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let origin = format!("http://{}", server.address);
    rejected_without_effects(&server, root.path(), &[("Origin", &origin)], 403);
    rejected_without_effects(&server, root.path(), &[], 403);
}
#[test]
fn wrong_launch_token_rejects_all_mutations_without_jobs_or_file_changes() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let origin = format!("http://{}", server.address);
    rejected_without_effects(
        &server,
        root.path(),
        &[("Origin", &origin), ("X-Compat-Token", &"0".repeat(64))],
        403,
    );
}
#[test]
fn page_bootstraps_a_stable_secret_without_leaking_it_in_state_or_announcement() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let before = snapshot(root.path());
    let token = server.token();
    assert_eq!(server.token(), token);
    assert!(!server.announcement.contains(&token));
    let state = server.get("/api/state");
    assert!(!state.body.contains(&token));
    assert!(!state.headers.contains_key("access-control-allow-origin"));
    let page = server.get("/");
    assert_eq!(page.headers["cache-control"], "no-store");
    assert_eq!(page.headers["x-frame-options"], "DENY");
    assert_eq!(page.headers["referrer-policy"], "no-referrer");
    let csp = &page.headers["content-security-policy"];
    for directive in [
        "frame-ancestors 'none'",
        "base-uri 'none'",
        "form-action 'none'",
        "connect-src 'self'",
    ] {
        assert!(csp.contains(directive), "missing {directive}");
    }
    assert_eq!(
        snapshot(root.path()),
        before,
        "page bootstrap persisted data"
    );
}
#[test]
fn foreign_null_alias_and_malformed_origins_reject_even_with_valid_token() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let address = &server.address;
    let alias = address.replace("127.0.0.1", "localhost");
    for origin in [
        "null".into(),
        "http://attacker.example".into(),
        format!("http://{alias}"),
        format!("https://{address}"),
        format!("http://{address}/"),
        "http://127.0.0.1:1".into(),
        format!("http://{address}, http://{address}"),
        format!("http://{address}.attacker.example"),
    ] {
        rejected_without_effects(
            &server,
            root.path(),
            &[("Origin", &origin), ("X-Compat-Token", &token)],
            403,
        );
    }
}
#[test]
fn host_mismatch_blocks_page_disclosure_and_all_mutations() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    for host in ["attacker.example", "127.0.0.1:1", "localhost:1", ""] {
        let headers = [
            ("Host", host),
            ("Origin", "http://attacker.example"),
            ("X-Compat-Token", &token),
        ];
        let page = exchange(&server.address, "GET", "/", &Value::Null, &headers);
        assert_eq!(page.status, 403);
        assert!(!page.body.contains(&token));
        rejected_without_effects(&server, root.path(), &headers, 403);
    }
}
#[test]
fn duplicate_security_headers_are_rejected_before_all_mutations() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let origin = format!("http://{}", server.address);
    for duplicate in [
        ("oRiGiN", origin.as_str()),
        ("x-compat-token", token.as_str()),
        ("HOST", server.address.as_str()),
    ] {
        let headers = [
            ("Host", server.address.as_str()),
            ("Origin", origin.as_str()),
            ("X-Compat-Token", token.as_str()),
            duplicate,
        ];
        rejected_without_effects(&server, root.path(), &headers, 400);
    }
}
#[test]
fn token_is_exact_and_cannot_be_supplied_in_a_body_or_legacy_header() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let origin = format!("http://{}", server.address);
    for wrong in [
        String::new(),
        token[..63].into(),
        format!("{token}0"),
        format!("z{}", &token[1..]),
        format!("{}z", &token[..63]),
        format!("{token},{token}"),
    ] {
        rejected_without_effects(
            &server,
            root.path(),
            &[("Origin", &origin), ("X-Compat-Token", &wrong)],
            403,
        );
    }
    rejected_without_effects(
        &server,
        root.path(),
        &[("Origin", &origin), ("X-Compat-Mutation-Token", &token)],
        403,
    );
    let before = snapshot(root.path());
    for route in ROUTES {
        let mut body = mutation_body();
        body["token"] = json!(token);
        assert_eq!(
            exchange(
                &server.address,
                "POST",
                route,
                &body,
                &[("Origin", &origin)]
            )
            .status,
            403
        );
    }
    assert_idle(&server);
    assert_eq!(snapshot(root.path()), before);
}
#[test]
fn every_launch_rotates_the_token_and_rejects_the_previous_launch() {
    let root = root("pass");
    let first = Server::launch(root.path());
    let old = first.token();
    drop(first);
    let next = Server::launch(root.path());
    assert_ne!(next.token(), old);
    let origin = format!("http://{}", next.address);
    rejected_without_effects(
        &next,
        root.path(),
        &[("Origin", &origin), ("X-Compat-Token", &old)],
        403,
    );
    let response = next.post("/api/run", &mutation_body(), &next.token());
    assert_eq!(response.status, 202, "{}", response.body);
    wait_done(&next);
}
fn wait_done(server: &Server) -> Value {
    let start = Instant::now();
    loop {
        let state: Value = serde_json::from_str(&server.get("/api/state").body).unwrap();
        if state["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["done"] == true)
        {
            return state;
        }
        assert!(start.elapsed() < Duration::from_secs(6));
        thread::sleep(Duration::from_millis(10));
    }
}
fn start_hanging_job(server: &Server, token: &str) -> Value {
    let response = server.post("/api/run", &mutation_body(), token);
    assert_eq!(response.status, 202, "{}", response.body);
    serde_json::from_str(&response.body).unwrap()
}
#[test]
fn unauthorized_stop_cannot_cancel_an_active_job() {
    let root = root("hang");
    let server = Server::launch(root.path());
    let token = server.token();
    let job = start_hanging_job(&server, &token);
    let body = json!({"id":job["id"]});
    for headers in [
        vec![],
        vec![("X-Compat-Token", "wrong")],
        vec![("X-Compat-Token", token.as_str()), ("Origin", "null")],
    ] {
        assert_eq!(
            exchange(&server.address, "POST", "/api/stop", &body, &headers).status,
            403
        );
    }
    let state = wait_done(&server);
    assert_eq!(
        state["runs"][0]["status"], "error",
        "unauthorized Stop cancelled the worker before its deadline"
    );
}
#[test]
fn trusted_cli_may_omit_origin_with_valid_token_for_run_and_stop() {
    let root = root("hang");
    let server = Server::launch(root.path());
    let token = server.token();
    let headers = [("X-Compat-Token", token.as_str())];
    let run = exchange(
        &server.address,
        "POST",
        "/api/run",
        &mutation_body(),
        &headers,
    );
    assert_eq!(run.status, 202, "{}", run.body);
    let job: Value = serde_json::from_str(&run.body).unwrap();
    assert_eq!(
        exchange(
            &server.address,
            "POST",
            "/api/stop",
            &json!({"id":job["id"]}),
            &headers
        )
        .status,
        202
    );
    wait_done(&server);
    assert_eq!(
        exchange(
            &server.address,
            "POST",
            "/api/removed",
            &mutation_body(),
            &headers
        )
        .status,
        404
    );
    wait_done(&server);
}
#[test]
fn localhost_alias_requires_its_own_exact_origin() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let alias = server.address.replace("127.0.0.1", "localhost");
    let page = exchange(
        &server.address,
        "GET",
        "/",
        &Value::Null,
        &[("Host", &alias)],
    );
    assert_eq!(page.status, 200);
    assert!(page.body.contains(&token));
    let origin = format!("http://{alias}");
    let headers = [
        ("Host", alias.as_str()),
        ("Origin", origin.as_str()),
        ("X-Compat-Token", token.as_str()),
    ];
    assert_eq!(
        exchange(
            &server.address,
            "POST",
            "/api/run",
            &mutation_body(),
            &headers
        )
        .status,
        202
    );
    wait_done(&server);
}

#[test]
fn foreign_get_cannot_disclose_bootstrap_or_state_and_get_routes_cannot_mutate() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let before = snapshot(root.path());
    for route in ["/", "/api/state"] {
        for origin in ["null", "http://attacker.example"] {
            let response = exchange(
                &server.address,
                "GET",
                route,
                &Value::Null,
                &[("Origin", origin)],
            );
            assert_eq!(response.status, 403);
            assert!(!response.body.contains(&token));
            assert!(!response.headers.contains_key("access-control-allow-origin"));
        }
    }
    for route in ROUTES {
        assert_eq!(server.get(route).status, 404);
        let preflight = exchange(
            &server.address,
            "OPTIONS",
            route,
            &Value::Null,
            &[
                ("Origin", "http://attacker.example"),
                ("Access-Control-Request-Headers", "X-Compat-Token"),
            ],
        );
        assert_eq!(preflight.status, 403);
        assert!(
            !preflight
                .headers
                .contains_key("access-control-allow-origin")
        );
    }
    assert_idle(&server);
    assert_eq!(snapshot(root.path()), before);
}
#[test]
fn static_reports_never_persist_the_live_launch_token() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let token = server.token();
    let page =
        compat_kit::dashboard::generate(root.path(), std::path::Path::new(".work/static")).unwrap();
    let page = std::fs::read_to_string(page).unwrap();
    assert!(!page.contains("<meta name=\"compat-token\""));
    assert!(!page.contains(&token));
    let state = std::fs::read_to_string(root.path().join(".work/static/state.json")).unwrap();
    assert!(!state.contains(&token));
}

#[test]
fn authenticated_stop_records_structured_disposition_and_http_state_groups() {
    let root = root("hang");
    let server = Server::launch(root.path());
    let token = server.token();
    let admission = server.post("/api/run", &json!({"fixture":"harness-only"}), &token);
    assert_eq!(admission.status, 202);
    let job: Value = serde_json::from_str(&admission.body).unwrap();
    assert_eq!(
        server
            .post("/api/stop", &json!({"id":job["id"]}), &token)
            .status,
        202
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let response = server.get("/api/state");
        assert_eq!(response.status, 200);
        let state: Value = serde_json::from_str(&response.body).unwrap();
        if let Some(record) = state["runs"].as_array().unwrap().first() {
            assert_eq!(record["status"], "skipped");
            assert_eq!(record["reporting"]["exit_code"], 4);
            assert_eq!(
                record["reporting"]["failure_causes"][0]["kind"],
                "cancelled"
            );
            assert_eq!(state["groups"][0]["attempts"].as_array().unwrap().len(), 1);
            assert_eq!(state["groups"][0]["status"], "skipped");
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn duplicate_headers_with_large_bodies_return_complete_rejection_without_effects() {
    let root = root("pass");
    let server = Server::launch(root.path());
    let before = snapshot(root.path());
    let response = exchange(
        &server.address,
        "POST",
        "/api/run",
        &json!({"fixture":"harness-only","padding":"x".repeat(262_144)}),
        &[("Host", &server.address), ("Host", &server.address)],
    );
    assert_eq!(response.status, 400);
    assert_idle(&server);
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn early_rejection_finishes_the_response_without_waiting_for_untrusted_body() {
    use std::io::{Read, Write};
    let root = root("pass");
    let server = Server::launch(root.path());
    let before = snapshot(root.path());
    let mut client = std::net::TcpStream::connect(&server.address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    write!(
        client,
        "POST /api/run HTTP/1.1\r\nHost: {}\r\nHost: {}\r\nContent-Length: 999999999\r\n\r\n",
        server.address, server.address
    )
    .unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 400"));
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap()["error"],
        "duplicate HTTP header"
    );
    let started = Instant::now();
    assert_idle(&server);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(snapshot(root.path()), before);
}
