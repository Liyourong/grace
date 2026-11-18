use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[derive(Deserialize, Serialize)]
struct Step {
    name: String,
    us: u64,
    ok: bool,
}
#[derive(Deserialize, Serialize)]
struct Summary {
    id: String,
    time: u64,
    method: String,
    route: String,
    status: u16,
    us: u64,
    #[serde(skip_deserializing)]
    pod: String,
}
#[derive(Deserialize, Serialize)]
struct Trace {
    #[serde(flatten)]
    request: Summary,
    steps: Vec<Step>,
    rejected_at: Option<String>,
}
#[derive(Clone, Default)]
pub struct Logs(Arc<Mutex<Vec<Trace>>>);

impl Logs {
    pub fn ingest(&self, pod: &str, value: serde_json::Value) {
        let Ok(mut trace) = serde_json::from_value::<Trace>(value) else {
            return;
        };
        let r = &mut trace.request;
        if r.id.len() != 32
            || !r.id.bytes().all(|b| b.is_ascii_hexdigit())
            || !matches!(
                r.method.as_str(),
                "GET" | "HEAD" | "PUT" | "POST" | "PATCH" | "DELETE" | "OPTIONS" | "OTHER"
            )
            || !matches!(
                r.route.as_str(),
                "/api/auth/login"
                    | "/api/auth/logout"
                    | "/api/auth/me"
                    | "/api/auth/register"
                    | "/api/auth/register/resend"
                    | "/api/auth/verify"
                    | "/api/practice/time"
                    | "/api/folders"
                    | "/api/folders/{folder_id}"
                    | "/api/papers"
                    | "/api/papers/{paper_id}"
                    | "/api/papers/{paper_id}/position"
                    | "/api/papers/{paper_id}/finish"
                    | "/api/papers/{paper_id}/answers/{question_id}"
                    | "/api/papers/{paper_id}/answers/{question_id}/explain"
            )
            || !(100..=599).contains(&r.status)
            || trace.steps.len() > 4
            || !matches!(
                trace.rejected_at.as_deref(),
                None | Some("Origin check" | "Request body" | "Credential validation")
            )
            || !trace.steps.iter().all(|s| {
                matches!(
                    s.name.as_str(),
                    "Account lookup"
                        | "Password check"
                        | "Session creation"
                        | "Session check"
                        | "Question lookup"
                        | "Grading"
                        | "Save transaction"
                )
            })
        {
            return;
        }
        r.pod = pod.to_owned();
        let mut entries = self.0.lock().unwrap();
        if entries.iter().any(|e| e.request.id == r.id) {
            return;
        }
        let i = entries.partition_point(|e| (e.request.time, &e.request.id) > (r.time, &r.id));
        if i < 500 {
            entries.insert(i, trace);
            entries.truncate(500);
        }
    }
}
pub fn valid_key(headers: &HeaderMap, verifier: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|key| {
            key.len() == 64 && hex::encode(Sha256::digest(key.as_bytes())) == verifier
        })
}

async fn private(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let local = peer.ip().is_loopback()
        && matches!(host, "127.0.0.1:19090" | "localhost:19090" | "[::1]:19090")
        && headers
            .get("origin")
            .is_none_or(|o| o.as_bytes() == format!("http://{host}").as_bytes());
    let mut response = if !local {
        StatusCode::FORBIDDEN.into_response()
    } else if !std::env::var("GRACE_PRESENTER_SHA256").is_ok_and(|v| valid_key(headers, &v)) {
        StatusCode::UNAUTHORIZED.into_response()
    } else {
        next.run(req).await
    };
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
}

pub fn router(logs: Logs) -> Router<super::Store> {
    Router::new()
        .route(
            "/api/logs",
            get(|State(logs): State<Logs>| async move {
                let entries = logs.0.lock().unwrap();
                Json(entries.iter().map(|t| &t.request).collect::<Vec<_>>()).into_response()
            }),
        )
        .route(
            "/api/traces/{id}",
            get(
                |State(logs): State<Logs>, Path(id): Path<String>| async move {
                    let entries = logs.0.lock().unwrap();
                    match entries.iter().find(|t| t.request.id == id) {
                        Some(trace) => Json(trace).into_response(),
                        None => StatusCode::NOT_FOUND.into_response(),
                    }
                },
            ),
        )
        .layer(from_fn(private))
        .with_state(logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn logs_library_templates_without_record_ids() {
        let logs = Logs::default();
        for (i, (method, route)) in [
            ("POST", "/api/folders"),
            ("PATCH", "/api/folders/{folder_id}"),
            ("DELETE", "/api/papers/{paper_id}"),
            ("DELETE", "/api/folders/123"),
        ].into_iter().enumerate() {
            logs.ingest("app-a", json!({"id":format!("{i:032x}"), "time":i, "route":route,
                "method":method, "status":200, "us":1, "steps":[]}));
        }
        let entries = logs.0.lock().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|e| !e.request.route.contains("123")));
    }

    #[test]
    fn keeps_newest_on_replay_and_ignores_untrusted_fields() {
        let logs = Logs::default();
        for i in (0..501).chain([11, 0]).rev().chain([0, 11, 500]) {
            logs.ingest("app-a", json!({"id":format!("{i:032x}"),"time":i,"route":"/api/auth/login",
                "method":"POST","status":200,"us":9,"pod":"forged","steps":[],"password":"private"}));
        }
        let entries = logs.0.lock().unwrap();
        assert_eq!(
            (
                entries.len(),
                entries[0].request.time,
                entries[499].request.time
            ),
            (500, 500, 1)
        );
        let v = serde_json::to_value(&entries[0]).unwrap();
        assert_eq!(v["pod"], "app-a");
        assert!(v.get("password").is_none());
    }
}
