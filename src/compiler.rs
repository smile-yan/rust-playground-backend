use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, error, info};

const COMPILE_TIMEOUT_SECONDS: u64 = 30;

pub struct CompileError {
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::fmt::Debug for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CompileError: {}", self.message)
    }
}

impl std::error::Error for CompileError {}

pub async fn compile_to_wasm(code: &str) -> anyhow::Result<Vec<u8>> {
    let temp_dir = tempfile::tempdir()?;
    let source_path = temp_dir.path().join("main.rs");
    let output_path = temp_dir.path().join("main.wasm");

    fs::write(&source_path, code).await?;
    debug!("Wrote source code to {:?}", source_path);

    let target = detect_wasi_target();
    info!("Compiling with target: {}", target);

    let mut child = Command::new("rustc")
        .arg(format!("--target={}", target))
        .arg("-C")
        .arg("opt-level=2")
        .arg("-o")
        .arg(&output_path)
        .arg(&source_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(COMPILE_TIMEOUT_SECONDS),
        child.wait(),
    )
    .await;

    let exit_status = match result {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => {
            let _ = child.start_kill();
            return Err(CompileError {
                message: "Compilation timed out".to_string(),
            }
            .into());
        }
    };

    let mut stderr = child
        .stderr
        .take()
        .expect("stderr should be captured");
    let mut stderr_buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut stderr_buf).await?;
    let stderr_text = String::from_utf8_lossy(&stderr_buf).to_string();

    if !exit_status.success() {
        error!("rustc failed with status: {:?}", exit_status);
        return Err(CompileError {
            message: stderr_text,
        }
        .into());
    }

    if !stderr_text.is_empty() {
        info!("rustc emitted warnings: {}", stderr_text);
    }

    let wasm_bytes = fs::read(&output_path).await?;
    if wasm_bytes.is_empty() {
        return Err(CompileError {
            message: "Compiler produced an empty WASM file".to_string(),
        }
        .into());
    }

    debug!("Compiled WASM size: {} bytes", wasm_bytes.len());
    Ok(wasm_bytes)
}

fn detect_wasi_target() -> &'static str {
    // Newer Rust toolchains renamed wasm32-wasi to wasm32-wasip1.
    // Prefer the modern name and fall back to the legacy name.
    if is_target_installed("wasm32-wasip1") {
        "wasm32-wasip1"
    } else {
        "wasm32-wasi"
    }
}

fn is_target_installed(target: &str) -> bool {
    match std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) => {
            let installed = String::from_utf8_lossy(&output.stdout);
            installed.lines().any(|line| line.trim() == target)
        }
        Err(_) => false,
    }
}
