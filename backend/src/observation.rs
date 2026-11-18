use axum::{
    Router,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
    routing::get,
};
use std::{sync::Mutex, time::Instant};

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
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str);
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
    let start = Instant::now();
    let response = next.run(req).await;
    if answer {
        let mut a = ANSWERS.lock().unwrap();
        a.total += 1;
        a.failed += u64::from(!response.status().is_success());
        a.seconds += start.elapsed().as_secs_f64();
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
