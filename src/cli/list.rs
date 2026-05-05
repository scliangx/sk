//! list 命令：列出所有 SSH 服务器配置

use colored::Colorize;

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

/// 列出所有服务器的完整流程
pub fn run(
    filter: Option<&str>,
    _reachable: bool,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    let servers = Orchestrator::list_servers(filter)?;

    if servers.is_empty() {
        if filter.is_some() {
            output.info(&format!("No servers matching filter '{}'.", filter.unwrap()));
        } else {
            output.info("No servers configured yet.");
            if !output.is_json() {
                println!();
                println!("  Quick Start:");
                println!(
                    "    {} {}",
                    "sk add <name> -H <IP> -u <user> -p <password>".dimmed(),
                    "# Add server with password".dimmed()
                );
                println!(
                    "    {} {}",
                    "sk ssh <name>".dimmed(),
                    "# Connect to server".dimmed()
                );
            }
        }
        return Ok(());
    }

    output.server_list(&servers);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_empty_does_not_panic() {
        let result = run(None, false, OutputFormat::Text, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_with_filter_does_not_panic() {
        let result = run(Some("nonexistent"), false, OutputFormat::Text, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_json_mode() {
        let result = run(None, false, OutputFormat::Json, false);
        assert!(result.is_ok());
    }
}
