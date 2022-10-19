use crate::board::Board;
use crate::config::Config;
use anyhow::Result;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Json, Router, Server};
use directories::ProjectDirs;
use std::net::IpAddr;
use std::sync::Arc;

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

async fn get_api_board(Extension(board): Extension<Arc<Board>>) -> impl IntoResponse {
    match board.load().await {
        Ok(data) => Ok(Json(data)),
        Err(error) => Err((StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    }
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
        .route("/api/board", get(get_api_board))
        .layer(Extension(board));
    let ip: IpAddr = config.server_ip.parse()?;
    Server::bind(&(ip, config.server_port).into())
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
