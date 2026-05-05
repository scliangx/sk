//! test 命令：测试 SSH 服务器连接状态

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::domain::ssh::connection::SshConnectionTester;
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

/// 测试服务器连接的完整流程
pub fn run(
    name: &str,
    timeout: u64,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    // 获取服务器配置
    let server = Orchestrator::get_server(name)?;
    let server = match server {
        Some(s) => s,
        None => {
            output.error(&format!(
                "Server '{}' not found. Use 'sk list' to see all servers.",
                name
            ));
            return Ok(());
        }
    };

    // 显示测试信息
    output.info(&format!(
        "Testing {} ({}:{})...",
        server.name, server.host, server.port
    ));

    // 运行连接测试
    let result = SshConnectionTester::test(&server, Some(timeout));

    // 输出结果
    output.test_result(name, &result);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_nonexistent_server() {
        let result = run("nonexistent-12345", 2, OutputFormat::Text, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_test_json_mode() {
        let result = run("nonexistent-12345", 2, OutputFormat::Json, false);
        assert!(result.is_ok());
    }
}
