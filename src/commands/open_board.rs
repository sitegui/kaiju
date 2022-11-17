mod static_files;

use crate::board::{Board, BoardData, BoardIssueData};
use crate::commands::open_board::static_files::{StaticFile, StaticSource};
use crate::config::Config;
use crate::issue_code;
use crate::issue_code::{parse_issue_markdown, prepare_api_body};
use crate::jira_api::JiraApi;
use crate::local_jira_cache::LocalJiraCache;
use anyhow::{ensure, Context, Error, Result};
use axum::extract::Path;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router, Server};
use directories::ProjectDirs;
use std::net::IpAddr;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::task;
use tower_http::cors::{AllowOrigin, CorsLayer};

struct ApiError(Error);

async fn get_root(source: Extension<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexHtml.serve(source.0)
}

async fn get_js(source: Extension<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexJs.serve(source.0)
}

async fn get_css(source: Extension<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexCss.serve(source.0)
}

async fn get_favicon(source: Extension<StaticSource>) -> impl IntoResponse {
    StaticFile::Favicon.serve(source.0)
}

async fn get_api_board(
    Extension(board): Extension<Arc<Board>>,
) -> Result<Json<BoardData>, ApiError> {
    let start = Instant::now();
    let data = board.load().await?;
    tracing::info!("Got board data in {:.1}s", start.elapsed().as_secs_f64());
    Ok(Json(data))
}

async fn get_api_issue(
    Extension(board): Extension<Arc<Board>>,
    Path(key): Path<String>,
) -> Result<Json<BoardIssueData>, ApiError> {
    let data = board.issue(key).await?;
    Ok(Json(data))
}

async fn get_new_issue_code(Extension(config): Extension<Arc<Config>>) -> Result<String, ApiError> {
    let code = issue_code::new_issue(&config)?;
    Ok(code)
}

async fn get_edit_issue_code(
    Path(key): Path<String>,
    Extension(config): Extension<Arc<Config>>,
    Extension(api): Extension<Arc<LocalJiraCache>>,
) -> Result<String, ApiError> {
    let issue = api.issue(key).await?;
    let code = issue_code::edit_issue(&config, issue.fields)?;
    Ok(code)
}

async fn post_new_issue(
    code: String,
    Extension(config): Extension<Arc<Config>>,
    Extension(api): Extension<Arc<JiraApi>>,
) -> Result<(), ApiError> {
    let info = parse_issue_markdown(&code).context("Failed to parse Markdown")?;
    let body = prepare_api_body(&config, info).context("Failed to prepare Jira API call")?;

    tracing::info!("Will request Jira API");
    let key = api.create_issue(&body).await?;
    tracing::info!("Created issue: {}/browse/{}", config.api_host, key);

    Ok(())
}

async fn post_edit_issue(
    code: String,
    Path(key): Path<String>,
    Extension(config): Extension<Arc<Config>>,
    Extension(api): Extension<Arc<JiraApi>>,
) -> Result<(), ApiError> {
    let info = parse_issue_markdown(&code).context("Failed to parse Markdown")?;
    let body = prepare_api_body(&config, info).context("Failed to prepare Jira API call")?;

    tracing::info!("Will request Jira API");
    api.edit_issue(&key, &body).await?;

    Ok(())
}

pub async fn open_board(
    project_dirs: &ProjectDirs,
    board_name: &str,
    dev_mode: bool,
) -> Result<()> {
    let config = Arc::new(Config::new(project_dirs)?);

    let static_source = if dev_mode {
        StaticSource::RunTime
    } else {
        StaticSource::CompileTime
    };
    let api = Arc::new(JiraApi::new(&config));
    let cached_api = Arc::new(LocalJiraCache::new(
        api.clone(),
        config.api_parallelism,
        config.cache.clone(),
    ));
    let board = Arc::new(Board::open(&config, cached_api.clone(), board_name).await?);

    let server_port = config.server_port;
    tracing::info!(
        "Will start local server on http://localhost:{}",
        server_port
    );
    let app = Router::new()
        .route("/", get(get_root))
        .route("/index.js", get(get_js))
        .route("/index.css", get(get_css))
        .route("/favicon.png", get(get_favicon))
        .route("/api/board", get(get_api_board))
        .route("/api/issue/:key", get(get_api_issue))
        .route("/api/new-issue-code", get(get_new_issue_code))
        .route("/api/edit-issue-code/:key", get(get_edit_issue_code))
        .route("/api/issue", post(post_new_issue))
        .route("/api/issue/:key", post(post_edit_issue))
        .layer(Extension(api))
        .layer(Extension(cached_api))
        .layer(Extension(board))
        .layer(Extension(static_source))
        .layer(Extension(config.clone()))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
                    origin
                        .to_str()
                        .map(|origin| origin.starts_with("http://localhost:"))
                        .unwrap_or(false)
                }))
                .allow_methods([Method::GET]),
        );
    let ip: IpAddr = config.server_ip.parse()?;
    let server = Server::bind(&(ip, server_port).into()).serve(app.into_make_service());

    task::spawn_blocking(move || {
        let url = format!("http://localhost:{}", server_port);
        match open_browser(&url) {
            Err(error) => tracing::warn!("Failed to open browser: {}", error),
            Ok(()) => tracing::info!("Opened default browser"),
        }
    });

    server.await?;

    Ok(())
}

fn open_browser(url: &str) -> Result<()> {
    thread::sleep(Duration::from_secs(1));

    let status = Command::new("xdg-open")
        .arg(url)
        .status()
        .context("Failed to open in browser")?;

    ensure!(status.success());

    Ok(())
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::warn!("Will answer endpoint with error: {:?}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", self.0)).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        ApiError(error)
    }
}
