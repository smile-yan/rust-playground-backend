use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, error, info};

const COMPILE_TIMEOUT_SECONDS: u64 = 120;

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

pub async fn compile_cpp_to_wasm(code: &str, language: &str) -> anyhow::Result<Vec<u8>> {
    let temp_dir = tempfile::tempdir()?;
    let source_ext = match language {
        "c" => "c",
        "cpp" => "cpp",
        _ => "cpp",
    };
    let source_path = temp_dir.path().join(format!("main.{}", source_ext));
    let output_path = temp_dir.path().join("main.wasm");

    fs::write(&source_path, code).await?;
    debug!("Wrote C/C++ source code to {:?}", source_path);

    let zig_subcmd = match language {
        "c" => "cc",
        _ => "c++",
    };

    info!(
        "Compiling C/C++ with zig {}, language: {}",
        zig_subcmd, language
    );

    let mut cmd = Command::new("zig");
    cmd.arg(zig_subcmd)
        .arg("-target")
        .arg("wasm32-wasi-musl")
        .arg("-o")
        .arg(&output_path);

    // Zig 的 libc++ 在 wasm32-wasi-musl 目标下默认不包含线程支持，
    // 而 <iostream> 等头文件会间接引入线程相关声明，需显式禁用。
    if language == "cpp" {
        cmd.arg("-D_LIBCPP_HAS_NO_THREADS");
    }

    let mut child = cmd
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

    let mut stderr = child.stderr.take().expect("stderr should be captured");
    let mut stderr_buf = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut stderr_buf).await?;
    let stderr_text = String::from_utf8_lossy(&stderr_buf).to_string();

    if !exit_status.success() {
        error!("zig {} failed with status: {:?}", zig_subcmd, exit_status);
        return Err(CompileError {
            message: stderr_text,
        }
        .into());
    }

    if !stderr_text.is_empty() {
        info!("zig {} emitted warnings: {}", zig_subcmd, stderr_text);
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
