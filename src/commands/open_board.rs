use crate::board::{Board, BoardData, BoardIssueData};
use crate::config::Config;
use anyhow::{Error, Result};
use axum::extract::Path;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router, Server};
use directories::ProjectDirs;
use std::net::IpAddr;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

async fn get_root() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html")],
        include_str!("../../resources/web/index.html"),
    )
}

async fn get_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        include_str!("../../resources/web/index.js"),
    )
}

async fn get_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../resources/web/index.css"),
    )
}

async fn get_favicon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../resources/web/favicon.png").as_slice(),
    )
}

struct ApiError(Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", self.0)).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        ApiError(error)
    }
}

async fn get_api_board(
    Extension(board): Extension<Arc<Board>>,
) -> Result<Json<BoardData>, ApiError> {
    let data = board.load().await?;
    Ok(Json(data))
}

async fn get_api_issue(
    Extension(board): Extension<Arc<Board>>,
    Path(key): Path<String>,
) -> Result<Json<BoardIssueData>, ApiError> {
    let data = board.issue(&key).await?;
    Ok(Json(data))
}

pub async fn open_board(project_dirs: &ProjectDirs, board_name: &str) -> Result<()> {
    let config = Config::new(project_dirs)?;

    let board = Arc::new(Board::open(&config, board_name).await?);

    tracing::info!(
        "Will start local server on http://localhost:{}",
        config.server_port
    );
    let app = Router::new()
        .route("/", get(get_root))
        .route("/index.js", get(get_js))
        .route("/index.css", get(get_css))
        .route("/favicon.png", get(get_favicon))
        .route("/api/board", get(get_api_board))
        .route("/api/issue/:key", get(get_api_issue))
        .layer(Extension(board))
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
    Server::bind(&(ip, config.server_port).into())
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
