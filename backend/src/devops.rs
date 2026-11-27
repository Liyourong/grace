use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result},
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use deadpool_postgres::{Object, Pool};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{env, time::Duration};

pub fn environment() -> String {
    env::var("GRACE_ENVIRONMENT").unwrap_or_else(|_| "Compose".into())
}

pub async fn allowed(db: &Object, id: i64) -> Result<bool> {
    Ok(db
        .query_opt("SELECT 1 FROM devops_operator WHERE user_id=$1", &[&id])
        .await?
        .is_some())
}
async fn authorize(state: &AppState, user: &CurrentUser) -> Result<Object> {
    let db = state.pool.get().await?;
    if !allowed(&db, user.id).await? {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "devops_required",
            "Test account access required",
        ));
    }
    Ok(db)
}

pub async fn prune(pool: Pool) {
    loop {
        if let Ok(db) = pool.get().await {
            if let Err(e) = db
                .execute(
                    "DELETE FROM operation_log WHERE occurred_at < now() - interval '7 days'",
                    &[],
                )
                .await
            {
                eprintln!("operation log retention: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

pub fn pages() -> Router<AppState> {
    Router::new()
        .route("/devops/", get(page))
        .route("/devops/{file}", get(asset))
        .layer(axum::middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert("cache-control", "private, no-store".parse().unwrap());
            r
        }))
}
async fn page(State(s): State<AppState>, user: Result<CurrentUser>) -> Result<Response> {
    let user = match user {
        Ok(user) => user,
        Err(e) => {
            let response = e.into_response();
            return Ok(if response.status() == StatusCode::UNAUTHORIZED {
                axum::response::Redirect::to("/?devops=1").into_response()
            } else { response });
        }
    };
    drop(authorize(&s, &user).await?);
    let page = include_str!("../../dashboard/index.html").replace("<html lang=\"en\">",
        &format!("<html lang=\"en\" data-environment=\"{}\">", match environment().as_str() {
            "DOKS" => "DOKS", "Minikube" => "Minikube", _ => "Compose"
        }));
    Ok(axum::response::Html(page).into_response())
}
async fn asset(State(s): State<AppState>, user: CurrentUser, axum::extract::Path(file): axum::extract::Path<String>) -> Result<Response> {
    drop(authorize(&s, &user).await?);
    let (kind, body) = match file.as_str() {
        "app.js" => ("text/javascript", include_str!("../../dashboard/app.js")),
        "logs.js" => ("text/javascript", include_str!("../../dashboard/logs.js")),
        "style.css" => ("text/css", include_str!("../../dashboard/style.css")),
        _ => return Err(AppError::not_found()),
    };
    Ok(([("content-type", kind)], body).into_response())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/devops/access", get(access))
        .route("/api/devops/logs", get(logs))
        .route("/api/devops/cluster/{resource}", get(cluster))
        .route("/api/devops/load/{command}", post(control))
        .route("/api/activity", post(activity))
}
async fn access(State(s): State<AppState>, u: CurrentUser) -> Result<StatusCode> {
    drop(authorize(&s, &u).await?);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Cursor {
    before: Option<i64>,
    #[serde(default)] pod: String,
    #[serde(default)] status: String,
    #[serde(default)] trace: String,
}
async fn logs(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Value>> {
    let db = authorize(&s, &u).await?;
    let rows = db
        .query(
            "SELECT seq, entry FROM operation_log WHERE occurred_at >= now()-interval '7 days'
        AND ($1::bigint IS NULL OR seq < $1)
        AND ($2 = '' OR entry->>'pod' = $2)
        AND starts_with(entry->>'status', $3) AND starts_with(request_id,$4) ORDER BY seq DESC LIMIT 51",
            &[&cursor.before,&cursor.pod,&cursor.status,&cursor.trace.to_ascii_lowercase()],
        )
        .await?;
    let next = if rows.len() > 50 {
        Some(rows[49].get::<_, i64>(0))
    } else {
        None
    };
    let items: Vec<Value> = rows.iter().take(50).map(|r| r.get(1)).collect();
    let pods: Vec<String> = db.query("SELECT DISTINCT entry->>'pod' AS pod FROM operation_log WHERE occurred_at >= now()-interval '7 days' AND entry ? 'pod' ORDER BY pod", &[]).await?.iter().map(|r|r.get(0)).collect();
    Ok(Json(
        json!({"pods":pods,"items":items,"before":next,"environment":environment()}),
    ))
}

// Accepted names describe actions, never learner input, DOM text or a URL.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    OpenOverview,
    OpenLibrary,
    OpenPaper,
    OpenUpload,
    OpenGenerator,
    OpenDevops,
    CopyCompose,
    CopyMinikube,
    CopyDoks,
    ViewLogs,
    ViewTopology,
    ViewMetrics,
    ViewReadiness,
    FilterWorker,
    ViewTrace,
    SelectEnvironment,
    ToggleFolder,
    ReviewAnswers,
    ChangeTheme,
    SelectQuestionTypes,
    SelectUploadFile,
    OpenLibraryDialog,
    CloseLibraryDialog,
}
#[derive(Deserialize)]
struct Activity {
    action: Action,
}
#[derive(Clone)]
pub struct ActionName(pub &'static str);
async fn activity(_: CurrentUser, Body(input): Body<Activity>) -> Response {
    use Action::*;
    let name = match input.action {
        OpenOverview => "Open overview",
        OpenLibrary => "Open library",
        OpenPaper => "Open paper",
        OpenUpload => "Open upload",
        OpenGenerator => "Open generator",
        OpenDevops => "Open DevOps",
        CopyCompose => "Copy Compose commands",
        CopyMinikube => "Copy Minikube commands",
        CopyDoks => "Copy DOKS commands",
        ViewTopology => "View topology",
        ViewMetrics => "View metrics",
        ViewReadiness => "View readiness",
        FilterWorker => "Filter worker",
        ViewLogs => "View log history",
        ViewTrace => "Inspect trace",
        SelectEnvironment => "Select command environment",
        ToggleFolder => "Toggle folder",
        ReviewAnswers => "Review answers",
        ChangeTheme => "Change theme",
        SelectQuestionTypes => "Select question types",
        SelectUploadFile => "Choose upload file",
        OpenLibraryDialog => "Open library dialog",
        CloseLibraryDialog => "Close library dialog",
    };
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.extensions_mut().insert(ActionName(name));
    r
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "cluster_unavailable",
        "Cluster monitor unavailable",
    )
}
fn client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .expect("cluster client")
        })
        .clone()
}
async fn cluster(State(s): State<AppState>, u: CurrentUser, headers: HeaderMap,
    axum::extract::Path(resource): axum::extract::Path<String>) -> Result<Response> {
    drop(authorize(&s, &u).await?);
    if !matches!(resource.as_str(), "events" | "metrics" | "snapshot") { return Err(AppError::not_found()); }
    if environment() == "Compose" { return Err(unavailable()); }
    let cookie = headers.get("cookie").ok_or_else(AppError::unauthorized)?;
    let upstream = client().get(format!("http://grace-dashboard:9090/api/{resource}"))
        .header("cookie", cookie).header("x-grace-control", "session")
        .timeout(Duration::from_secs(if resource == "events" { 60 } else { 12 }))
        .send().await.map_err(|_| unavailable())?;
    if !upstream.status().is_success() { return Ok(upstream.status().into_response()); }
    let kind = if resource == "events" { "text/event-stream" } else { "application/json" };
    let chunks = futures_util::stream::try_unfold(upstream, |mut r| async move {
        Ok::<_, reqwest::Error>(r.chunk().await?.map(|b| (b,r)))
    });
    Ok(([("content-type",kind),("cache-control","private, no-store")],axum::body::Body::from_stream(chunks)).into_response())
}
async fn control(
    State(s): State<AppState>,
    u: CurrentUser,
    axum::extract::Path(cmd): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    drop(authorize(&s, &u).await?);
    let name = match cmd.as_str() {
        "start" => "Start load test",
        "stop" => "Stop load test",
        _ => return Err(AppError::not_found()),
    };
    let cookie = headers.get("cookie").ok_or_else(AppError::unauthorized)?;
    let r = client()
        .post("http://grace-dashboard:9090/internal/operator-load")
        .header("cookie", cookie)
        .header("x-grace-control", "session")
        .json(&cmd)
        .send()
        .await
        .map_err(|_| unavailable())?;
    let mut out = r.status().into_response();
    out.extensions_mut().insert(ActionName(name));
    Ok(out)
}
