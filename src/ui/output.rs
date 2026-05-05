//! 统一输出模块
//!
//! 提供两种输出模式：
//! - 人类可读模式（默认）：彩色终端输出，表格格式化
//! - JSON 模式：机器可读的结构化输出，适合脚本调用
//!
//! 所有 CLI 命令的输出都通过此模块统一处理。

use colored::*;

use crate::cli::args::OutputFormat;
use crate::domain::config::model::{ConnectionTestResult, Server, ServerStatus};

impl From<OutputFormat> for OutputMode {
    fn from(f: OutputFormat) -> Self {
        match f {
            OutputFormat::Text => OutputMode::Human,
            OutputFormat::Json => OutputMode::Json,
        }
    }
}

/// 输出模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// 人类可读的文本输出
    Human,
    /// 机器可读的 JSON 输出
    Json,
}

/// 输出管理器
///
/// 封装所有终端输出操作，统一处理 colors / icons / JSON。
pub struct Output {
    mode: OutputMode,
    verbose: bool,
}

impl Output {
    /// 创建新的输出管理器
    pub fn new(mode: OutputMode, verbose: bool) -> Self {
        Self { mode, verbose }
    }

    /// 输出成功信息
    pub fn success(&self, message: &str) {
        match self.mode {
            OutputMode::Human => println!("{} {}", "✅".green(), message),
            OutputMode::Json => {
                let json = serde_json::json!({
                    "status": "ok",
                    "message": message
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 输出错误信息
    pub fn error(&self, message: &str) {
        match self.mode {
            OutputMode::Human => eprintln!("{} {}", "❌".red(), message),
            OutputMode::Json => {
                let json = serde_json::json!({
                    "status": "error",
                    "message": message
                });
                eprintln!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 输出警告信息
    pub fn warn(&self, message: &str) {
        match self.mode {
            OutputMode::Human => eprintln!("{} {}", "⚠".yellow(), message),
            OutputMode::Json => {
                let json = serde_json::json!({
                    "status": "warning",
                    "message": message
                });
                eprintln!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 输出信息（仅在 verbose 模式下）
    pub fn info(&self, message: &str) {
        if self.verbose {
            match self.mode {
                OutputMode::Human => println!("{} {}", "ℹ".cyan(), message),
                OutputMode::Json => {
                    let json = serde_json::json!({
                        "status": "info",
                        "message": message
                    });
                    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
                }
            }
        }
    }

    /// 输出服务器列表（表格形式 / JSON）
    pub fn server_list(&self, servers: &[(Server, ServerStatus)]) {
        match self.mode {
            OutputMode::Human => self.print_server_table(servers),
            OutputMode::Json => self.print_server_json(servers),
        }
    }

    /// 以表格形式输出服务器列表
    fn print_server_table(&self, servers: &[(Server, ServerStatus)]) {
        if servers.is_empty() {
            println!("{}", "No servers configured. Use 'sk add' to add one.".dimmed());
            return;
        }

        let header = format!(
            "{:<20} {:<30} {:<12} {:<15} {:<15}",
            "NAME", "HOST", "PORT", "USER", "STATUS"
        );
        println!("{}", header.bold());
        println!("{}", "─".repeat(95).dimmed());

        for (server, status) in servers {
            let name = server.name.cyan();
            let host = server.host.clone();
            let port = server.port.to_string();
            let user = server.user.clone();
            let status_text = format!("{} {}", status.icon(), status.description());

            println!(
                "{:<20} {:<30} {:<12} {:<15} {}",
                name,
                host,
                port,
                user,
                status_text
            );
        }

        println!("{}", "─".repeat(95).dimmed());
        println!("Total: {} server(s)", servers.len());
    }

    /// 以 JSON 格式输出服务器列表
    fn print_server_json(&self, servers: &[(Server, ServerStatus)]) {
        let items: Vec<serde_json::Value> = servers
            .iter()
            .map(|(server, status)| {
                serde_json::json!({
                    "name": server.name,
                    "host": server.host,
                    "port": server.port,
                    "user": server.user,
                    "identity_file": server.identity_file,
                    "password_stored": server.password_stored,
                    "status": status.description(),
                    "created_at": server.created_at,
                })
            })
            .collect();

        let json = serde_json::json!({
            "total": servers.len(),
            "servers": items
        });

        println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
    }

    /// 输出连接测试结果
    pub fn test_result(&self, server_name: &str, result: &ConnectionTestResult) {
        match self.mode {
            OutputMode::Human => self.print_test_result_human(server_name, result),
            OutputMode::Json => {
                let json = serde_json::json!({
                    "server": server_name,
                    "result": result
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 以人类可读格式输出连接测试结果
    fn print_test_result_human(&self, server_name: &str, result: &ConnectionTestResult) {
        println!("{}", format!("Test Results: {}", server_name).bold());
        println!("{}", "─".repeat(50).dimmed());

        // TCP 层
        let tcp_status = if result.tcp_ok {
            "OK".green()
        } else {
            "FAIL".red()
        };
        println!(
            "TCP Connection: {:>30}  ({}ms)",
            tcp_status, result.tcp_latency_ms
        );

        // SSH 认证层
        if result.tcp_ok {
            let auth_status = if result.auth_ok {
                "OK".green()
            } else {
                "FAIL".red()
            };
            println!(
                "SSH Auth: {:>30}  ({}ms, {})",
                auth_status,
                result.total_latency_ms,
                if result.auth_method.is_empty() {
                    "N/A"
                } else {
                    &result.auth_method
                }
            );
        }

        // 错误信息
        if let Some(ref error) = result.error {
            println!("Error: {}", error.red());
        }

        // 总结
        if result.is_ok() {
            println!(
                "\n{} Connected! Use `ssh {}` to login.",
                "✅".green(),
                server_name
            );
        } else {
            println!("\n{} Connection failed. Check server status and network.", "❌".red());
        }
    }

    /// 输出添加服务器成功后的提示
    pub fn add_success(&self, server: &Server, has_password: bool) {
        match self.mode {
            OutputMode::Human => {
                println!();
                self.success(&format!("Server '{}' has been added.", server.name));
                println!();
                println!("  {}", format!("sk ssh {}", server.name).bold());

                if has_password {
                    println!("  {} Password stored — auto-login enabled", "🔒".green());
                } else {
                    println!(
                        "  {} No password stored — password will be required on connect",
                        "⚠".yellow()
                    );
                }

                if let Some(ref key) = server.identity_file {
                    println!("  {} Identity: {}", "🔑".dimmed(), key.display());
                }
            }
            OutputMode::Json => {
                let json = serde_json::json!({
                    "status": "ok",
                    "server": {
                        "name": server.name,
                        "host": server.host,
                        "port": server.port,
                        "user": server.user,
                        "identity_file": server.identity_file,
                        "password_stored": server.password_stored,
                    },
                    "command": format!("ssh {}", server.name),
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 输出删除成功信息
    pub fn remove_success(&self, name: &str, keys_deleted: bool) {
        match self.mode {
            OutputMode::Human => {
                self.success(&format!("Server '{}' has been removed.", name));
                if keys_deleted {
                    self.info("Associated key files have been deleted.");
                }
            }
            OutputMode::Json => {
                let json = serde_json::json!({
                    "status": "ok",
                    "message": format!("Server '{}' has been removed", name),
                    "keys_deleted": keys_deleted,
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
            }
        }
    }

    /// 检查是否为 JSON 输出模式
    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    /// 检查是否为详细模式
    #[allow(dead_code)]
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_server(name: &str) -> Server {
        Server::new(
            name.to_string(),
            "10.0.0.1".to_string(),
            "admin".to_string(),
        )
    }

    #[test]
    fn test_output_mode_human() {
        let output = Output::new(OutputMode::Human, false);
        assert!(!output.is_json());
        assert!(!output.is_verbose());
    }

    #[test]
    fn test_output_mode_json() {
        let output = Output::new(OutputMode::Json, true);
        assert!(output.is_json());
        assert!(output.is_verbose());
    }

    #[test]
    fn test_output_mode_verbose() {
        let output = Output::new(OutputMode::Human, true);
        assert!(!output.is_json());
        assert!(output.is_verbose());
    }

    #[test]
    fn test_server_list_empty_does_not_panic() {
        let output = Output::new(OutputMode::Human, false);
        output.server_list(&[]);
        // 不 panic 即为通过
    }

    #[test]
    fn test_server_list_json_does_not_panic() {
        let output = Output::new(OutputMode::Json, false);
        output.server_list(&[]);
    }

    #[test]
    fn test_server_list_with_data() {
        let output = Output::new(OutputMode::Human, false);
        let server = test_server("prod");
        let status = ServerStatus::Bare;
        output.server_list(&[(server, status)]);
    }

    #[test]
    fn test_test_result_human() {
        let output = Output::new(OutputMode::Human, false);
        let result = ConnectionTestResult::success(10, 150, "publickey");
        output.test_result("prod", &result);
    }

    #[test]
    fn test_test_result_json() {
        let output = Output::new(OutputMode::Json, false);
        let result = ConnectionTestResult::tcp_failed("timeout".to_string());
        output.test_result("bad-server", &result);
    }

    #[test]
    fn test_test_result_auth_failed() {
        let output = Output::new(OutputMode::Human, false);
        let result = ConnectionTestResult::auth_failed(5, 100, "bad key".to_string());
        output.test_result("server", &result);
    }

    #[test]
    fn test_add_success_output() {
        let output = Output::new(OutputMode::Human, false);
        let server = test_server("new-server")
            .with_identity_file(PathBuf::from("~/.ssh/sk/keys/new-server_key"));
        output.add_success(&server, true);
    }

    #[test]
    fn test_remove_success_output() {
        let output = Output::new(OutputMode::Human, false);
        output.remove_success("old-server", true);
    }
}
