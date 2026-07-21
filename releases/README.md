# 预编译二进制文件

本目录存放不同系统的预编译二进制文件，无需编译源码即可直接运行。

## 文件命名规则

```
rust-playground-<架构>-<系统>-<ABI>
```

常见文件名示例：

| 文件名 | 适用系统 |
|--------|----------|
| `rust-playground-x86_64-unknown-linux-gnu` | Linux x86_64 |
| `rust-playground-x86_64-apple-darwin` | macOS Intel 芯片 |
| `rust-playground-aarch64-apple-darwin` | macOS Apple 芯片 |
| `rust-playground-x86_64-pc-windows-msvc.exe` | Windows x86_64 |

## 如何选择

- **Linux 服务器 / WSL**：选择 `x86_64-unknown-linux-gnu`
- **macOS Intel（2019 及之前）**：选择 `x86_64-apple-darwin`
- **macOS Apple Silicon（M1/M2/M3）**：选择 `aarch64-apple-darwin`
- **Windows**：选择 `x86_64-pc-windows-msvc.exe`

## 运行方式

### Linux / macOS

```bash
# 赋予执行权限
chmod +x rust-playground-x86_64-apple-darwin

# 启动服务
./rust-playground-x86_64-apple-darwin
```

### Windows

```powershell
# 直接运行
.\rust-playground-x86_64-pc-windows-msvc.exe
```

服务启动后访问 `http://127.0.0.1:9001`。

## 注意事项

即使使用预编译二进制，运行本服务仍需要本地安装 Rust 工具链以及 `wasm32-wasip1` 目标，因为服务会调用 `rustc` 编译用户提交的代码。

```bash
rustup target add wasm32-wasip1
```
