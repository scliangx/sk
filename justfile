# =============================================================================
# sk (SSH Key) — E2E Test Suite
#
# 用法: just test-core  /  just test-smoke
# =============================================================================

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

SSH_PORT := "2222"
SSH_USER := "testuser"
SSH_PASS := "testpass"
CONTAINER_NAME := "sk-test-ssh"
IMAGE := "docker.io/library/ubuntu:latest"
SERVER_NAME := "sk-e2e-test"
HOST_IP := "172.22.200.102"

default: test-smoke

# ---- 构建 ----
build:
    cargo build --release

# ---- 容器管理 ----
start-ssh:
    -podman rm -f {{CONTAINER_NAME}}
    podman run -d --name {{CONTAINER_NAME}} -p {{SSH_PORT}}:22 {{IMAGE}} sleep infinity
    podman exec {{CONTAINER_NAME}} bash -c "apt-get update -qq 2>/dev/null; apt-get install -y -qq openssh-server 2>/dev/null; useradd -m -s /bin/bash {{SSH_USER}} 2>/dev/null; echo '{{SSH_USER}}:{{SSH_PASS}}' | chpasswd; mkdir -p /var/run/sshd /home/{{SSH_USER}}/.ssh; touch /home/{{SSH_USER}}/.ssh/authorized_keys; chmod 700 /home/{{SSH_USER}}/.ssh; chmod 600 /home/{{SSH_USER}}/.ssh/authorized_keys; chown -R {{SSH_USER}}:{{SSH_USER}} /home/{{SSH_USER}}/.ssh; echo PasswordAuthentication yes >> /etc/ssh/sshd_config; echo PubkeyAuthentication yes >> /etc/ssh/sshd_config; /usr/sbin/sshd"

stop-ssh:
    -podman stop {{CONTAINER_NAME}}
    -podman rm {{CONTAINER_NAME}}

# 生成测试密钥对并推送到容器 (单行: powershell 变量不跨行共享)
setup-key:
    $k = "$env:USERPROFILE\.ssh\sk\keys\{{SERVER_NAME}}_key"; mkdir -Force (Split-Path $k) *>$null; ssh-keygen -t ed25519 -f $k -N '""' -C "sk-e2e" -q 2>$null; type "$k.pub" | podman exec -i {{CONTAINER_NAME}} bash -c "cat >> /home/{{SSH_USER}}/.ssh/authorized_keys"

# ---- 单元测试 ----
test-unit:
    cargo test

# =============================================================================
# E2E 测试: 密码认证方式
# =============================================================================

test-add-password: build
    cargo run -- add {{SERVER_NAME}}-pw -H {{HOST_IP}} -u {{SSH_USER}} -p {{SSH_PASS}} -P {{SSH_PORT}} -f

test-conn-password: build test-add-password
    cargo run -- test {{SERVER_NAME}}-pw --timeout 15

test-list-password: build test-add-password
    cargo run -- list

test-remove-password:
    cargo run -- remove {{SERVER_NAME}}-pw -f

# =============================================================================
# E2E 测试: 密钥认证方式
# =============================================================================

test-add-key: build setup-key
    cargo run -- add {{SERVER_NAME}}-key -H {{HOST_IP}} -u {{SSH_USER}} -P {{SSH_PORT}} -i "$env:USERPROFILE\.ssh\sk\keys\{{SERVER_NAME}}_key" -f

test-conn-key: build test-add-key
    cargo run -- test {{SERVER_NAME}}-key --timeout 15

test-ssh-key: build setup-key test-add-key
    ssh -o StrictHostKeyChecking=no -o BatchMode=yes -o ConnectTimeout=10 -i "$env:USERPROFILE\.ssh\sk\keys\{{SERVER_NAME}}_key" -p {{SSH_PORT}} {{SSH_USER}}@{{HOST_IP}} "echo KEY_AUTH_OK"

test-remove-key:
    cargo run -- remove {{SERVER_NAME}}-key -f

# =============================================================================
# E2E 测试: 生成密钥 + 推送方式 (-k)
# =============================================================================

test-add-withkey: build
    cargo run -- add {{SERVER_NAME}}-wk -H {{HOST_IP}} -u {{SSH_USER}} -P {{SSH_PORT}} -k -f

test-remove-withkey:
    cargo run -- remove {{SERVER_NAME}}-wk -f

# =============================================================================
# 其他 E2E 测试
# =============================================================================

test-import: build
    @"`nHost test-import`n    HostName 1.2.3.4`n    User admin`n"@ > /tmp/sk_test_config
    cargo run -- import -f /tmp/sk_test_config -y
    -cargo run -- remove test-import -f
    -rm /tmp/sk_test_config

test-export: build
    cargo run -- export
    cargo run -- export -F json
    cargo run -- export -o /tmp/sk_test_export.yaml
    -rm /tmp/sk_test_export.yaml

test-doctor: build
    cargo run -- doctor
    cargo run -- doctor -j

test-help: build
    cargo run -- --version
    cargo run -- --help
    cargo run -- add --help
    cargo run -- completion --help

test-errors: build
    -cargo run -- add "" -H h -u u -p p
    -cargo run -- add a-b -H h -u u
    -cargo run -- remove noexist-xyz -f
    -cargo run -- test noexist-xyz

# =============================================================================
# 组合测试
# =============================================================================

# 密码认证完整流程
test-flow-password: start-ssh test-add-password test-conn-password test-list-password test-remove-password stop-ssh

# 密钥认证完整流程
test-flow-key: start-ssh setup-key test-add-key test-conn-key test-ssh-key test-remove-key stop-ssh

# 冒烟测试 (密码 + 密钥)
test-smoke: start-ssh test-add-password test-conn-password test-add-key test-conn-key test-ssh-key test-remove-password test-remove-key stop-ssh

# 核心测试
test-core: start-ssh test-unit test-help test-add-password test-conn-password test-add-key test-conn-key test-ssh-key test-list-password test-import test-export test-doctor test-errors test-remove-password test-remove-key stop-ssh

# ---- 清理 ----
clean: stop-ssh
    -rm -r -Force "$env:USERPROFILE\.ssh\sk\keys\{{SERVER_NAME}}_key*" *>$null
