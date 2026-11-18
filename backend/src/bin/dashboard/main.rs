mod cluster;
mod load_test;
mod metrics;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{
        Html, IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    convert::Infallible,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

const KINDS: [&str; 7] = [
    "nodes",
    "pods",
    "services",
    "slices",
    "claims",
    "deployments",
    "hpa",
];
type Store = Arc<watch::Sender<View>>;

#[derive(Clone, Default, serde::Serialize)]
struct View {
    resources: BTreeMap<String, BTreeMap<String, Value>>,
    synced: BTreeMap<String, bool>,
    updated: u64,
    run: Option<Run>,
    probes: VecDeque<Probe>,
    pod_events: VecDeque<cluster::PodEvent>,
    uptime_url: Option<String>,
    load_test: Option<load_test::Test>,
}
#[derive(Clone, serde::Serialize)]
struct Run {
    started: u64,
    requests: Vec<Row>,
}
#[derive(Clone, serde::Serialize)]
struct Row {
    id: String,
    client: Option<Completion>,
    pod: Option<String>,
    server_status: Option<u16>,
}
#[derive(Clone, Deserialize, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Completion {
    id: String,
    start: u64,
    end: u64,
    status: Option<u16>,
    timeout: bool,
}
#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    time: u64,
    ms: u32,
    ok: bool,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Run { ids: Vec<String>, started: u64 },
    Result(Completion),
    Probe(Probe),
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn apply(view: &mut View, input: Input) -> Result<(), StatusCode> {
    match input {
        Input::Run { ids, started } => {
            if ids.len() != 10
                || ids.iter().collect::<HashSet<_>>().len() != 10
                || ids
                    .iter()
                    .any(|id| id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(StatusCode::BAD_REQUEST);
            }
            if view.run.as_ref().is_some_and(|r| {
                r.started == started && r.requests.iter().map(|x| &x.id).eq(ids.iter())
            }) {
                return Ok(());
            }
            if view.run.as_ref().is_some_and(|run| {
                started <= run.started
                    || (now().saturating_sub(run.started) < 10_000
                        && run.requests.iter().any(|r| r.client.is_none()))
            }) {
                return Err(StatusCode::CONFLICT);
            }
            view.run = Some(Run {
                started,
                requests: ids
                    .into_iter()
                    .map(|id| Row {
                        id,
                        client: None,
                        pod: None,
                        server_status: None,
                    })
                    .collect(),
            });
        }
        Input::Result(c) => {
            if c.end < c.start || c.status.is_some_and(|s| !(100..=599).contains(&s)) {
                return Err(StatusCode::BAD_REQUEST);
            }
            let row = view
                .run
                .as_mut()
                .and_then(|r| r.requests.iter_mut().find(|r| r.id == c.id))
                .ok_or(StatusCode::CONFLICT)?;
            row.client = Some(c);
        }
        Input::Probe(p) => {
            if view.probes.back().is_some_and(|last| last.time >= p.time) {
                return Ok(());
            }
            view.probes.push_back(p);
            if view.probes.len() > 200 {
                view.probes.pop_front();
            }
        }
    }
    Ok(())
}
async fn ingest(
    State(store): State<Store>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> StatusCode {
    // The demo CLI writes through a second, pod-loopback-only port.
    if headers.contains_key("origin") {
        return StatusCode::FORBIDDEN;
    }
    let mut status = StatusCode::NO_CONTENT;
    let mut input = Some(input);
    store.send_if_modified(|view| match apply(view, input.take().unwrap()) {
        Ok(()) => true,
        Err(s) => {
            status = s;
            false
        }
    });
    status
}
async fn snapshot(State(s): State<Store>) -> Json<Value> {
    Json(json!(&*s.borrow()))
}
async fn ready(State(s): State<Store>) -> StatusCode {
    let view = s.borrow();
    if KINDS
        .iter()
        .all(|kind| view.synced.get(*kind) == Some(&true))
    {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
async fn events(State(s): State<Store>) -> impl IntoResponse {
    let rx = s.subscribe();
    let snapshots = stream::unfold((rx, true), |(mut rx, first)| async move {
        if !first && rx.changed().await.is_err() {
            return None;
        }
        let event = Event::default()
            .json_data(&*rx.borrow_and_update())
            .expect("serialize view");
        Some((Ok::<_, Infallible>(event), (rx, false)))
    });
    Sse::new(snapshots).keep_alive(KeepAlive::default())
}
#[tokio::main]
async fn main() {
    let url = std::env::var("UPTIME_URL")
        .ok()
        .filter(|v| v.starts_with("https://cloud.digitalocean.com/"));
    let (tx, _) = watch::channel(View {
        uptime_url: url,
        load_test: load_test::enabled().then(load_test::Test::default),
        ..View::default()
    });
    let store = Arc::new(tx);
    let kube = cluster::Kube::new().expect("Kubernetes connection");
    for kind in KINDS {
        tokio::spawn(cluster::observe(kube.clone(), store.clone(), kind));
    }
    tokio::spawn(cluster::observe_logs(kube, store.clone()));
    let mut app = Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../../../../dashboard/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [("content-type", "application/javascript")],
                    include_str!("../../../../dashboard/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [("content-type", "text/css")],
                    include_str!("../../../../dashboard/style.css"),
                )
            }),
        )
        .route("/health/ready", get(ready))
        .route("/api/snapshot", get(snapshot))
        .route("/api/events", get(events))
        .route("/api/metrics", get(metrics::ranges));
    if load_test::enabled() {
        app = app.route(
            "/api/load-test",
            get(load_test::lease).post(load_test::control),
        );
        tokio::spawn(load_test::monitor(store.clone()));
    }
    let app = app.with_state(store.clone());
    let demo = Router::new()
        .route("/demo", post(ingest))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(store);
    let read = tokio::net::TcpListener::bind("0.0.0.0:9090").await.unwrap();
    let write = tokio::net::TcpListener::bind("127.0.0.1:9091")
        .await
        .unwrap();
    tokio::try_join!(
        axum::serve(
            read,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>()
        )
        .into_future(),
        axum::serve(write, demo).into_future()
    )
    .unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn readiness_requires_all_cluster_feeds() {
        let store = Arc::new(watch::channel(View::default()).0);
        assert_eq!(
            ready(State(store.clone())).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        store.send_modify(|v| v.synced.extend(KINDS.map(|k| (k.to_owned(), true))));
        assert_eq!(ready(State(store.clone())).await, StatusCode::OK);
        store.send_modify(|v| {
            v.synced.insert("pods".into(), false);
        });
        assert_eq!(ready(State(store)).await, StatusCode::SERVICE_UNAVAILABLE);
    }
    #[test]
    fn duplicate_run_preserves_completion_and_old_results_are_rejected() {
        let ids: Vec<_> = (0..10).map(|i| format!("{i:032x}")).collect();
        let mut view = View::default();
        let run = || Input::Run {
            ids: ids.clone(),
            started: 1,
        };
        apply(&mut view, run()).unwrap();
        let mut c = Completion {
            id: ids[0].clone(),
            start: 1,
            end: 2,
            status: Some(401),
            timeout: false,
        };
        apply(&mut view, Input::Result(c.clone())).unwrap();
        apply(&mut view, run()).unwrap();
        assert!(view.run.as_ref().unwrap().requests[0].client.as_ref() == Some(&c));
        c.id = "other".into();
        assert_eq!(
            apply(&mut view, Input::Result(c)),
            Err(StatusCode::CONFLICT)
        );
    }
}
