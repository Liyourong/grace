use axum::{
    Router,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
    routing::get,
};
use rand_core::{OsRng, RngCore};
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

tokio::task_local! {
    static STEPS: RefCell<Vec<serde_json::Value>>;
}

pub async fn step<T>(
    name: &'static str,
    work: impl Future<Output = crate::common::Result<T>>,
) -> crate::common::Result<T> {
    let start = Instant::now();
    let result = work.await;
    let _ = STEPS.try_with(|steps| {
        steps.borrow_mut().push(json!({
            "name": name, "us": start.elapsed().as_micros() as u64, "ok": result.is_ok()
        }))
    });
    result
}

struct Answers {
    total: u64,
    failed: u64,
    seconds: f64,
}
static ANSWERS: Mutex<Answers> = Mutex::new(Answers {
    total: 0,
    failed: 0,
    seconds: 0.0,
});

pub async fn record(req: Request, next: Next) -> Response {
    let route = req.extensions().get::<MatchedPath>().cloned();
    let route = route.as_ref().map(MatchedPath::as_str);
    let answer =
        route == Some("/api/papers/{paper_id}/answers/{question_id}") && req.method() == "PUT";
    let demo = if route == Some("/api/auth/login") && req.method() == "POST" {
        req.headers()
            .get("x-grace-demo-id")
            .and_then(|v| v.to_str().ok())
            .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned)
    } else {
        None
    };
    let method = match req.method().as_str() {
        s @ ("GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS") => s.to_owned(),
        _ => "OTHER".to_owned(),
    };
    let mut id = [0; 16];
    OsRng.fill_bytes(&mut id);
    let id = hex::encode(id);
    let start = Instant::now();
    let detailed = answer || (route == Some("/api/auth/login") && method == "POST");
    let (mut response, steps) = if detailed {
        STEPS
            .scope(RefCell::new(Vec::new()), async {
                let response = next.run(req).await;
                (response, STEPS.with(|s| s.take()))
            })
            .await
    } else {
        (next.run(req).await, Vec::new())
    };
    let elapsed = start.elapsed();
    let finished = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    response
        .headers_mut()
        .insert("x-trace-id", id.parse().unwrap());
    let rejected_at = if detailed && steps.iter().all(|s| s["ok"] == true) {
        response
            .extensions()
            .get::<crate::common::RejectionStage>()
            .map(|s| s.0)
    } else {
        None
    };
    println!(
        "{}",
        json!({"event":"grace.request","rejected_at":rejected_at,"id":id,"time":finished,"us":elapsed.as_micros() as u64,
        "method":method,"route":route,"status":response.status().as_u16(),"steps":steps})
    );

    if answer {
        let mut a = ANSWERS.lock().unwrap();
        a.total += 1;
        a.failed += u64::from(!response.status().is_success());
        a.seconds += elapsed.as_secs_f64();
    }
    if let Some(id) = demo {
        println!(
            "{}",
            serde_json::json!({"event":"grace.demo.login","id":id,"status":response.status().as_u16()})
        );
    }
    response
}

pub async fn start_metrics() {
    let Ok(port) = std::env::var("METRICS_PORT") else {
        return;
    };
    let socket = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("metrics listener");
    let metrics = Router::new().route("/metrics", get(|| async {
        let a = ANSWERS.lock().unwrap();
        format!("grace_answers_total {}\ngrace_answer_failures_total {}\ngrace_answer_seconds_total {}\n", a.total, a.failed, a.seconds)
    }));
    tokio::spawn(async move {
        axum::serve(socket, metrics).await.expect("serve metrics");
    });
}
