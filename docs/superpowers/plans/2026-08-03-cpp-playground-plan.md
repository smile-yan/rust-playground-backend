# C/C++ Playground 接口实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `rust-playground` 新增 `/evaluate-cpp.json` 和 `/api/run-cpp` 接口，支持用户提交 C 或 C++ 源码，使用 Zig 编译为 `wasm32-wasip1`，并在现有 wasmtime 沙箱中执行。

**Architecture:** 复用现有 `sandbox::run_wasm` 执行 WASM；新增 `cpp_compiler` 模块负责将 C/C++ 源码编译为 WASM；在 `api.rs` 中新增针对 C/C++ 的请求解析与编排逻辑；在 `main.rs` 中注册新路由。编译与执行错误均复用现有响应格式。

**Tech Stack:** Rust 2021, axum 0.8, tokio 1.43, wasmtime 32, wasmtime-wasi 32, Zig, tempfile, anyhow, serde, tracing

## Global Constraints

- `language` 字段默认值为 `"cpp"`。
- 代码长度上限 64 KiB（复用 `MAX_CODE_LENGTH`）。
- 编译超时 30 秒。
- 执行内存上限 256 MB，执行超时 5 秒（复用 `sandbox.rs` 限制）。
- 不启用文件系统、网络、环境变量或子进程访问。
- 文档和注释使用中文。

---

## File Structure

| 文件 | 职责 |
|------|------|
| `src/cpp_compiler.rs` | 将 C/C++ 源码写入临时文件，调用 `zig cc -target wasm32-wasip1` 编译为 WASM，30 秒超时，返回 WASM 字节或编译错误。 |
| `src/api.rs` | 新增 `EvaluateCppRequest` 结构体与 `evaluate_cpp` 处理函数，校验长度，编排 `cpp_compiler` 与 `sandbox`，构造 `RunResponse`。 |
| `src/main.rs` | 注册 `/evaluate-cpp.json` 与 `/api/run-cpp` 路由。 |
| `README.md` | 新增环境依赖章节，覆盖 macOS 与 Linux 的 Rust + Zig 安装。 |
| `AGENTS.md` | 更新接口列表、技术栈、安全限制说明。 |

---

### Task 1: 新增 C/C++ 编译模块

**Files:**
- Create: `src/cpp_compiler.rs`

**Interfaces:**
- Produces: `pub async fn compile_cpp_to_wasm(code: &str, language: &str) -> anyhow::Result<Vec<u8>>`
- Produces: `pub struct CompileError { pub message: String }`（与 `compiler.rs` 同结构，但独立定义避免模块耦合）

- [ ] **Step 1: 创建 `src/cpp_compiler.rs` 文件**

```rust
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

    info!("Compiling C/C++ with zig cc, language: {}", language);

    let mut child = Command::new("zig")
        .arg("cc")
        .arg("-target")
        .arg("wasm32-wasip1")
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
        error!("zig cc failed with status: {:?}", exit_status);
        return Err(CompileError {
            message: stderr_text,
        }
        .into());
    }

    if !stderr_text.is_empty() {
        info!("zig cc emitted warnings: {}", stderr_text);
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
```

- [ ] **Step 2: 在 `src/main.rs` 中声明新模块**

Modify: `src/main.rs:6-8`

```rust
mod api;
mod compiler;
mod cpp_compiler;
mod sandbox;
```

- [ ] **Step 3: 编译检查**

Run:
```bash
cargo check
```

Expected: 无错误，可能有一个未使用模块的 warning。

---

### Task 2: 在 API 层增加 C/C++ 处理

**Files:**
- Modify: `src/api.rs`

**Interfaces:**
- Consumes: `crate::cpp_compiler::compile_cpp_to_wasm`
- Consumes: `crate::sandbox::run_wasm`
- Produces: `pub async fn evaluate_cpp(Json(payload): Json<EvaluateCppRequest>) -> (StatusCode, Json<RunResponse>)`

- [ ] **Step 1: 新增请求结构体与模块引用**

在 `src/api.rs` 顶部：

```rust
use crate::{compiler, cpp_compiler, sandbox};
```

在 `EvaluateRequest` 结构体之后新增：

```rust
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
```

- [ ] **Step 2: 新增 `evaluate_cpp` 处理函数**

在 `evaluate` 函数之后新增：

```rust
pub async fn evaluate_cpp(Json(payload): Json<EvaluateCppRequest>) -> (StatusCode, Json<RunResponse>) {
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
```

- [ ] **Step 3: 编译检查**

Run:
```bash
cargo check
```

Expected: 无错误。

---

### Task 3: 注册新路由

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `api::evaluate_cpp`

- [ ] **Step 1: 添加新路由**

Modify `src/main.rs` 路由注册部分为：

```rust
let app = Router::new()
    .route("/ping", get(|| async { "pong" }))
    .route("/evaluate.json", post(api::evaluate))
    .route("/api/run", post(api::evaluate))
    .route("/evaluate-cpp.json", post(api::evaluate_cpp))
    .route("/api/run-cpp", post(api::evaluate_cpp))
    .layer(tower_http::cors::CorsLayer::permissive());
```

- [ ] **Step 2: 更新启动日志（可选）**

将日志信息改为：
```rust
info!("Playground server listening on http://{}", addr);
```

- [ ] **Step 3: 编译并运行**

Run:
```bash
cargo build
cargo run
```

Expected: 服务成功启动，监听 `0.0.0.0:9001`。

---

### Task 4: 更新 README.md

**Files:**
- Modify: `README.md`

- [ ] **Step 1: 在环境要求中新增 Zig**

在 Rust 工具链说明后新增：

```markdown
### Zig

C/C++ 代码通过 Zig 编译为 `wasm32-wasip1`：

#### macOS

```bash
brew install zig
```

#### Linux

```bash
# 以 x86_64 为例，其他架构请替换对应 tarball
wget https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz
tar xf zig-linux-x86_64-0.13.0.tar.xz
sudo mv zig-linux-x86_64-0.13.0 /opt/zig
sudo ln -s /opt/zig/zig /usr/local/bin/zig
```

安装后验证：

```bash
zig version
```
```

- [ ] **Step 2: 新增 C/C++ 接口说明**

在 `/evaluate.json` 接口说明之后新增：

```markdown
### `POST /evaluate-cpp.json`

执行 C 或 C++ 代码。

请求体：

```json
{
  "code": "#include <iostream>\nint main() { std::cout << \"Hello, C++!\" << std::endl; return 0; }",
  "language": "cpp"
}
```

`language` 可选 `"c"` 或 `"cpp"`，默认为 `"cpp"`。

响应体与 `/evaluate.json` 格式一致。

### `POST /api/run-cpp`

与 `/evaluate-cpp.json` 完全相同的处理函数，仅作为本地历史兼容入口。
```

- [ ] **Step 3: 验证文档格式**

Run:
```bash
cargo check
```

Expected: 无错误（README 不影响编译）。

---

### Task 5: 更新 AGENTS.md

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: 更新技术栈、接口列表、代码组织、安全限制**

在技术栈中新增：
```markdown
- **C/C++ 编译器**：Zig
```

在 API 接口中新增 `/evaluate-cpp.json` 和 `/api/run-cpp` 说明。

在代码组织表中新增 `src/cpp_compiler.rs`。

在安全限制中说明 C/C++ 同样进入 wasmtime 沙箱执行。

- [ ] **Step 2: 检查一致性**

通读 `AGENTS.md`，确保新增描述与实际代码一致。

---

### Task 6: 编译检查与接口测试

**Files:**
- 无新增文件

- [ ] **Step 1: 完整编译**

Run:
```bash
cargo build
```

Expected: 成功，无错误。

- [ ] **Step 2: 启动服务**

Run:
```bash
cargo run
```

Expected: 服务监听 `0.0.0.0:9001`。

- [ ] **Step 3: 健康检查**

Run:
```bash
curl http://127.0.0.1:9001/ping
```

Expected: `pong`

- [ ] **Step 4: C++ 执行测试**

Run:
```bash
curl -X POST http://127.0.0.1:9001/evaluate-cpp.json \
  -H "Content-Type: application/json" \
  -d '{"code":"#include <iostream>\nint main() { std::cout << \"hello cpp\" << std::endl; return 0; }","language":"cpp"}'
```

Expected:
```json
{"success":true,"stdout":"hello cpp\n","stderr":"","error":null}
```

- [ ] **Step 5: C 执行测试**

Run:
```bash
curl -X POST http://127.0.0.1:9001/evaluate-cpp.json \
  -H "Content-Type: application/json" \
  -d '{"code":"#include <stdio.h>\nint main() { printf(\"hello c\\n\"); return 0; }","language":"c"}'
```

Expected:
```json
{"success":true,"stdout":"hello c\n","stderr":"","error":null}
```

- [ ] **Step 6: 编译错误测试**

Run:
```bash
curl -X POST http://127.0.0.1:9001/evaluate-cpp.json \
  -H "Content-Type: application/json" \
  -d '{"code":"int main() { int x = \"wrong\"; return 0; }","language":"cpp"}'
```

Expected: `success` 为 `false`，`error` 为 `"Compilation failed"`，`stderr` 包含类型错误信息。

- [ ] **Step 7: 运行 `cargo fmt` 与 `cargo clippy`**

Run:
```bash
cargo fmt
cargo clippy
```

Expected: `fmt` 不修改文件；`clippy` 无关键警告。

---

## Spec Coverage

| 设计文档要求 | 对应任务 |
|-------------|---------|
| 新增 `/evaluate-cpp.json` 与 `/api/run-cpp` | Task 3 |
| 请求体支持 `code` 与 `language`，默认 `"cpp"` | Task 2 |
| 使用 Zig 编译到 `wasm32-wasip1` | Task 1 |
| 复用 wasmtime 沙箱执行 | Task 1, Task 2 |
| 编译超时 30 秒 | Task 1 |
| 代码长度限制 64 KiB | Task 2 |
| README.md 新增依赖安装说明 | Task 4 |
| AGENTS.md 更新接口与架构说明 | Task 5 |
| 本地接口测试 | Task 6 |

## Placeholder Scan

- 无 "TBD" / "TODO" / "implement later" / "fill in details"。
- 无 "Add appropriate error handling" 等模糊描述。
- 每个步骤包含具体代码或命令。

## Type Consistency

- `compile_cpp_to_wasm(code: &str, language: &str) -> anyhow::Result<Vec<u8>>` 在 Task 1 定义，Task 2 中通过 `cpp_compiler::compile_cpp_to_wasm` 调用。
- `EvaluateCppRequest` 在 Task 2 定义，Task 3 中通过 `api::evaluate_cpp` 使用。
- `RunResponse` 复用现有类型，未变更。
