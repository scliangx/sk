//! import 命令：从 ~/.ssh/config 导入已有配置

use crate::cli::args::OutputFormat;
use crate::domain::export::importer::Importer;
use crate::error::SkResult;
use crate::ui::interactive::Interactive;
use crate::ui::output::{Output, OutputMode};

pub fn run(
    file: Option<&str>,
    yes: bool,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    let result = if let Some(path) = file {
        Importer::import_file(std::path::Path::new(path))?
    } else {
        Importer::import_default()?
    };

    if result.servers.is_empty() {
        output.warn("No importable server configurations found.");
        for w in &result.warnings {
            output.info(w);
        }
        return Ok(());
    }

    // 展示解析结果
    if !output.is_json() {
        println!("\nFound {} server(s) to import:", result.servers.len());
        for s in &result.servers {
            println!("  {}  →  {}@{}:{}", s.name, s.user, s.host, s.port);
        }
        println!();
    }

    // 确认导入
    let confirmed = yes || Interactive::confirm("Import these servers into sk?");
    if !confirmed {
        output.info("Import cancelled.");
        return Ok(());
    }

    let (added, skipped) = Importer::add_to_managed(&result.servers, false)?;

    match output_format {
        OutputFormat::Text => {
            output.success(&format!("Imported {} server(s).", added));
            if skipped > 0 {
                output.info(&format!("Skipped {} existing server(s). Use --force to overwrite.", skipped));
            }
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "status": "ok",
                "imported": added,
                "skipped": skipped,
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_default() {
        // 不应 panic
        let _ = run(None, true, OutputFormat::Text, false);
    }
}
