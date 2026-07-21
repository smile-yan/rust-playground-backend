# 使用 systemd 管理 rust-playground 服务（崩溃自动重启）

本文档介绍如何在 Linux 服务器上通过 systemd 管理 `rust-playground` 后端服务，使其在崩溃、异常退出或服务器重启后自动恢复。

## 前置条件

- 已完成二进制部署（例如部署到 `/opt/rust-playground/rust-playground`）
- 服务器已安装 systemd（大多数现代 Linux 发行版默认自带）
- 具有 `sudo` 权限

## 1. 创建 systemd 服务文件

创建 `/etc/systemd/system/rust-playground.service`：

```ini
[Unit]
Description=Rust Playground Backend
Documentation=https://github.com/smile-yan/rust-playground-backend
After=network-online.target
Wants=network-online.target

[Service]
Type=simple

# 运行服务的用户和组（建议不要使用 root）
User=rust-playground
Group=rust-playground

# 工作目录
WorkingDirectory=/opt/rust-playground

# 启动命令
ExecStart=/opt/rust-playground/rust-playground

# 重启策略：非正常退出时自动重启
Restart=on-failure
RestartSec=5

# 更激进的策略：任何原因退出都重启（包括正常退出）
# Restart=always
# RestartSec=5

# 启动失败后的重试限制
StartLimitInterval=60s
StartLimitBurst=3

# 环境变量
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

# 确保 rustup 提供的 cargo/rustc 在 PATH 中
# 如果 rustc 通过 rustup 安装，需要显式设置 PATH
Environment="PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/root/.cargo/bin"

# 资源限制（可选）
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

## 2. 创建运行用户（推荐）

不要以 root 运行该服务：

```bash
sudo useradd --system --no-create-home --home-dir /opt/rust-playground --shell /usr/sbin/nologin rust-playground
sudo chown -R rust-playground:rust-playground /opt/rust-playground
```

## 3. 确保 rustup 环境可用

如果服务器上的 Rust 是通过 rustup 安装的，服务启动时可能找不到 `rustc`。需要在服务文件中正确设置 `PATH`，或者在启动脚本中 source cargo 环境。

### 方案 A：使用包装脚本（推荐）

创建 `/opt/rust-playground/start.sh`：

```bash
#!/bin/bash
set -euo pipefail

# Source rustup environment if available
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

exec /opt/rust-playground/rust-playground
```

然后：

```bash
sudo chmod +x /opt/rust-playground/start.sh
sudo chown rust-playground:rust-playground /opt/rust-playground/start.sh
```

并将服务文件中的 `ExecStart` 改为：

```ini
ExecStart=/opt/rust-playground/start.sh
```

### 方案 B：直接使用 root 的 cargo 路径

如果 rustup 安装在 `/root/.cargo/`，且你坚持不使用包装脚本，可以将 PATH 写死：

```ini
Environment="PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
```

## 4. 重载并启动服务

```bash
# 重载 systemd 配置
sudo systemctl daemon-reload

# 设置开机自启
sudo systemctl enable rust-playground

# 启动服务
sudo systemctl start rust-playground

# 查看状态
sudo systemctl status rust-playground
```

## 5. 查看日志

```bash
# 实时查看日志
sudo journalctl -u rust-playground -f

# 查看最近 100 条日志
sudo journalctl -u rust-playground -n 100

# 查看今天的日志
sudo journalctl -u rust-playground --since today
```

## 6. 常用操作

```bash
# 手动重启
sudo systemctl restart rust-playground

# 停止
sudo systemctl stop rust-playground

# 重新加载 systemd（修改服务文件后需要执行）
sudo systemctl daemon-reload

# 查看服务是否正在运行
sudo systemctl is-active rust-playground
```

## 7. 测试崩溃自动重启

服务运行后，可以手动 kill 进程测试重启：

```bash
sudo pkill -f /opt/rust-playground/rust-playground
sleep 5
sudo systemctl status rust-playground
```

如果配置正确，状态应显示为 `active (running)`，并且 `Restart` 计数会增加。

## 8. 与 GitHub Actions 部署集成

部署流水线中已经包含对 `rust-playground.service` 的检测逻辑：

```bash
if systemctl list-unit-files | grep -q "^rust-playground.service"; then
  sudo systemctl daemon-reload
  sudo systemctl restart rust-playground
fi
```

因此只要服务器上创建并启用了该服务，每次推送新 tag 后流水线都会自动重启服务。

## 9. 端口冲突说明

当前后端监听 `0.0.0.0:9001`。如果该端口被其他进程占用，服务启动会失败。请先确认端口空闲：

```bash
sudo ss -tlnp | grep 9001
```

如果 9001 被占用，需要先停止占用该端口的进程，或修改后端监听端口（修改 `src/main.rs` 后重新部署）。

## 参考

- [systemd.service 文档](https://www.freedesktop.org/software/systemd/man/systemd.service.html)
- [systemd 自动重启最佳实践](https://www.freedesktop.org/software/systemd/man/systemd.service.html#Restart=)
