//! export 命令：导出配置到文件

use crate::cli::args::OutputFormat;
use crate::domain::export::exporter::{ExportFormat, Exporter};
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

pub fn run(
    output_path: Option<&str>,
    format: &str,
    cli_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(cli_format), verbose);

    let export_fmt = match format.to_lowercase().as_str() {
        "json" => ExportFormat::Json,
        _ => ExportFormat::Yaml,
    };

    match output_path {
        Some(path) => {
            Exporter::export_to_file(std::path::Path::new(path), export_fmt)?;
            output.success(&format!("Configuration exported to {}", path));
        }
        None => {
            let content = Exporter::export_all(export_fmt)?;
            println!("{}", content);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_export_yaml_to_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("export.yaml");
        let result = run(
            Some(path.to_str().unwrap()),
            "yaml",
            OutputFormat::Text,
            false,
        );
        assert!(result.is_ok());
        assert!(path.exists());
    }

    #[test]
    fn test_export_json_stdout() {
        let result = run(None, "json", OutputFormat::Text, false);
        assert!(result.is_ok());
    }
}
