//! batch 命令：从 CSV 批量导入服务器配置
//!
//! CSV 格式：name,host,user,port,password
//! 并发控制 + 进度条 + 错误汇总

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::domain::config::model::Server;
use crate::domain::password::store::PasswordManager;
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

/// 单条 CSV 记录
#[derive(Debug, Clone)]
struct CsvRecord {
    name: String,
    host: String,
    user: String,
    port: u16,
    password: Option<String>,
}

/// 单个服务器的导入结果
#[derive(Debug)]
struct ImportItem {
    name: String,
    error: Option<String>,
}

pub fn run(
    file: &str,
    concurrency: usize,
    output_format: OutputFormat,
    verbose: bool,
) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), verbose);

    // 解析 CSV
    let records = parse_csv(file)?;
    if records.is_empty() {
        output.warn("No valid records found in CSV file.");
        return Ok(());
    }

    let total = records.len();
    output.info(&format!("Found {} server(s) in CSV. Importing...", total));

    // 并发导入
    let processed = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(std::sync::Mutex::new(Vec::new()));

    let chunks: Vec<Vec<CsvRecord>> = records
        .chunks((records.len() + concurrency - 1) / concurrency)
        .map(|c| c.to_vec())
        .collect();

    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let processed = processed.clone();
            let errors = errors.clone();
            std::thread::spawn(move || {
                let mut batch_errors = Vec::new();
                for rec in &chunk {
                    let name = rec.name.clone();
                    match import_one(rec) {
                        Ok(()) => {
                            processed.fetch_add(1, Ordering::SeqCst);
                            if processed.load(Ordering::SeqCst) % 10 == 0 || processed.load(Ordering::SeqCst) == 1 {
                                eprintln!(
                                    "  Progress: {}/{}",
                                    processed.load(Ordering::SeqCst),
                                    processed.load(Ordering::SeqCst) // 近似
                                );
                            }
                        }
                        Err(e) => {
                            processed.fetch_add(1, Ordering::SeqCst);
                            batch_errors.push(ImportItem {
                                name: name.clone(),
                                error: Some(format!("{}", e)),
                            });
                        }
                    }
                }
                if !batch_errors.is_empty() {
                    let mut errs = errors.lock().unwrap();
                    errs.extend(batch_errors);
                }
            })
        })
        .collect();

    for h in handles {
        let _ = h.join();
    }

    let final_processed = processed.load(Ordering::SeqCst);
    let errs = errors.lock().unwrap();
    let success_count = final_processed - errs.len();

    // 输出结果
    match output_format {
        OutputFormat::Text => {
            println!(
                "✅ Batch import complete: {} succeeded, {} failed ({} total)",
                success_count,
                errs.len(),
                final_processed
            );
            for e in errs.iter() {
                println!("  ❌ {}: {}", e.name, e.error.as_ref().unwrap_or(&"unknown".into()));
            }
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "status": "ok",
                "total": final_processed,
                "succeeded": success_count,
                "failed": errs.len(),
                "errors": errs.iter().map(|e| {
                    serde_json::json!({"name": e.name, "error": e.error})
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
        }
    }

    Ok(())
}

/// 解析 CSV 文件
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

        // 最少需要 name,host,user
        if record.len() < 3 {
            eprintln!("⚠ Skipping line {}: need at least name,host,user", idx + 1);
            continue;
        }

        let name = record.get(0).unwrap_or("").trim().to_string();
        let host = record.get(1).unwrap_or("").trim().to_string();
        let user = record.get(2).unwrap_or("").trim().to_string();
        let port = record
            .get(3)
            .and_then(|p| p.trim().parse::<u16>().ok())
            .unwrap_or(22);
        let password = record.get(4).map(|p| p.trim().to_string()).filter(|p| !p.is_empty());

        if name.is_empty() || host.is_empty() || user.is_empty() {
            eprintln!("⚠ Skipping line {}: name/host/user cannot be empty", idx + 1);
            continue;
        }

        records.push(CsvRecord {
            name,
            host,
            user,
            port,
            password,
        });
    }

    Ok(records)
}

/// 导入单条记录
fn import_one(rec: &CsvRecord) -> SkResult<()> {
    // 跳过连接验证（批量模式），直接写入配置
    let mut server = Server::new(rec.name.clone(), rec.host.clone(), rec.user.clone());
    server.port = rec.port;

    Orchestrator::add_server(
        &rec.name, &rec.host, &rec.user, rec.port, None, true,
    )?;

    // 存储密码
    if let Some(ref pass) = rec.password {
        let secret = crate::domain::config::model::SecretString::new(pass.clone());
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
        assert_eq!(records[0].port, 22);
        assert_eq!(records[0].password.as_deref(), Some("secret"));
        assert_eq!(records[1].port, 22);
        assert!(records[1].password.is_none());
    }

    #[test]
    fn test_parse_csv_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.csv");
        std::fs::File::create(&path).unwrap();

        let records = parse_csv(path.to_str().unwrap()).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_parse_csv_skips_invalid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("invalid.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "onlyone").unwrap();
        writeln!(f, ",,").unwrap();
        writeln!(f, "ok,host.com,user").unwrap();

        let records = parse_csv(path.to_str().unwrap()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "ok");
    }
}
