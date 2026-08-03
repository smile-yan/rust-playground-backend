use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{compiler, cpp_compiler, sandbox};

const MAX_CODE_LENGTH: usize = 64 * 1024; // 64 KiB

/// Request body compatible with the official Rust Playground `/evaluate.json` endpoint,
/// while still accepting our legacy `{ "code": "..." }` format.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct EvaluateRequest {
    code: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    edition: Option<String>,
    #[serde(default)]
    #[serde(rename = "crateType")]
    crate_type: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    tests: Option<bool>,
    #[serde(default)]
    backtrace: Option<bool>,
}

/// C/C++ Playground 请求体。
#[derive(Debug, Deserialize)]
pub struct EvaluateCppRequest {
    code: String,
    #[serde(default = "default_cpp_language")]
    language: String,
}

fn default_cpp_language() -> String {
    "cpp".to_string()
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    success: bool,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

pub async fn evaluate(Json(payload): Json<EvaluateRequest>) -> (StatusCode, Json<RunResponse>) {
    info!(
        "Received code submission ({} bytes) channel={:?} edition={:?}",
        payload.code.len(),
        payload.channel,
        payload.edition
    );

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

pub async fn evaluate_cpp(
    Json(payload): Json<EvaluateCppRequest>,
) -> (StatusCode, Json<RunResponse>) {
    info!(
        "Received C/C++ code submission ({} bytes) language={}",
        payload.code.len(),
        payload.language
    );

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

    let language = match payload.language.as_str() {
        "c" => "c",
        "cpp" => "cpp",
        _ => "cpp",
    };

    match compile_cpp_and_run(&payload.code, language).await {
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

async fn compile_cpp_and_run(code: &str, language: &str) -> anyhow::Result<RunResponse> {
    let wasm_bytes = match cpp_compiler::compile_cpp_to_wasm(code, language).await {
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
