pub mod assets;
pub mod config_api;
pub mod groups;
pub mod items;
pub mod repos;
pub mod scan;
pub mod tags;
pub mod ws;

use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Error as AnyhowError;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::fmt;

pub fn json_ok<T: Serialize>(data: T) -> HttpResponse {
    HttpResponse::Ok().json(data)
}

pub enum ApiError {
    Bad(AnyhowError),
    NotFound(AnyhowError),
    Conflict(AnyhowError),
}

impl ApiError {
    pub fn bad(msg: impl Into<String>) -> Self {
        ApiError::Bad(AnyhowError::msg(msg.into()))
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiError::NotFound(AnyhowError::msg(msg.into()))
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        ApiError::Conflict(AnyhowError::msg(msg.into()))
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Bad(e) | ApiError::NotFound(e) | ApiError::Conflict(e) => write!(f, "{e}"),
        }
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for ApiError {}

impl From<AnyhowError> for ApiError {
    fn from(e: AnyhowError) -> Self {
        ApiError::Bad(e)
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Bad(AnyhowError::new(e).context("database error"))
    }
}

impl From<r2d2::Error> for ApiError {
    fn from(e: r2d2::Error) -> Self {
        ApiError::Bad(AnyhowError::new(e).context("database pool error"))
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            ApiError::Bad(_) => actix_web::http::StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => actix_web::http::StatusCode::CONFLICT,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .json(serde_json::json!({ "error": self.to_string() }))
    }
}

/// Load a tag group's rules. `includes = None` means "all items".
pub fn group_rules(conn: &Connection, group_id: i64) -> anyhow::Result<(Option<Vec<i64>>, Vec<i64>)> {
    let row: Option<(Option<String>, String)> = conn
        .query_row(
            "SELECT includes, excludes FROM tag_groups WHERE id = ?1",
            [group_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        None => anyhow::bail!("group not found: {group_id}"),
        Some((inc, exc)) => {
            let includes: Option<Vec<i64>> = match inc {
                Some(s) => Some(serde_json::from_str(&s).unwrap_or_default()),
                None => None,
            };
            let excludes: Vec<i64> = serde_json::from_str(&exc).unwrap_or_default();
            Ok((includes, excludes))
        }
    }
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .service(
                web::resource("/repos")
                    .route(web::get().to(repos::list))
                    .route(web::post().to(repos::add)),
            )
            .service(
                web::resource("/repos/{id}").route(web::delete().to(repos::remove)),
            )
            .service(
                web::resource("/repos/{id}/scan").route(web::post().to(scan::scan)),
            )
            .service(
                web::resource("/repos/{id}/export").route(web::get().to(repos::export)),
            )
            .service(
                web::resource("/repos/{id}/settings")
                    .route(web::get().to(repos::get_settings))
                    .route(web::put().to(repos::put_settings))
                    .route(web::delete().to(repos::reset_settings)),
            )
            .service(
                web::resource("/repos/{id}/tags")
                    .route(web::get().to(tags::list))
                    .route(web::post().to(tags::create)),
            )
            .service(
                web::resource("/repos/{id}/tags/{tag_id}")
                    .route(web::put().to(tags::update))
                    .route(web::delete().to(tags::delete)),
            )
            .service(
                web::resource("/repos/{id}/items")
                    .route(web::get().to(items::list))
                    .route(web::post().to(items::create)),
            )
            .service(
                web::resource("/repos/{id}/items/{item_id}")
                    .route(web::put().to(items::update))
                    .route(web::delete().to(items::delete)),
            )
            .service(
                web::resource("/repos/{id}/items/{item_id}/tags")
                    .route(web::post().to(items::add_tags)),
            )
            .service(
                web::resource("/repos/{id}/items/{item_id}/tags/{tag_id}")
                    .route(web::delete().to(items::remove_tag)),
            )
            .service(
                web::resource("/repos/{id}/groups")
                    .route(web::get().to(groups::list))
                    .route(web::post().to(groups::create)),
            )
            .service(
                web::resource("/repos/{id}/groups/{group_id}")
                    .route(web::put().to(groups::update))
                    .route(web::delete().to(groups::delete)),
            )
            .service(
                web::resource("/repos/{id}/groups/{group_id}/items")
                    .route(web::get().to(groups::items)),
            )
            .service(
                web::resource("/config")
                    .route(web::get().to(config_api::get))
                    .route(web::put().to(config_api::put)),
            )
            .service(web::resource("/about").route(web::get().to(config_api::about)))
            .service(
                web::resource("/infoparse/preview").route(web::post().to(config_api::infoparse_preview)),
            ),
    )
    .route("/ws", web::get().to(ws::ws))
    .route("/", web::get().to(assets::index))
    .route("/assets/{tail:.*}", web::get().to(assets::asset))
    .default_service(web::to(assets::spa));
}
