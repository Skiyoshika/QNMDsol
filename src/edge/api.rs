// HTTP route handlers live alongside the service for now. This module is a
// placeholder for shared request/response shapes used by the edge API.

use serde::Serialize;

#[derive(Serialize)]
pub struct Ok {
    pub ok: bool,
}

impl Ok {
    pub fn yes() -> Self {
        Self { ok: true }
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: String,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: error.into(),
        }
    }
}
