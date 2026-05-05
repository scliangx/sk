//! remove 命令：批量删除 SSH 服务器配置

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::error::SkResult;
use crate::ui::interactive::Interactive;
use crate::ui::output::{Output, OutputMode};

pub fn run(
    names: &[String],
    force: bool,
    delete_keys: bool,
    output_format: OutputFormat,
    _verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), _verbose);

    for name in names {
        let server = Orchestrator::get_server(name)?;
        let server = match server {
            Some(s) => s,
            None => {
                output.warn(&format!("Server '{}' not found, skipped.", name));
                continue;
            }
        };

        // 确认删除
        if !force {
            let message = format!(
                "Delete server '{}' ({}:{})?",
                server.name, server.host, server.port
            );
            if !Interactive::confirm(&message) {
                output.info(&format!("Skipped '{}'.", name));
                continue;
            }
        }

        // 确认删除密钥
        let del_keys = delete_keys
            || (!force && server.identity_file.is_some() && Interactive::confirm_delete_keys(name));

        output.info(&format!("Removing '{}'...", name));
        let (removed, keys_deleted) = Orchestrator::remove_server(name, del_keys)?;

        if removed {
            output.remove_success(name, keys_deleted);
        } else {
            output.warn(&format!("Server '{}' not found in config, skipped.", name));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_nonexistent_single() {
        let names = vec!["nonexistent-xyz".into()];
        assert!(run(&names, true, false, OutputFormat::Text, false).is_ok());
    }

    #[test]
    fn test_remove_nonexistent_multiple() {
        let names = vec!["no1".into(), "no2".into()];
        assert!(run(&names, true, false, OutputFormat::Text, false).is_ok());
    }
}
