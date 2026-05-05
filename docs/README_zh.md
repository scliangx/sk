# sk — SSH 密钥管理器

[English](../README.md) | 中文

> **sk** 消除 SSH 密码提示。添加一次服务器，之后只需 `sk <名称>` 即可连接。

> **⚠️ 开发状态：** 项目处于早期活跃开发阶段（v0.1.0）。核心功能（add / remove / list / test / sk <名称> 直连 / import / export / doctor / completion）已可用并通过测试。部分功能（`batch` 基础实现，`sync` 骨架占位）。v1.0 之前 API 可能变化。欢迎贡献和反馈。

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey)]()

---

## 功能特性

- **一键连接** — `sk prod` 替代 `ssh user@10.0.0.1 -p 2222 -i ~/.ssh/key`
- **密码自动登录** — `sk add -p <密码>` 安全存储凭据，`sk <名称>` 自动使用
- **密钥免密配置** — `sk add -k` 生成 ED25519 密钥 + 推送到服务器 + 写入配置
- **连接验证** — 每次 `sk add` 先测试连接，不可达的服务器拒绝添加
- **原生 SSH 协议** — 核心操作不依赖外部 `ssh` 二进制（使用 ssh2 / libssh2）
- **安全存储** — 密码存储在系统钥匙串（macOS 钥匙串 / Windows 凭据管理器 / Linux Secret Service），AES-256-GCM 加密文件降级
- **SSH config 兼容** — 读写标准 `~/.ssh/config`，可直接使用 `ssh` 命令
- **跨平台** — macOS、Linux、Windows

---

## 安装

```bash
# 从源码安装
cargo install --git https://github.com/scliangx/sk.git

# 或本地编译
git clone https://github.com/scliangx/sk.git
cd sk
cargo build --release
```

**前置依赖:** Rust 1.70+, OpenSSH 客户端

---

## 快速开始

```bash
# 1. 添加服务器（带密码）
sk add prod -H 10.0.0.1 -u admin -p mypassword

# 2. 连接（自动使用密码）
sk prod

# 3. 添加并配置密钥免密登录
sk add staging -H staging.example.com -u deploy -k

# 4. 列出所有服务器
sk list

# 5. 测试连接
sk test prod

# 6. 删除服务器
sk remove prod staging
```

---

## 命令参考

### `sk <名称>` — 连接

```
sk prod                 # 连接已配置的服务器
sk user@host            # 临时连接（提示输入密码）
sk user@host:2222       # 指定端口
sk                      # 交互式选择服务器
```

认证优先级：存储密码 → IdentityFile 密钥 → ssh-agent → 提示输入

### `sk add` — 添加服务器

```
sk add <名称> -H <主机> -u <用户> -p <密码> [-P 端口] [-i 密钥文件]
sk add <名称> -H <主机> -u <用户> -k              # 密钥免密模式
```

| 参数 | 说明 |
|------|------|
| `-H, --host` | 服务器 IP 或域名 |
| `-u, --user` | SSH 用户名 |
| `-p, --password` | SSH 密码（安全存储） |
| `-P, --port` | SSH 端口（默认 22） |
| `-i, --identity-file` | 使用已有密钥文件 |
| `-k, --with-key` | 生成 ED25519 密钥 + 推送到服务器 |
| `-f, --force` | 强制覆盖已有配置 |

添加前会先测试连接，不可达的服务器不会写入配置。

### `sk remove` — 删除服务器

```
sk remove prod                  # 单个
sk remove prod staging dev      # 批量
sk remove prod -f               # 跳过确认
sk remove prod -f -k            # 同时删除密钥文件
```

### `sk list` — 列出服务器

```
sk list                  # 表格视图
sk list -j               # JSON 输出
sk list prod             # 按名称过滤
```

### `sk test` — 测试连接

```
sk test prod
sk test prod -v           # 详细模式
sk test prod -j           # JSON 输出
sk test prod -t 5         # 自定义超时（秒）
```

### `sk import` — 导入配置

从 SSH config 格式文件导入（标准 `~/.ssh/config` 语法）。

```
sk import                     # 从 ~/.ssh/config 导入
sk import -f servers.txt      # 从 SSH config 文件导入
sk import -y                  # 跳过确认
```

### `sk export` — 导出配置

```
sk export                     # YAML 输出到终端
sk export -F json             # JSON 输出到终端
sk export -o backup.yaml      # 写入文件
```

### `sk doctor` — 健康检查

```
sk doctor              # 检查所有服务器
sk doctor -j           # JSON 输出
```

检查项：配置文件有效性、密钥文件存在性及权限、密码存储状态、TCP 可达性。

### `sk completion` — Shell 补全

```
sk completion install     # 自动检测 shell 并安装
sk completion uninstall   # 卸载补全
sk completion bash        # 输出 bash 脚本
sk completion powershell  # 输出 PowerShell 脚本
sk completion zsh         # 输出 zsh 脚本
sk completion fish        # 输出 fish 脚本
```

### `sk batch` — 批量导入

```
sk batch add servers.csv      # CSV 格式: name,host,user,port,password
sk batch add servers.csv -c 8 # 并发数
```

---

## 全局选项

| 参数 | 说明 |
|------|------|
| `-v, --verbose` | 详细输出 |
| `-j, --json` | JSON 格式输出 |
| `-h, --help` | 显示帮助 |
| `-V, --version` | 显示版本 |

---

## 工作原理

```
~/.ssh/
├── config              # 标准 SSH 配置（sk 读写 Host 块）
└── sk/                 # sk 数据目录
    ├── metadata.yaml   # 服务器元数据
    ├── passwords/      # AES-256-GCM 加密密码 (*.enc)
    └── keys/           # ED25519 密钥对 (name_key, name_key.pub)
```

**密码存储流程：**
1. 系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）
2. AES-256-GCM 文件降级，Argon2id 密钥派生（19MB 内存，3 轮迭代）
3. 密钥绑定到机器指纹（hostname、username、HOME、OS machine-id）

---

## 退出码

| 码 | 含义 |
|----|------|
| 0 | 成功 |
| 1 | 网络错误 |
| 2 | 认证失败 |
| 3 | 文件写入错误 |
| 4 | 参数错误 |
| 5 | 依赖缺失 |
| 6 | 配置错误 |
| 7 | 密钥操作失败 |
| 8 | 密码存储错误 |
| 99 | 内部错误 |

---

## 开发

```bash
cargo test              # 156 个单元测试
cargo build --release   # 优化编译
```

### E2E 测试（需要 podman）

```bash
just test-core          # 完整测试套件
just test-smoke         # 快速冒烟测试
```

---

## License

MIT License. 详见 [LICENSE](../LICENSE)。
