# sk(ssh keys) — SSH Keys Manager

[中文文档](docs/README_zh.md) | English

> **sk** eliminates SSH password prompts. Add a server once, then connect with just `sk <name>`.

> **⚠️ Development Status:** This project is in active early development (v0.1.0). Core features (add / remove / list / test / direct connect via `sk <name>` / import / export / doctor / completion) are functional and tested. Some planned features (`batch` is basic, `sync` is a stub). APIs may change before v1.0. Contributions and feedback welcome.

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey)]()

---

## Features

- **One-command connect** — `sk prod` instead of `ssh user@10.0.0.1 -p 2222 -i ~/.ssh/key`
- **Password auto-login** — `sk add -p <password>` stores credentials securely, `sk <name>` uses them automatically
- **Key-based setup** — `sk add -k` generates ED25519 key + pushes to server + writes config
- **Connection verification** — every `sk add` tests the connection before saving
- **SSH protocol native** — no external `ssh` binary dependency for core operations (uses `ssh2` / libssh2)
- **Secure storage** — passwords stored in system keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service) with AES-256-GCM encrypted file fallback
- **SSH config compatible** — stores data in `~/.sk/`, can import from / export to `~/.ssh/config`
- **Cross-platform** — macOS, Linux, Windows

---

## Installation

```bash
# From source
cargo install --git https://github.com/scliangx/sk.git

# Or build locally
git clone https://github.com/scliangx/sk.git
cd sk
cargo build --release
```

**Prerequisites:** Rust 1.70+, OpenSSH client

---

## Quick Start

```bash
# 1. Add a server with password
sk add prod -H 10.0.0.1 -u admin -p mypassword

# 2. Connect (password auto-used)
sk prod

# 3. Add with key-based auth (generate + push)
sk add staging -H staging.example.com -u deploy -k

# 4. List all servers
sk list

# 5. Test connection
sk test prod

# 6. Remove servers
sk remove prod staging
```

---

## Commands

### `sk <name>` — Connect

```
sk prod                 # connect to configured server
sk user@host            # ad-hoc connection (password prompt)
sk user@host:2222       # ad-hoc with port
sk                      # interactive server selection
```

Authentication priority: stored password → IdentityFile key → ssh-agent → prompt

### `sk add` — Add Server

```
sk add <name> -H <host> -u <user> -p <password> [-P 2222] [-i ~/.sk/keys/key]
sk add <name> -H <host> -u <user> -k                # key-based setup
```

| Flag | Description |
|------|-------------|
| `-H, --host` | Server IP or hostname |
| `-u, --user` | SSH username |
| `-p, --password` | SSH password (stored securely) |
| `-P, --port` | SSH port (default: 22) |
| `-i, --identity-file` | Use existing key file |
| `-k, --with-key` | Generate ED25519 key + push to server |
| `-f, --force` | Overwrite existing config |

Before saving, `sk add` tests the connection — unreachable servers are rejected.

### `sk remove` — Remove Servers

```
sk remove prod                  # single
sk remove prod staging dev      # batch
sk remove prod -f               # skip confirmation
sk remove prod -f -k            # also delete key files
```

### `sk list` — List Servers

```
sk list                  # table view
sk list -j               # JSON output
sk list prod             # filter by name
```

### `sk test` — Test Connection

```
sk test prod
sk test prod -v           # verbose (SSH handshake details)
sk test prod -j           # JSON output
sk test prod -t 5         # custom timeout (seconds)
```

### `sk import` — Import Config

Imports from SSH config format (standard `~/.ssh/config` syntax).

```
sk import                     # from ~/.ssh/config
sk import -f servers.txt      # from SSH config file
sk import -y                  # skip confirmation
```

### `sk export` — Export Config

```
sk export                     # YAML to stdout
sk export -F json             # JSON to stdout
sk export -o backup.yaml      # write to file
```

### `sk doctor` — Health Check

```
sk doctor              # check all servers
sk doctor -j           # JSON output
```

Checks: config file validity, key file existence/permissions, password storage status, TCP reachability.

### `sk completion` — Shell Completion

```
sk completion install     # auto-detect shell and install
sk completion uninstall   # remove completion
sk completion bash        # print bash script
sk completion powershell  # print PowerShell script
sk completion zsh         # print zsh script
sk completion fish        # print fish script
```

### `sk batch` — Batch Import

```
sk batch add servers.csv     # CSV: name,host,user,port,password
sk batch add servers.csv -c 8  # concurrency
```

---

## Global Options

| Flag | Description |
|------|-------------|
| `-v, --verbose` | Verbose output |
| `-j, --json` | JSON output (scripting-friendly) |
| `-h, --help` | Show help |
| `-V, --version` | Show version |

---

## How It Works

```
~/.sk/
├── servers.yaml        # Primary data store (all server configs)
├── metadata.yaml       # Server metadata (created_at, password_stored)
├── passwords/          # AES-256-GCM encrypted passwords (*.enc)
└── keys/               # ED25519 key pairs (name_key, name_key.pub)
```

> sk does NOT touch `~/.ssh/config`. Use `sk import` to pull from SSH config, or `sk export --to-ssh` to write back.

**Password storage flow:**
1. System keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service)
2. AES-256-GCM file fallback with Argon2id key derivation (19MB memory, 3 iterations)
3. Key bound to machine fingerprints (hostname, username, HOME, OS machine-id)

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Network error |
| 2 | Authentication failed |
| 3 | File write error |
| 4 | Invalid argument |
| 5 | Missing dependency |
| 6 | Config error |
| 7 | Key operation failed |
| 8 | Password store error |
| 99 | Internal error |

---

## Development

```bash
cargo test              # 156 unit tests
cargo build --release   # optimized build
```

### E2E Testing (requires podman)

```bash
just test-core          # full test suite with SSH container
just test-smoke         # quick smoke test
```

---

## License

MIT License. See [LICENSE](LICENSE).
