# AGENTS.md — rust-playground

本文件面向 AI 编程助手，介绍 `rust-playground` 项目的结构、构建方式、运行架构与开发约定。阅读本文档前，默认读者对项目一无所知。

---

## 项目概述

`rust-playground` 是一个用 Rust 编写的后端服务，用于接收用户提交的 Rust 代码，将其编译为 WebAssembly（WASM），并在 `wasmtime` 沙箱中安全执行。

它是官方 Rust Playground 的轻量替代后端，兼容其 `/evaluate.json` 请求格式，同时保留了一个本地兼容端点 `/api/run`。

主要能力：

- 通过 HTTP API 接收 Rust 源代码。
- 使用本地 `rustc` 将代码编译为 `wasm32-wasip1` 目标。
- 在 `wasmtime` 中运行生成的 WASM，限制内存和运行时间。
- 返回执行结果（stdout / stderr / 错误信息）。

> 注意：这是一个最小可用版本，适合学习和本地使用。部署到公网时，建议额外增加限流、容器隔离等安全措施。

---

## 技术栈

- **语言**：Rust（edition 2021）
- **异步运行时**：Tokio（`full` feature）
- **Web 框架**：axum 0.8
- **WASM 运行时**：wasmtime 32 + wasmtime-wasi 32
- **序列化**：serde / serde_json
- **日志与追踪**：tracing / tracing-subscriber
- **CORS**：tower-http
- **临时文件**：tempfile
- **错误处理**：anyhow

---

## 项目结构

```
rust-playground/
├── Cargo.toml              # Rust 项目配置与依赖
├── Cargo.lock              # 依赖锁定文件
├── README.md               # 用户文档（中文）
├── LICENSE                 # MIT 许可证
├── .gitignore              # Git 忽略规则
├── AGENTS.md               # 本文件
├── src/
│   ├── main.rs             # 服务入口：路由、启动、日志
│   ├── api.rs              # HTTP API：请求解析、响应构造、长度校验
│   ├── compiler.rs         # 调用 rustc 将 Rust 源码编译为 WASM
│   └── sandbox.rs          # 使用 wasmtime 沙箱执行 WASM
├── docs/
│   ├── deployment.md       # 服务器部署文档
│   └── systemd.md          # systemd 托管文档
├── releases/
│   └── README.md           # 预编译二进制说明
└── .github/workflows/
    └── release.yml         # 推送 v* tag 时自动构建、发布与部署
```

---

## 构建与运行

### 环境要求

- Rust 1.96 或更高版本（通过 rustup 安装）
- `wasm32-wasip1` 编译目标：

```bash
rustup target add wasm32-wasip1
```

### 本地开发

```bash
# 检查代码
cargo check

# 编译
cargo build

# 运行服务
cargo run
```

服务启动后监听 `http://0.0.0.0:9001`。

### 生产构建

```bash
cargo build --release
```

---

## API 接口

服务提供以下 HTTP 端点：

### `GET /ping`

健康检查，返回 `pong`。

### `POST /evaluate.json`

执行 Rust 代码。请求体兼容官方 Rust Playground：

```json
{
  "code": "fn main() { println!(\"Hello, WASM!\"); }"
}
```

只有 `code` 字段是必需的，其余字段（`channel`、`edition`、`crateType`、`mode`、`tests`、`backtrace`）仅作兼容保留。

返回示例：

```json
{
  "success": true,
  "stdout": "Hello, WASM!\n",
  "stderr": "",
  "error": null
}
```

### `POST /api/run`

与 `/evaluate.json` 完全相同的处理函数，仅作为本地历史兼容入口。

---

## 代码组织

| 文件 | 职责 |
|------|------|
| `src/main.rs` | 配置 `tracing` 日志、注册路由、绑定 `0.0.0.0:9001`、启动 axum 服务。 |
| `src/api.rs` | 定义 `EvaluateRequest` / `RunResponse` 结构体；处理代码长度限制（最大 64 KiB）；编排 `compiler` 与 `sandbox`；将内部错误转换为 HTTP 响应。 |
| `src/compiler.rs` | 将源码写入临时文件，调用 `rustc --target=wasm32-wasip1 -C opt-level=2` 编译，30 秒超时；自动调用 `rustup target add wasm32-wasip1` 确保目标已安装。 |
| `src/sandbox.rs` | 使用 `wasmtime` 运行 WASM。配置 epoch 中断、256 MB 内存上限、stdout/stderr 内存管道、5 秒执行超时；在独立阻塞线程中执行，避免阻塞 Tokio 运行时。 |

---

## 安全限制

- **代码长度上限**：64 KiB（`src/api.rs`）。
- **编译超时**：30 秒（`src/compiler.rs`）。
- **运行内存上限**：256 MB（`src/sandbox.rs`）。
- **运行超时**：5 秒（`src/sandbox.rs`）。
- **WASI 能力**：默认 `wasmtime_wasi::WasiCtxBuilder::new().build_p1()`，不启用文件系统、网络、环境变量或子进程访问。
- **执行隔离**：每个请求使用独立的 `Engine`、`Store` 和 `Module`。

> 当前实现未做请求限流。若暴露到公网，请在反向代理或负载均衡层补充限流策略。

---

## 部署流程

项目使用 GitHub Actions 自动构建与部署（`.github/workflows/release.yml`）。

### 触发方式

推送以 `v` 开头的 tag：

```bash
git tag -a v0.0.1 -m "release v0.0.1"
git push origin v0.0.1
```

### CI 流程

1. **build-linux-x64**：使用 `x86_64-unknown-linux-musl` 目标构建静态二进制，避免 glibc 版本不兼容。
2. **build-cross**：为以下平台构建发布二进制：
   - `aarch64-unknown-linux-gnu`（Linux ARM64，原生 ARM runner）
   - `x86_64-apple-darwin`（macOS Intel）
   - `aarch64-apple-darwin`（macOS Apple Silicon）
   - `x86_64-pc-windows-msvc`（Windows x64）
3. **deploy**：`prepare` job 读取 `DEPLOY_SERVERS` Secret 中的 JSON 矩阵，生成动态部署矩阵；`deploy` job 并行将 `linux-x64` 二进制通过 SSH 上传到 JSON 中配置的每台云服务器部署目录，校验 sha256，确保 Rust 工具链与 `wasm32-wasip1` 目标存在，启动服务，并通过 `GET /ping` 健康检查。
4. **release**：创建 GitHub Release，上传所有预编译二进制与校验文件。

### 部署所需 Secrets

在仓库 `Settings → Secrets and variables → Actions` 中配置：

| Secret | 说明 | 默认值 |
|--------|------|--------|
| `DEPLOY_SERVERS` | 服务器部署矩阵的 JSON 字符串，见下方格式 | 必填 |

JSON 格式示例：

```json
{
  "include": [
    {
      "host": "1.2.3.4",
      "port": "22",
      "user": "deploy",
      "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----",
      "path": "/opt/rust-playground"
    },
    {
      "host": "5.6.7.8",
      "port": "22",
      "user": "deploy",
      "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----",
      "path": "/opt/rust-playground"
    }
  ]
}
```

字段说明：

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `host` | 服务器域名或 IP | 必填 |
| `user` | 登录用户名 | 必填 |
| `private_key` | SSH 私钥全文，换行用 `\n` 表示 | 必填 |
| `port` | SSH 端口 | `22` |
| `path` | 部署目录 | `/opt/rust-playground` |

如需部署到更多或更少服务器，直接修改 `DEPLOY_SERVERS` 的 `include` 数组即可，无需改动 workflow 文件。

### 服务器部署后可选：systemd 托管

流水线默认以普通用户进程启动服务。如需崩溃自动重启，可在服务器上配置 systemd 服务。详见 `docs/systemd.md`。若 `/etc/systemd/system/rust-playground.service` 已存在，部署脚本会自动调用 `systemctl restart`。

---

## 开发约定

- **语言**：源代码、注释与文档均使用中文。
- **格式化**：使用 `cargo fmt` 统一代码风格。
- **静态检查**：使用 `cargo clippy` 检查常见问题。
- **日志级别**：默认使用 `tracing` 的 `INFO` 级别；可通过 `RUST_LOG` 环境变量调整。
- **错误处理**：上层使用 `anyhow::Result`，编译错误使用自定义 `CompileError` 类型以区分编译失败与内部错误。
- **临时文件**：编译过程使用 `tempfile::tempdir()`，目录在编译结束后自动清理。
- **阻塞操作**：WASM 执行使用 `tokio::task::spawn_blocking`，避免阻塞异步运行时。

---

## 测试说明

当前项目为最小可用版本，**未包含单元测试或集成测试**。验证功能的主要方式：

1. 编译检查：

```bash
cargo check
```

2. 本地启动后调用接口：

```bash
curl -X POST http://127.0.0.1:9001/evaluate.json \
  -H "Content-Type: application/json" \
  -d '{"code":"fn main() { println!(\"hello\"); }"}'
```

3. 健康检查：

```bash
curl http://127.0.0.1:9001/ping
```

如需补充测试，可参考 axum 官方测试示例，使用 `tower::ServiceExt::oneshot` 对 `api::evaluate` 进行集成测试。

---

## 注意事项与常见坑

- 服务运行时会调用本机的 `rustc` 和 `rustup`，因此即使使用预编译二进制，服务器也必须安装 Rust 工具链与 `wasm32-wasip1` 目标。
- `wasm32-wasi` 在新工具链中已重命名为 `wasm32-wasip1`，代码统一使用后者。
- 服务监听 `0.0.0.0:9001`，请确保防火墙与安全组放行该端口。
- 部署脚本使用 `setsid` 启动进程以脱离 SSH 会话；若使用 systemd，请按 `docs/systemd.md` 配置，并确保 `PATH` 包含 `rustc` 所在目录。
- 修改监听端口需要编辑 `src/main.rs` 中 `SocketAddr::from(([0, 0, 0, 0], 9001))` 后重新部署。

---

## 许可证

MIT License，详见 `LICENSE`。
