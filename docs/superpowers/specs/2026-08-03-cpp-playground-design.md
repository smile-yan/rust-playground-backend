# C/C++ Playground 接口设计

## 目标

为 `rust-playground` 新增一个 C/C++ Playground 接口，使用户能够提交 C 或 C++ 源码，由服务端编译为 `wasm32-wasi-musl`，并在现有的 wasmtime 沙箱中安全执行。

## 背景

当前项目仅支持 Rust 代码：

1. 接收 Rust 源码。
2. 使用 `rustc` 编译为 `wasm32-wasip1`。
3. 在 wasmtime 沙箱中执行，限制内存与运行时间。

C/C++ 同样可以通过 LLVM/Clang 或 Zig 工具链编译到 WebAssembly，从而复用现有 wasmtime 执行沙箱。本设计选择 **Zig** 作为编译工具链，原因是：

- 单二进制文件，部署简单。
- 内置 `wasm32-wasi-musl` 目标与 libc，无需额外配置 sysroot。
- 交叉编译能力强，未来扩展方便。

> **目标三元组说明**：设计初稿期望使用 `wasm32-wasip1`，但 Zig 0.16.0 直接传入该目标会报 `UnknownOperatingSystem`。实际验证 `wasm32-wasi-musl` 可被 Zig 接受，且生成的 WASM 模块能在 wasmtime preview1 中正常运行，因此统一采用 `wasm32-wasi-musl`。

## 设计原则

- **最小改动**：复用现有 `sandbox.rs` 的执行能力，只新增编译模块。
- **一致体验**：C/C++ 接口的请求/响应格式与现有 Rust 接口保持一致。
- **同等安全**：C/C++ 代码编译后同样进入 wasmtime 沙箱，受相同内存、时间、能力限制。

## API 设计

### 新增端点

```
POST /evaluate-cpp.json
POST /api/run-cpp     # 本地兼容入口
```

### 请求体

```json
{
  "code": "#include <iostream>\nint main() { std::cout << \"Hello, C++!\" << std::endl; return 0; }",
  "language": "cpp"
}
```

字段说明：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `code` | string | 是 | C 或 C++ 源代码 |
| `language` | string | 否 | `"c"` 或 `"cpp"`，默认为 `"cpp"` |

### 响应体

与 `/evaluate.json` 完全一致：

```json
{
  "success": true,
  "stdout": "Hello, C++!\n",
  "stderr": "",
  "error": null
}
```

## 编译流程

1. 接收请求，校验 `code` 长度不超过 64 KiB。
2. 根据 `language` 决定临时源文件名：
   - `"c"` → `main.c`
   - `"cpp"` → `main.cpp`
3. 将源码写入临时目录。
4. 调用 Zig 编译：
   - C 源码使用 `zig cc`：
     ```bash
     zig cc -target wasm32-wasi-musl -o main.wasm main.c
     ```
   - C++ 源码使用 `zig c++`：
     ```bash
     zig c++ -target wasm32-wasi-musl -o main.wasm main.cpp
     ```
5. 编译超时 30 秒，捕获 stdout/stderr。
6. 编译失败时，返回 stderr 内容，并标记 `error: "Compilation failed"`。
7. 编译成功后，读取 `main.wasm` 字节，临时目录自动清理。

## 执行流程

复用现有 `sandbox::run_wasm(wasm_bytes)`：

- 内存限制：256 MB
- 执行超时：5 秒
- 不启用文件系统、网络、环境变量或子进程访问
- 每个请求使用独立的 `Engine`、`Store` 和 `Module`

## 代码组织

| 文件 | 变更 |
|------|------|
| `src/main.rs` | 注册 `/evaluate-cpp.json` 和 `/api/run-cpp` 路由 |
| `src/api.rs` | 新增 `EvaluateCppRequest` 与 `evaluate_cpp` 处理函数，复用 `RunResponse` |
| `src/cpp_compiler.rs` | 新增 C/C++ 编译逻辑，C 调用 `zig cc`、C++ 调用 `zig c++` |
| `README.md` | 新增环境依赖章节，覆盖 macOS 与 Linux 安装方式 |
| `AGENTS.md` | 更新接口列表与安全限制说明 |

## 安全限制

- 代码长度上限：64 KiB
- 编译超时：30 秒
- 执行内存上限：256 MB
- 执行超时：5 秒
- WASI 能力：默认 `wasmtime_wasi::WasiCtxBuilder::new().build_p1()`，不启用文件系统、网络、环境变量或子进程访问

> 与 Rust 端点一致，当前实现未做请求限流。若暴露到公网，请在反向代理或负载均衡层补充限流策略。

## 依赖安装

### macOS

```bash
# Rust
rustup target add wasm32-wasip1

# Zig
brew install zig
```

### Linux

```bash
# Rust
rustup target add wasm32-wasip1

# Zig（以 x86_64 为例，其他架构替换对应 tarball）
wget https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz
tar xf zig-linux-x86_64-0.13.0.tar.xz
sudo mv zig-linux-x86_64-0.13.0 /opt/zig
sudo ln -s /opt/zig/zig /usr/local/bin/zig
```

确保 `zig` 在 `PATH` 中，且服务进程能够访问到。

## 测试方式

1. 编译检查：
   ```bash
   cargo check
   ```

2. 本地启动后调用接口：
   ```bash
   curl -X POST http://127.0.0.1:9001/evaluate-cpp.json \
     -H "Content-Type: application/json" \
     -d '{"code":"#include <iostream>\nint main() { std::cout << \"hello\" << std::endl; return 0; }","language":"cpp"}'
   ```

3. 健康检查：
   ```bash
   curl http://127.0.0.1:9001/ping
   ```

## 实现顺序

1. 新增 `src/cpp_compiler.rs` 编译模块。
2. 在 `src/api.rs` 中新增 C/C++ 请求处理函数。
3. 在 `src/main.rs` 中注册新路由。
4. 更新 `README.md` 依赖安装说明。
5. 更新 `AGENTS.md` 接口与安全说明。
6. 本地编译并接口测试。
