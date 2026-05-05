//! CLI 参数定义
//!
//! sk 支持两种模式：
//! 1. 直接连接：sk <server> 或 sk <user@host>
//! 2. 管理命令：sk add/list/remove/test/import/export

use clap::{Parser, Subcommand};

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// sk — SSH 密钥管理工具
///
/// 没有子命令时，sk 作为 SSH 连接工具：
///   sk prod          → 连接已配置的服务器
///   sk user@host     → 直连（输入密码）
///
/// 管理命令：
///   sk add / list / remove / test / import / export
#[derive(Parser, Debug)]
#[command(
    name = "sk",
    version,
    about = "SSH Key Manager — one command for passwordless SSH",
    long_about = "Without a subcommand, sk connects to a server:\n  sk <name>        → connect to configured server\n  sk <user@host>    → ad-hoc connection\n\nUse subcommands to manage servers: add, list, remove, test, import, export.",
    after_help = "Examples:\n  sk prod\n  sk root@10.0.0.1\n  sk add prod -H 10.0.0.1 -u admin -p secret\n  sk list",
    disable_help_subcommand = true,
    subcommand_required = false
)]
pub struct Cli {
    #[arg(short = 'v', long, global = true, help = "Enable verbose output")]
    pub verbose: bool,

    #[arg(short = 'j', long, global = true, help = "Output in JSON format")]
    pub json: bool,

    /// 连接目标（不与子命令同时使用）
    #[arg(required = false, help = "Server name or user@host[:port] to connect to")]
    pub target: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// 管理子命令
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// 添加服务器配置
    #[command(
        name = "add",
        visible_alias = "a",
        about = "Add an SSH server configuration",
        after_help = "Examples:\n  sk add prod -H 10.0.0.1 -u admin -p secret123\n  sk add prod -H 10.0.0.1 -u admin -k"
    )]
    Add {
        #[arg(help = "Server name (alias)")]
        name: String,
        #[arg(short = 'H', long, help = "Server IP address or hostname")]
        host: String,
        #[arg(short = 'u', long, help = "SSH login username")]
        user: String,
        #[arg(short = 'p', long, help = "SSH login password (stored securely)")]
        password: Option<String>,
        #[arg(short = 'P', long, default_value = "22", help = "SSH port (1-65535)")]
        port: u16,
        #[arg(short = 'i', long, help = "Specify identity file path")]
        identity_file: Option<String>,
        #[arg(short = 'k', long, help = "Interactive: generate key + push to server")]
        with_key: bool,
        #[arg(short = 'f', long, help = "Force overwrite existing configuration")]
        force: bool,
    },

    /// 删除服务器配置
    #[command(
        name = "remove",
        visible_alias = "rm",
        about = "Remove SSH server configurations",
        after_help = "Examples:\n  sk remove prod\n  sk remove prod staging --force\n  sk remove prod staging dev -k"
    )]
    Remove {
        #[arg(help = "Name(s) of the server(s) to remove", num_args = 1.., required = true)]
        names: Vec<String>,
        #[arg(short = 'f', long, help = "Skip confirmation")]
        force: bool,
        #[arg(short = 'k', long, help = "Also delete associated key files")]
        delete_keys: bool,
    },

    /// 列出所有服务器
    #[command(
        name = "list",
        visible_alias = "ls",
        about = "List all configured SSH servers"
    )]
    List {
        #[arg(help = "Filter by keyword (optional)")]
        filter: Option<String>,
        #[arg(short = 'r', long, help = "Only show reachable servers")]
        reachable: bool,
    },

    /// 测试服务器连接
    #[command(
        name = "test",
        visible_alias = "t",
        about = "Test SSH server connection",
        after_help = "Examples:\n  sk test prod\n  sk test prod --verbose"
    )]
    Test {
        #[arg(help = "Name of the server to test")]
        name: String,
        #[arg(short = 't', long, default_value = "10", help = "TCP timeout in seconds")]
        timeout: u64,
    },

    /// 导入已有 SSH 配置
    #[command(name = "import", about = "Import from ~/.ssh/config")]
    Import {
        #[arg(short = 'f', long, help = "Import source file path")]
        file: Option<String>,
        #[arg(short = 'y', long, help = "Skip confirmation")]
        yes: bool,
    },

    /// 导出配置
    #[command(name = "export", about = "Export configuration to file")]
    Export {
        #[arg(short = 'o', long, help = "Export file path")]
        output: Option<String>,
        #[arg(short = 'F', long, default_value = "yaml", help = "Format: yaml or json")]
        format: String,
    },

    /// Shell 补全: 返回服务器名称列表
    #[command(name = "__complete-servers", hide = true)]
    CompleteServers {
        #[arg(required = false)]
        prefix: Option<String>,
    },

    /// Shell 补全脚本
    #[command(name = "completion", about = "Shell completion (auto-installs if no args)")]
    Completion {
        /// Shell 名称 或 "install" 自动安装
        #[arg(required = false, help = "Shell name (bash/zsh/fish/powershell) or 'install' to auto-configure")]
        shell: Option<String>,
    },

    /// 配置健康诊断
    #[command(name = "doctor", about = "Diagnose configuration issues")]
    Doctor {
        #[arg(short = 'f', long, help = "Auto-fix issues (not yet implemented)")]
        fix: bool,
    },

    /// 批量操作（CSV 导入）
    #[command(name = "batch", about = "Batch import from CSV")]
    Batch {
        #[command(subcommand)]
        action: BatchAction,
    },

    /// 配置同步（Git）
    #[command(name = "sync", about = "Config sync via Git")]
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
}

/// 批量操作子命令
#[derive(Debug, Subcommand)]
pub enum BatchAction {
    #[command(name = "add", about = "Batch add servers from CSV")]
    Add {
        #[arg(help = "CSV file path")]
        file: String,
        #[arg(short = 'c', long, default_value = "4", help = "Concurrency")]
        concurrency: usize,
    },
}

/// 配置同步子命令
#[derive(Debug, Subcommand)]
pub enum SyncAction {
    #[command(name = "push", about = "Push to Git repository")]
    Push {
        #[arg(short = 'm', long, help = "Commit message")]
        message: Option<String>,
    },
    #[command(name = "pull", about = "Pull from Git repository")]
    Pull,
    #[command(name = "init", about = "Initialize sync repository")]
    Init {
        #[arg(help = "Git remote URL")]
        url: String,
    },
}
