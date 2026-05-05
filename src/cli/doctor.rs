//! sk doctor — 配置健康诊断
//!
//! 检查 SSH config、密钥文件、网络连通性等方面的健康状态。

use crate::app::orchestrator::Orchestrator;
use crate::cli::args::OutputFormat;
use crate::error::SkResult;
use crate::ui::output::{Output, OutputMode};

pub fn run(output_format: OutputFormat, _fix: bool) -> SkResult<()> {
    let output = Output::new(OutputMode::from(output_format), false);

    if output_format == OutputFormat::Json {
        doctor_json(&output)
    } else {
        doctor_text(&output)
    }
}

fn doctor_text(_output: &Output) -> SkResult<()> {
    println!("sk doctor — Configuration Health Check\n");

    // 1. SSH config 文件
    let config_path = crate::infra::fs::ssh_config_path()?;
    if config_path.exists() {
        println!("  ✅ SSH config: {}", config_path.display());
    } else {
        println!("  ⚠ SSH config not found. Run 'sk add' to create one.");
        println!();
        return Ok(());
    }

    // 2. 解析所有服务器
    let servers = match Orchestrator::list_servers(None) {
        Ok(s) => s,
        Err(e) => {
            println!("  ❌ Config parse error: {}", e);
            return Ok(());
        }
    };

    println!("  ✅ {} server(s) configured\n", servers.len());

    // 3. 检查每台服务器
    for (server, status) in &servers {
        println!("  {} {}", status.icon(), server.name);
        println!("    Connection: {}@{}:{}", server.user, server.host, server.port);

        // 密钥文件检查
        if let Some(ref key_path) = server.identity_file {
            let expanded = crate::domain::config::parser::expand_tilde(&key_path.to_string_lossy());
            let path = std::path::Path::new(&expanded);
            if path.exists() {
                print!("    Key file: {} ✅ exists", key_path.display());
                // Unix 权限检查
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(path) {
                        let mode = meta.permissions().mode() & 0o777;
                        if mode != 0o600 {
                            println!(" ⚠ permissions {:o} (should be 600)", mode);
                        } else {
                            println!(" ✅ permissions 600");
                        }
                    } else {
                        println!();
                    }
                }
                #[cfg(not(unix))]
                println!();
            } else {
                println!("    Key file: {} ❌ not found", key_path.display());
            }

            // 公钥文件
            let pub_path = path.with_extension("pub");
            if pub_path.exists() {
                println!("    Pub key:  {} ✅ exists", pub_path.display());
            } else {
                println!("    Pub key:  {} ❌ not found", pub_path.display());
            }
        } else {
            println!("    Auth: {} (no key file)", status.description());
        }

        // 密码存储状态
        let meta = crate::domain::config::metadata::MetadataManager::load_default()
            .ok()
            .and_then(|m| m.get_server(&server.name).cloned());
        if let Some(m) = meta {
            if m.password_stored {
                println!("    Password: ✅ stored ({})", m.password_backend);
            } else {
                println!("    Password: ⚠ not stored");
            }
        }

        // 快速 TCP 探活
        use std::net::TcpStream;
        use std::time::Duration;
        let addr = format!("{}:{}", server.host, server.port);
        match TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_secs(3),
        ) {
            Ok(_) => println!("    Reachable: ✅ TCP OK"),
            Err(_) => println!("    Reachable: ❌ unreachable"),
        }

        println!();
    }

    println!("Done. {} server(s) checked.", servers.len());
    Ok(())
}

fn doctor_json(_output: &Output) -> SkResult<()> {
    let servers = Orchestrator::list_servers(None).unwrap_or_default();
    let items: Vec<serde_json::Value> = servers
        .iter()
        .map(|(s, status)| {
            serde_json::json!({
                "name": s.name,
                "host": s.host,
                "port": s.port,
                "user": s.user,
                "status": status.description(),
                "has_identity_file": s.identity_file.is_some(),
                "password_stored": s.password_stored,
            })
        })
        .collect();

    let json = serde_json::json!({
        "total": servers.len(),
        "servers": items,
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
    Ok(())
}
