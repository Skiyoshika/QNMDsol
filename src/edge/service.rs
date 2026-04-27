use std::sync::{Arc, Mutex};

use serde::Serialize;
use tiny_http::{Header, Response, Server};

use crate::edge::config::EdgeConfig;

#[derive(Debug, Default, Serialize)]
pub struct EdgeState {
    pub connected: bool,
    pub streaming: bool,
    pub last_error: Option<String>,
}

fn json_response<T: Serialize>(value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_data(body).with_header(header)
}

pub fn run_edge_service(config: EdgeConfig) -> anyhow::Result<()> {
    let bind = format!("{}:{}", config.host, config.port);
    let server = Server::http(&bind).map_err(|e| anyhow::anyhow!("failed to bind {bind}: {e}"))?;
    let state = Arc::new(Mutex::new(EdgeState::default()));
    println!("neurostick pi edge listening on http://{bind}");

    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/").to_string();
        let method = request.method().as_str().to_string();
        match (method.as_str(), path.as_str()) {
            ("GET", "/health") => {
                request.respond(json_response(&serde_json::json!({"ok": true})))?;
            }
            ("GET", "/status") => {
                let guard = state.lock().unwrap();
                request.respond(json_response(&*guard))?;
            }
            _ => {
                request.respond(
                    Response::from_string("{\"error\":\"not found\"}")
                        .with_status_code(404)
                        .with_header(
                            Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"application/json"[..],
                            )
                            .unwrap(),
                        ),
                )?;
            }
        }
    }
    Ok(())
}
