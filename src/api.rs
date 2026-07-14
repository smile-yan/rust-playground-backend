use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{compiler, sandbox};

const MAX_CODE_LENGTH: usize = 64 * 1024; // 64 KiB

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    code: String,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    success: bool,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

pub async fn run_code(Json(payload): Json<RunRequest>) -> (StatusCode, Json<RunResponse>) {
    info!("Received code submission ({} bytes)", payload.code.len());

    if payload.code.len() > MAX_CODE_LENGTH {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(RunResponse {
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(format!(
                    "Code exceeds maximum length of {} bytes",
                    MAX_CODE_LENGTH
                )),
            }),
        );
    }

    match compile_and_run(&payload.code).await {
        Ok(output) => (StatusCode::OK, Json(output)),
        Err(e) => {
            error!("Internal error: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RunResponse {
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(format!("Internal server error: {}", e)),
                }),
            )
        }
    }
}

async fn compile_and_run(code: &str) -> anyhow::Result<RunResponse> {
    let wasm_bytes = match compiler::compile_to_wasm(code).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(RunResponse {
                success: false,
                stdout: String::new(),
                stderr: e.to_string(),
                error: Some("Compilation failed".to_string()),
            });
        }
    };

    match sandbox::run_wasm(&wasm_bytes).await {
        Ok(output) => Ok(RunResponse {
            success: output.success,
            stdout: output.stdout,
            stderr: output.stderr,
            error: output.error,
        }),
        Err(e) => Ok(RunResponse {
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
            error: Some("Execution failed".to_string()),
        }),
    }
}
