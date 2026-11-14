//! Start-up: config, database pool, migrations, and the three modules' routers behind the shared guards.
//!
//! Each module owns its routes, its rules (`rules.rs`, pure functions) and its tables (`store.rs`, SQL):
//!   auth     — users, sessions
//!   papers   — papers, questions
//!   practice — answers, paper_progress

mod auth;
mod common;
mod papers;
mod practice;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{Method, StatusCode, header::ORIGIN},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::get,
};
use deadpool_postgres::{Config, Pool, PoolConfig, Runtime, Timeouts};
use tokio_postgres::NoTls;
use tower_http::services::ServeDir;

use crate::common::{AppError, AppState};

const MAX_BODY_BYTES: usize = 1_048_576;

/// New migrations go at the end; never edit one that has shipped.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    ("0002_free_usernames", include_str!("../migrations/0002_free_usernames.sql")),
    ("0003_default_paper", include_str!("../migrations/0003_default_paper.sql")),
];

#[tokio::main]
async fn main() {
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let state = AppState {
        pool: make_pool(&std::env::var("DATABASE_URL").expect("DATABASE_URL must be set")),
        cookie_secure: env("COOKIE_SECURE", "false") == "true",
        origins: Arc::new(
            env("APP_ORIGINS", "http://localhost:8080")
                .split(',')
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        ),
    };
    migrate(&state.pool).await;

    let api = Router::new()
        .merge(auth::router())
        .merge(papers::router())
        .merge(practice::router())
        .layer(from_fn_with_state(state.clone(), require_origin))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES));

    let app = Router::new()
        .merge(api)
        .route("/health/live", get(|| async { StatusCode::OK }))
        .route("/health/ready", get(ready))
        .fallback_service(ServeDir::new(env("STATIC_DIR", "../frontend")))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", env("PORT", "8080"));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    eprintln!("grace listening on {addr}");
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await.expect("server");
}

fn make_pool(url: &str) -> Pool {
    let mut cfg = Config::new();
    cfg.url = Some(url.to_string());
    let t = Some(Duration::from_secs(5));
    // 10 per pod: 5 HPA pods + 2 rollout surge = 70, under Postgres's default max_connections of 100.
    cfg.pool = Some(PoolConfig { timeouts: Timeouts { wait: t, create: t, recycle: t }, ..PoolConfig::new(10) });
    cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("pool config")
}

/// Waits up to 30 s for the database, then applies each migration once.
async fn migrate(pool: &Pool) {
    for attempt in 1..=30 {
        match try_migrate(pool).await {
            Ok(()) => return,
            Err(e) if attempt < 30 => {
                eprintln!("database not ready ({e}); retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => panic!("migrations failed: {e}"),
        }
    }
}

/// An advisory lock keeps several pods starting together from migrating twice.
async fn try_migrate(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
    let mut db = pool.get().await?;
    let tx = db.transaction().await?;
    tx.batch_execute(
        "SELECT pg_advisory_xact_lock(727372);
         CREATE TABLE IF NOT EXISTS schema_migrations (
             version TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())",
    )
    .await?;
    for (version, sql) in MIGRATIONS {
        if tx.query_opt("SELECT 1 FROM schema_migrations WHERE version = $1", &[version]).await?.is_none() {
            tx.batch_execute(sql).await?;
            tx.execute("INSERT INTO schema_migrations (version) VALUES ($1)", &[version]).await?;
            eprintln!("applied migration {version}");
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Cookie sessions need a same-origin check on every state-changing request (CSRF).
async fn require_origin(State(s): State<AppState>, req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        let origin = req.headers().get(ORIGIN).and_then(|v| v.to_str().ok());
        if !origin.is_some_and(|o| s.origins.iter().any(|a| a == o)) {
            return AppError::new(StatusCode::FORBIDDEN, "origin_not_allowed", "请求来源不被允许").into_response();
        }
    }
    next.run(req).await
}

async fn ready(State(s): State<AppState>) -> StatusCode {
    let check = async { s.pool.get().await.ok()?.query_one("SELECT 1", &[]).await.ok() };
    match tokio::time::timeout(Duration::from_secs(2), check).await {
        Ok(Some(_)) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn shutdown() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.ok() };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal").recv().await
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<Option<()>>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
