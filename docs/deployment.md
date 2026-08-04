# rust-playground 后端部署文档

本文档介绍如何从零开始部署 `rust-playground` 后端服务。

## 1. 服务器要求

- 操作系统：Linux（CentOS 7/8、Ubuntu、Debian 等均可）
- 架构：x86_64
- 内存：建议 ≥ 2GB
- 端口：需要对外开放 `9001/tcp`

## 2. 安装 Rust 工具链

后端在运行时会调用 `rustc` 编译用户提交的 Rust 代码，因此服务器上必须安装 Rust。

### 使用国内镜像安装（推荐）

```bash
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

curl --proto '=https' --tlsv1.2 -sSf \
  https://mirrors.ustc.edu.cn/rust-static/rustup/rustup-init.sh | sh -s -- -y --default-toolchain stable
```

将镜像地址写入 `~/.bashrc`，避免以后更新工具链时走官方源：

```bash
cat >> ~/.bashrc << 'EOF'
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup
EOF
```

加载环境变量：

```bash
source $HOME/.cargo/env
source ~/.bashrc
```

验证安装：

```bash
rustc --version
cargo --version
```

## 3. 安装 wasm32-wasip1 编译目标

后端将用户代码编译为 WASM，需要 `wasm32-wasip1` 目标：

```bash
rustup target add wasm32-wasip1
```

验证：

```bash
rustup target list --installed | grep wasm32-wasip1
```

## 4. 创建部署目录

```bash
sudo mkdir -p /opt/rust-playground
sudo chown $(id -u):$(id -g) /opt/rust-playground
```

> 你也可以使用其他目录，例如 `~/rust-playground`，只需在 GitHub Secrets 中配置对应的 `DEPLOY_PATH`。

## 5. 开放防火墙端口

服务默认监听 `0.0.0.0:9001`，需要放行该端口。

### CentOS 7/8（firewalld）

```bash
sudo firewall-cmd --permanent --add-port=9001/tcp
sudo firewall-cmd --reload
```

### CentOS 6 / iptables

```bash
sudo iptables -I INPUT -p tcp --dport 9001 -j ACCEPT
```

### 云服务商安全组

还需要在阿里云、腾讯云、AWS 等控制台的安全组/防火墙规则中放行 **9001/tcp**。

## 6. 配置 GitHub Secrets

在仓库 `Settings → Secrets and variables → Actions` 中添加以下 Secrets：

| Secret | 说明 | 示例 |
| --- | --- | --- |
| `DEPLOY_SERVERS` | 服务器部署矩阵的 JSON 字符串 | 见下方格式 |
| `GLOBAL_PRIVATE_KEY` | 所有服务器共用的 SSH 私钥 | `-----BEGIN OPENSSH PRIVATE KEY-----...` |

`DEPLOY_SERVERS` JSON 格式示例：

```json
{
  "include": [
    {
      "host": "1.2.3.4",
      "port": "22",
      "user": "deploy",
      "path": "/opt/rust-playground"
    },
    {
      "host": "5.6.7.8",
      "port": "22",
      "user": "deploy",
      "path": "/opt/rust-playground"
    }
  ]
}
```

字段说明：

| 字段 | 说明 | 默认值 |
| --- | --- | --- |
| `host` | 服务器域名或 IP | 必填 |
| `user` | 登录用户名 | 必填 |
| `port` | SSH 端口 | `22` |
| `path` | 部署目录 | `/opt/rust-playground` |

注意：

- `GLOBAL_PRIVATE_KEY` 对应的公钥需要添加到所有服务器的 `~/.ssh/authorized_keys`。
- 如果 `host` 是域名且解析到多个 IP，流水线会自动解析并固定使用其中一个 IP。

## 7. GitHub Actions 部署流程

推送以 `v` 开头的 tag 即可触发部署：

```bash
git tag -a v0.0.1 -m "release v0.0.1"
git push origin v0.0.1
```

流水线会做以下事情：

1. 使用 `x86_64-unknown-linux-musl` 目标编译静态二进制，避免服务器 glibc 版本不兼容。
2. 通过 `tar over SSH` 将二进制上传到服务器部署目录。
3. 在服务器上启动二进制，监听 `0.0.0.0:9001`。
4. 通过 `GET /ping` 进行健康检查。

## 8. 验证部署

部署完成后，在任意机器上测试：

```bash
curl http://<服务器IP>:9001/ping
```

应返回：

```
pong
```

测试代码执行接口：

```bash
curl -X POST http://<服务器IP>:9001/evaluate.json \
  -H "Content-Type: application/json" \
  -d '{"code":"fn main() { println!(\"hello\"); }"}'
```

## 9. 常见问题

### 9.1 健康检查 `HTTP 000`

说明端口没通。检查：

- 服务器本地防火墙是否放行 9001
- 云服务商安全组是否放行 9001
- 服务是否真的在监听 `0.0.0.0:9001`（而不是 `127.0.0.1:9001`）

查看本地监听：

```bash
ss -tlnp | grep 9001
```

### 9.2 `GLIBC_2.xxx not found`

如果看到类似错误，说明没有使用 musl 构建的二进制。请确认推送的是最新 tag，当前工作流已经改为 `x86_64-unknown-linux-musl` 目标。

### 9.3 `wasm32-wasi` 目标找不到

新工具链已将 `wasm32-wasi` 重命名为 `wasm32-wasip1`。请确认执行了：

```bash
rustup target add wasm32-wasip1
```

### 9.4 进程没有持续运行

如果 SSH 断开后进程消失，可以通过 systemd 托管。创建服务文件：

```bash
sudo tee /etc/systemd/system/rust-playground.service << 'EOF'
[Unit]
Description=Rust Playground Backend
After=network.target

[Service]
Type=simple
User=rust
WorkingDirectory=/opt/rust-playground
ExecStart=/opt/rust-playground/rust-playground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

重载并启动：

```bash
sudo systemctl daemon-reload
sudo systemctl enable rust-playground
sudo systemctl start rust-playground
sudo systemctl status rust-playground
```

> 注意：当前 GitHub Actions 默认使用普通用户进程启动，未直接使用 systemd。如需 systemd 托管，可手动在服务器上配置。
