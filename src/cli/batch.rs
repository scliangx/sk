//! batch 命令：从 CSV 批量导入服务器配置
//!
//! CSV 格式：name,host,user,port,password
//! 直接写入 config 和密码存储，不检查连接可达性

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::domain::config::model::SecretString;
use crate::domain::password::store::PasswordManager;
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

#[derive(Debug, Clone)]
struct CsvRecord {
    name: String,
    host: String,
    user: String,
    port: u16,
    password: Option<String>,
}

pub fn run(
    file: &str,
    _concurrency: usize,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    let records = parse_csv(file)?;
    if records.is_empty() {
        output.warn("No valid records found in CSV file.");
        return Ok(());
    }

    // 确保数据目录存在
    crate::infra::fs::init_ssh_env()?;

    let total = records.len();
    output.info(&format!("Importing {} server(s)...", total));

    let mut succeeded = 0u32;
    let mut errors: Vec<(String, String)> = Vec::new();

    // 单线程顺序写入，避免文件锁冲突
    for rec in &records {
        match import_one(rec) {
            Ok(()) => {
                succeeded += 1;
                eprintln!("  [{}/{}] ✅ {}", succeeded + errors.len() as u32, total, rec.name);
            }
            Err(e) => {
                errors.push((rec.name.clone(), format!("{}", e)));
                eprintln!("  [{}/{}] ❌ {}", succeeded + errors.len() as u32, total, rec.name);
            }
        }
    }

    match output_format {
        OutputFormat::Text => {
            println!(
                "✅ Batch import complete: {} succeeded, {} failed ({} total)",
                succeeded,
                errors.len(),
                total
            );
            for (name, err) in &errors {
                println!("  ❌ {}: {}", name, err);
            }
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "status": "ok",
                "total": total,
                "succeeded": succeeded,
                "failed": errors.len(),
                "errors": errors.iter().map(|(n, e)| {
                    serde_json::json!({"name": n, "error": e})
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
        }
    }

    Ok(())
}

fn parse_csv(path: &str) -> SkResult<Vec<CsvRecord>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .flexible(true)
        .from_path(path)
        .map_err(|e| crate::error::SkError::Config(format!("Cannot open CSV: {}", e)))?;

    let mut records = Vec::new();
    for (idx, result) in reader.records().enumerate() {
        let record = result.map_err(|e| {
            crate::error::SkError::Config(format!("CSV parse error at line {}: {}", idx + 1, e))
        })?;
        if record.len() < 3 {
            eprintln!("⚠ Skipping line {}: need name,host,user", idx + 1);
            continue;
        }
        let name = record.get(0).unwrap_or("").trim().to_string();
        let host = record.get(1).unwrap_or("").trim().to_string();
        let user = record.get(2).unwrap_or("").trim().to_string();
        let port = record.get(3).and_then(|p| p.trim().parse().ok()).unwrap_or(22);
        let password = record.get(4).map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        if name.is_empty() || host.is_empty() || user.is_empty() { continue; }
        records.push(CsvRecord { name, host, user, port, password });
    }
    Ok(records)
}

fn import_one(rec: &CsvRecord) -> SkResult<()> {
    Orchestrator::add_server(&rec.name, &rec.host, &rec.user, rec.port, None, true)?;

    if let Some(ref pass) = rec.password {
        let secret = SecretString::new(pass.clone());
        let pm = PasswordManager::new();
        pm.save(&rec.name, &secret)?;
        let mut meta = crate::domain::config::metadata::MetadataManager::load_default()?;
        meta.upsert_server(&rec.name, true, pm.backend_name());
        meta.save()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_csv_valid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "server1,192.168.1.1,root,22,secret").unwrap();
        writeln!(f, "server2,example.com,admin").unwrap();
        let records = parse_csv(path.to_str().unwrap()).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "server1");
        assert_eq!(records[0].password.as_deref(), Some("secret"));
    }

    #[test]
    fn test_parse_csv_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.csv");
        std::fs::File::create(&path).unwrap();
        assert!(parse_csv(path.to_str().unwrap()).unwrap().is_empty());
    }

    #[test]
    fn test_parse_csv_skips_invalid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("invalid.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "onlyone").unwrap();
        writeln!(f, ",,").unwrap();
        writeln!(f, "ok,host.com,user").unwrap();
        assert_eq!(parse_csv(path.to_str().unwrap()).unwrap().len(), 1);
    }
}
