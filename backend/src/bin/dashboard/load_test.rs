use super::{Store, View, now};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
struct CpuWarning {
    time: u64,
    cpu: u64,
    target: u64,
}

#[derive(Clone, Serialize)]
pub struct Test {
    phase: Phase,
    deadline: u64,
    started: u64,
    initial: u64,
    cpu_warning: Option<CpuWarning>,
    first_desired: Option<u64>,
    first_scale_ms: Option<u64>,
    ready_ms: Option<u64>,
}
impl Default for Test {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            deadline: 0,
            started: 0,
            initial: 0,
            cpu_warning: None,
            first_desired: None,
            first_scale_ms: None,
            ready_ms: None,
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

fn above_target(hpa: Option<&serde_json::Value>, time: u64) -> Option<CpuWarning> {
    let hpa = hpa?;
    let cpu = hpa["cpu"].as_u64()?;
    let target = hpa["target"].as_u64()?;
    (target > 0 && cpu > target).then_some(CpuWarning { time, cpu, target })
}

pub(super) fn finish(view: &mut View, time: u64) -> bool {
    if !view
        .load_test
        .as_ref()
        .is_some_and(|t| t.phase == Phase::Running)
    {
        return false;
    }
    let hpa = view.resources.get("hpa").and_then(|group| group.get("app"));
    let desired = hpa.and_then(|h| h["desired"].as_u64());
    let synced = observed(view);
    let test = view.load_test.as_mut().unwrap();
    let mut warning = false;
    if synced && test.cpu_warning.is_none() {
        test.cpu_warning = above_target(hpa, time);
        warning = test.cpu_warning.is_some();
    }
    let first =
        synced && test.first_scale_ms.is_none() && desired.is_some_and(|n| n > test.initial);
    if first {
        test.first_scale_ms = Some(time.saturating_sub(test.started));
        test.first_desired = desired;
    }
    let phase = if !synced {
        Phase::Interrupted
    } else if available(view) >= Some(5) {
        Phase::Reached
    } else if time >= view.load_test.as_ref().unwrap().deadline {
        Phase::TimedOut
    } else {
        return first || warning;
    };
    let test = view.load_test.as_mut().unwrap();
    if phase == Phase::Reached {
        test.ready_ms = Some(time.saturating_sub(test.started));
    }
    test.phase = phase;
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
    let hpa = view.resources.get("hpa").and_then(|group| group.get("app"));
    let initial = hpa
        .and_then(|h| h["desired"].as_u64())
        .or_else(|| available(view))
        .unwrap_or(0);
    let can_start = observed(view) && available(view) < Some(5);
    let Some(test) = view.load_test.as_mut() else {
        return StatusCode::NOT_FOUND;
    };
    match cmd {
        Command::Start if test.phase == Phase::Running => StatusCode::OK,
        Command::Start if !can_start => StatusCode::CONFLICT,
        Command::Start => {
            test.started = time;
            test.initial = initial;
            test.cpu_warning = above_target(hpa, time);
            test.first_desired = None;
            test.first_scale_ms = None;
            test.ready_ms = None;
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
pub async fn session_guard(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let headers = req.headers();
    if headers.get("x-grace-control").and_then(|h| h.to_str().ok()) != Some("session")
        || headers.contains_key("origin")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(cookie) = headers.get("cookie") else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match client
        .get("http://app/api/devops/access")
        .header("cookie", cookie)
        .send()
        .await
    {
        Ok(r) if r.status() == StatusCode::NO_CONTENT => {}
        Ok(r) if r.status() == StatusCode::UNAUTHORIZED => {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Ok(_) => return StatusCode::FORBIDDEN.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
    next.run(req).await
}
pub async fn operator_control(State(s): State<Store>, Json(cmd): Json<Command>) -> StatusCode {
    let mut status = StatusCode::OK;
    s.send_modify(|v| status = change(v, cmd, now()));
    status
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn warning_is_observed_once_and_survives_the_scaling_decision() {
        let mut v = View {
            load_test: Some(Test::default()),
            ..View::default()
        };
        for kind in ["hpa", "deployments"] {
            v.synced.insert(kind.into(), true);
        }
        let status = |cpu, desired| json!({"cpu":cpu, "target":70, "desired":desired, "max":5});
        v.resources
            .entry("hpa".into())
            .or_default()
            .insert("app".into(), status(70, 2));
        v.resources
            .entry("deployments".into())
            .or_default()
            .insert("app".into(), json!({"available":2}));
        assert_eq!(change(&mut v, Command::Start, 100), StatusCode::ACCEPTED);
        assert!(!finish(&mut v, 101));
        v.resources
            .get_mut("hpa")
            .unwrap()
            .insert("app".into(), status(72, 2));
        assert!(finish(&mut v, 110));
        assert!(v.load_test.as_ref().unwrap().first_desired.is_none());
        v.resources
            .get_mut("hpa")
            .unwrap()
            .insert("app".into(), status(210, 5));
        assert!(finish(&mut v, 120));
        let test = v.load_test.as_ref().unwrap();
        assert_eq!(
            test.cpu_warning.as_ref().map(|w| (w.time, w.cpu, w.target)),
            Some((110, 72, 70))
        );
        assert_eq!(
            (test.first_desired, test.first_scale_ms),
            (Some(5), Some(20))
        );
        assert!(!finish(&mut v, 121));
        change(&mut v, Command::Stop, 122);
        v.resources
            .get_mut("hpa")
            .unwrap()
            .insert("app".into(), status(1, 2));
        change(&mut v, Command::Start, 130);
        assert!(v.load_test.as_ref().unwrap().cpu_warning.is_none());
        assert!(v.load_test.as_ref().unwrap().first_desired.is_none());
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
