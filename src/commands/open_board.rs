mod static_files;

use crate::board::{Board, BoardData, BoardIssueData};
use crate::commands::open_board::static_files::{StaticFile, StaticSource};
use crate::config::Config;
use crate::issue_code;
use crate::issue_code::{parse_issue_markdown, prepare_api_body};
use crate::jira_api::JiraApi;
use crate::local_jira_cache::LocalJiraCache;
use anyhow::{ensure, Context, Error, Result};
use axum::extract::FromRef;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, Server};
use directories::ProjectDirs;
use itertools::Itertools;
use serde::Deserialize;
use std::net::IpAddr;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::task;
use tower_http::cors::{AllowOrigin, CorsLayer};

struct ApiError(Error);

#[derive(Debug, Clone, FromRef)]
struct ApiState {
    static_source: StaticSource,
    board: Arc<Board>,
    config: Arc<Config>,
    cached_api: Arc<LocalJiraCache>,
    api: Arc<JiraApi>,
}

async fn get_root(source: State<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexHtml.serve(source.0)
}

async fn get_js(source: State<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexJs.serve(source.0)
}

async fn get_css(source: State<StaticSource>) -> impl IntoResponse {
    StaticFile::IndexCss.serve(source.0)
}

async fn get_favicon(source: State<StaticSource>) -> impl IntoResponse {
    StaticFile::Favicon.serve(source.0)
}

async fn get_api_board(State(board): State<Arc<Board>>) -> Result<Json<BoardData>, ApiError> {
    let start = Instant::now();
    let data = board.load().await?;
    tracing::info!("Got board data in {:.1}s", start.elapsed().as_secs_f64());
    Ok(Json(data))
}

async fn get_api_issue(
    State(board): State<Arc<Board>>,
    Path(key): Path<String>,
) -> Result<Json<BoardIssueData>, ApiError> {
    let data = board.issue(key).await?;
    Ok(Json(data))
}

#[derive(Debug, Deserialize)]
struct GetNewIssueCodeQuery {
    status_ids: String,
}

async fn get_new_issue_code(
    State(config): State<Arc<Config>>,
    Query(query): Query<GetNewIssueCodeQuery>,
) -> Result<String, ApiError> {
    let status_ids = query
        .status_ids
        .split(',')
        .map(ToString::to_string)
        .collect_vec();
    let code = issue_code::new_issue(&config, Some(&status_ids))?;
    Ok(code)
}

async fn get_edit_issue_code(
    Path(key): Path<String>,
    State(config): State<Arc<Config>>,
    State(api): State<Arc<LocalJiraCache>>,
) -> Result<String, ApiError> {
    let issue = api.issue(key).await?;
    let code = issue_code::edit_issue(&config, issue.fields)?;
    Ok(code)
}

async fn post_new_issue(
    State(config): State<Arc<Config>>,
    State(api): State<Arc<JiraApi>>,
    State(cached_api): State<Arc<LocalJiraCache>>,
    code: String,
) -> Result<(), ApiError> {
    let info = parse_issue_markdown(&code).context("Failed to parse Markdown")?;
    let body = prepare_api_body(&config, info).context("Failed to prepare Jira API call")?;

    tracing::info!("Will request Jira API");
    let key = api.create_issue(&body).await?;
    tracing::info!("Created issue: {}/browse/{}", config.api_host, key);

    cached_api.clear();

    Ok(())
}

async fn post_edit_issue(
    Path(key): Path<String>,
    State(config): State<Arc<Config>>,
    State(api): State<Arc<JiraApi>>,
    State(cached_api): State<Arc<LocalJiraCache>>,
    code: String,
) -> Result<(), ApiError> {
    let info = parse_issue_markdown(&code).context("Failed to parse Markdown")?;

    if let Some(transition_name) = &info.transition {
        let transition = config
            .transitions
            .iter()
            .find(|transition| &transition.name == transition_name)
            .with_context(|| format!("Transition {} is not known", transition_name))?;

        api.transition_issue(&key, &transition.id).await?;
    }

    let body = prepare_api_body(&config, info).context("Failed to prepare Jira API call")?;
    tracing::info!("Will request Jira API");
    api.edit_issue(&key, &body).await?;

    cached_api.clear();

    Ok(())
}

pub async fn open_board(
    project_dirs: &ProjectDirs,
    board_name: &str,
    no_browser: bool,
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
    let ip: IpAddr = config.server_ip.parse()?;
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
        .with_state(ApiState {
            api,
            cached_api,
            board,
            static_source,
            config,
        })
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
    let server = Server::bind(&(ip, server_port).into()).serve(app.into_make_service());

    if !no_browser {
        task::spawn_blocking(move || {
            let url = format!("http://localhost:{}", server_port);
            match open_browser(&url) {
                Err(error) => tracing::warn!("Failed to open browser: {}", error),
                Ok(()) => tracing::info!("Opened default browser"),
            }
        });
    }

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
