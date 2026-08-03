# Rust Playground 后端

一个用 Rust 编写的后端服务，接收用户提交的 Rust 代码，编译成 WebAssembly 后在 wasmtime 沙箱中安全执行。

<img alt="image" src="https://github.com/user-attachments/assets/f4c41da8-9a57-40ce-99b1-4ed493f110a9" />

欢迎访问 rust 学习 playground: [https://rust.smileyan.cn/](https://rust.smileyan.cn/)

## 功能

- 提供 HTTP API 运行 Rust 代码
- 兼容官方 Rust Playground 的请求格式
- 使用 wasmtime 沙箱执行，限制内存和运行时间

## 环境要求

- 安装 [Rust](https://rustup.rs/)（1.96 或更高版本）
- 安装 `wasm32-wasip1` 编译目标：

```bash
rustup target add wasm32-wasip1
```

### Zig

C/C++ 代码通过 Zig 编译为 `wasm32-wasi-musl`：

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

## 运行服务

### 方式一：源码运行

```bash
cargo run
```

### 方式二：使用预编译二进制

从 `releases/` 目录下载适合你系统的二进制文件：

| 系统 | 选择文件 |
|------|----------|
| Linux x86_64 | `rust-playground-x86_64-unknown-linux-gnu` |
| macOS Intel | `rust-playground-x86_64-apple-darwin` |
| macOS Apple Silicon | `rust-playground-aarch64-apple-darwin` |
| Windows x86_64 | `rust-playground-x86_64-pc-windows-msvc.exe` |

Linux / macOS 启动示例：

```bash
chmod +x rust-playground-x86_64-apple-darwin
./rust-playground-x86_64-apple-darwin
```

Windows 启动示例：

```powershell
.\rust-playground-x86_64-pc-windows-msvc.exe
```

> 注意：使用预编译二进制时，仍需要本地安装 Rust 工具链和 `wasm32-wasip1` 目标，因为服务会调用 `rustc` 编译用户代码。

服务启动后监听 `http://0.0.0.0:9001`。

## 使用接口

### POST /evaluate.json

请求体：

```json
{
  "code": "fn main() { println!(\"Hello, WASM!\"); }"
}
```

只有 `code` 字段是必需的，其他字段仅作兼容保留。

返回示例：

```json
{
  "success": true,
  "stdout": "Hello, WASM!\n",
  "stderr": "",
  "error": null
}
```

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

### 命令行示例

```bash
curl -X POST http://127.0.0.1:9001/evaluate.json \
  -H "Content-Type: application/json" \
  -d '{"code":"fn main() { let sum: i32 = (1..=100).sum(); println!(\"{}\", sum); }"}'
```

## 安全限制

- 代码长度上限：64 KiB
- 编译超时：30 秒
- 内存上限：256 MB
- 运行超时：5 秒
- 禁止文件系统、网络、环境变量和子进程访问

## 注意事项

本项目是一个最小可用版本，适合学习和本地使用。如需部署到公网，建议增加限流、容器隔离等安全措施。

## 许可证

MIT
