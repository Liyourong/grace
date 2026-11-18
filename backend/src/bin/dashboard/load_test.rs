use super::{Store, View, now};
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::Duration};

#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Running,
    Reached,
    Stopped,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Serialize)]
pub struct Test {
    phase: Phase,
    deadline: u64,
}
impl Default for Test {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            deadline: 0,
        }
    }
}

pub fn enabled() -> bool {
    std::env::var("GRACE_HPA_DEMO").as_deref() == Ok("1")
}

fn available(view: &View) -> Option<u64> {
    view.resources.get("deployments")?.get("app")?["available"].as_u64()
}

fn observed(view: &View) -> bool {
    ["deployments", "hpa"]
        .iter()
        .all(|k| view.synced.get(*k) == Some(&true))
        && view
            .resources
            .get("hpa")
            .and_then(|r| r.get("app"))
            .is_some_and(|h| h["max"] == 5 && h["target"].as_u64().is_some())
        && available(view).is_some()
}

fn finish(view: &mut View, time: u64) -> bool {
    if !view
        .load_test
        .as_ref()
        .is_some_and(|t| t.phase == Phase::Running)
    {
        return false;
    }
    let phase = if !observed(view) {
        Phase::Interrupted
    } else if available(view) >= Some(5) {
        Phase::Reached
    } else if time >= view.load_test.as_ref().unwrap().deadline {
        Phase::TimedOut
    } else {
        return false;
    };
    view.load_test.as_mut().unwrap().phase = phase;
    true
}
pub async fn monitor(s: Store) {
    loop {
        s.send_if_modified(|v| finish(v, now()));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
pub async fn lease(State(s): State<Store>) -> StatusCode {
    let v = s.borrow();
    if v.load_test
        .as_ref()
        .is_some_and(|t| t.phase == Phase::Running && now() < t.deadline)
        && observed(&v)
        && available(&v) < Some(5)
    {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::OK
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Start,
    Stop,
}

fn change(view: &mut View, cmd: Command, time: u64) -> StatusCode {
    let can_start = observed(view) && available(view) < Some(5);
    let Some(test) = view.load_test.as_mut() else {
        return StatusCode::NOT_FOUND;
    };
    match cmd {
        Command::Start if test.phase == Phase::Running => StatusCode::OK,
        Command::Start if !can_start => StatusCode::CONFLICT,
        Command::Start => {
            test.phase = Phase::Running;
            test.deadline = time + 180_000;
            StatusCode::ACCEPTED
        }
        Command::Stop => {
            if test.phase == Phase::Running {
                test.phase = Phase::Stopped;
            }
            StatusCode::OK
        }
    }
}
pub async fn control(
    State(s): State<Store>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(cmd): Json<Command>,
) -> StatusCode {
    // Kubernetes port-forward connects to Pod loopback. Other Pods may only read the lease.
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let origin = headers.get("origin").and_then(|h| h.to_str().ok());
    if !peer.ip().is_loopback()
        || !matches!(
            host,
            "localhost:9090"
                | "127.0.0.1:9090"
                | "[::1]:9090"
                | "localhost:19090"
                | "127.0.0.1:19090"
                | "[::1]:19090"
        )
        || origin != Some(format!("http://{host}").as_str())
    {
        return StatusCode::FORBIDDEN;
    }
    if let Ok(verifier) = std::env::var("GRACE_PRESENTER_SHA256") {
        if !super::logs::valid_key(&headers, &verifier) {
            return StatusCode::UNAUTHORIZED;
        }
    }
    let mut status = StatusCode::OK;
    s.send_modify(|v| status = change(v, cmd, now()));
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn load_control_accepts_only_same_origin_loopback_forwards() {
        let store = std::sync::Arc::new(
            tokio::sync::watch::channel(View {
                load_test: Some(Test::default()),
                ..View::default()
            })
            .0,
        );
        for (peer, host, origin, expected) in [
            (
                "127.0.0.1:4321",
                "127.0.0.1:19090",
                "http://127.0.0.1:19090",
                StatusCode::OK,
            ),
            (
                "[::1]:4321",
                "localhost:9090",
                "http://localhost:9090",
                StatusCode::OK,
            ),
            (
                "127.0.0.1:4321",
                "localhost:19090",
                "http://localhost:9090",
                StatusCode::FORBIDDEN,
            ),
            (
                "127.0.0.1:4321",
                "example.org:19090",
                "http://example.org:19090",
                StatusCode::FORBIDDEN,
            ),
            (
                "10.0.0.1:4321",
                "127.0.0.1:19090",
                "http://127.0.0.1:19090",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("host", host.parse().unwrap());
            headers.insert("origin", origin.parse().unwrap());
            assert_eq!(
                control(
                    State(store.clone()),
                    ConnectInfo(peer.parse().unwrap()),
                    headers,
                    Json(Command::Stop)
                )
                .await,
                expected
            );
        }
    }

    #[test]
    fn target_timeout_and_repeat_click() {
        let mut v = View {
            load_test: Some(Test::default()),
            ..View::default()
        };
        for k in ["deployments", "hpa"] {
            v.synced.insert(k.into(), true);
        }
        v.resources
            .entry("hpa".into())
            .or_default()
            .insert("app".into(), json!({"max":5,"target":70}));
        v.resources
            .entry("deployments".into())
            .or_default()
            .insert("app".into(), json!({"available":2}));
        assert_eq!(change(&mut v, Command::Start, 0), StatusCode::ACCEPTED);
        assert_eq!(change(&mut v, Command::Start, 150_000), StatusCode::OK);
        assert!(finish(&mut v, 180_000));
        assert_eq!(v.load_test.as_ref().unwrap().phase, Phase::TimedOut);
        change(&mut v, Command::Start, 190_000);
        v.resources
            .get_mut("deployments")
            .unwrap()
            .get_mut("app")
            .unwrap()["available"] = json!(5);
        assert!(finish(&mut v, 191_000));
        assert_eq!(v.load_test.as_ref().unwrap().phase, Phase::Reached);
        assert_eq!(
            change(&mut v, Command::Start, 192_000),
            StatusCode::CONFLICT
        );
    }
}
