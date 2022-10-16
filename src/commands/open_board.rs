use crate::board::Board;
use crate::config::Config;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use directories::ProjectDirs;
use std::error::Error as StdError;
use std::sync::Arc;

#[get("/")]
async fn get_root() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(include_str!("../../resources/web/index.html"))
}

#[get("/index.js")]
async fn get_js() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/javascript")
        .body(include_str!("../../resources/web/index.js"))
}

#[get("/index.css")]
async fn get_css() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/css")
        .body(include_str!("../../resources/web/index.css"))
}

#[get("/api/board")]
async fn get_api_board(board: web::Data<Arc<Board>>) -> Result<impl Responder, Box<dyn StdError>> {
    let data = board.load().await?;

    Ok(web::Json(data))
}

pub async fn open_board(project_dirs: &ProjectDirs, board_name: &str) -> Result<()> {
    let config = Config::new(project_dirs)?;

    let board = Arc::new(Board::open(&config, board_name).await?);

    tracing::info!(
        "Will start local server on http://localhost:{}",
        config.server_port
    );
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(board.clone()))
            .service(get_api_board)
            .service(get_root)
    })
    .bind((config.server_ip.as_str(), config.server_port))?
    .run()
    .await?;

    Ok(())
}
