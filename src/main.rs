//! sk — SSH 密钥管理 CLI 工具
//!
//! 无子命令时作为连接工具：
//!   sk prod       → 查找配置并连接
//!   sk user@host   → 直连（交互式输入密码）

mod app;
mod cli;
mod domain;
mod error;
mod infra;
mod ui;

use clap::Parser;
use cli::args::{Cli, Commands, OutputFormat};
use error::SkResult;
use std::fs;
use std::process;

fn main() {
    // __complete 子命令由 clap-dyn-autocomplete 处理，不经过主 CLI
    if std::env::args().nth(1).as_deref() == Some("__complete") {
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        rt.block_on(async {
            use clap_dyn_autocomplete::Complete;
            let c = Complete::parse();
            c.println_to_stub_script::<Cli>(None, cli::custom_complete::SkCompleterFactory).await;
        });
        return;
    }

    let cli = Cli::parse();

    let fmt = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };
    let verbose = cli.verbose;

    let result: SkResult<()> = match &cli.command {
        Some(Commands::Add {
            name, host, user, password, port, identity_file, with_key, force,
        }) => cli::add::run(
            name, host, user, *port,
            password.as_deref(), identity_file.as_deref(),
            *with_key, *force, fmt, verbose,
        ),
        Some(Commands::Remove { names, force, delete_keys }) => {
            cli::remove::run(names, *force, *delete_keys, fmt, verbose)
        }
        Some(Commands::List { filter, reachable }) => {
            cli::list::run(filter.as_deref(), *reachable, fmt, verbose)
        }
        Some(Commands::Test { name, timeout }) => {
            cli::test::run(name, *timeout, fmt, verbose)
        }
        Some(Commands::Import { file, yes }) => {
            cli::import_cmd::run(file.as_deref(), *yes, fmt, verbose)
        }
        Some(Commands::Export { output, format }) => {
            cli::export_cmd::run(output.as_deref(), format, fmt, verbose)
        }
        Some(Commands::CompleteServers { prefix }) => {
            for s in domain::config::store::load_all().unwrap_or_default() {
                if prefix.as_ref().map_or(true, |p| s.name.starts_with(p)) {
                    println!("{}\tconfigured", s.name);
                }
            }
            Ok(())
        }
        Some(Commands::Completion { shell }) => {
            cli::completion::run(shell.as_deref())
        }
        Some(Commands::Doctor { fix }) => {
            cli::doctor::run(fmt, *fix)
        }
        Some(Commands::Batch { action }) => match action {
            crate::cli::args::BatchAction::Add { file, concurrency } => {
                cli::batch::run(file, *concurrency, fmt, verbose)
            }
        },
        Some(Commands::Sync { .. }) => {
            eprintln!("sync command is not yet implemented");
            Ok(())
        }
        None => {
            let target = cli.target.as_deref().unwrap_or("");
            if target.is_empty() {
                Cli::parse_from(["sk", "--help"]);
                let marker = "# sk completion";
                if !check_completion_installed(marker) && !cli.json {
                    eprintln!("\n💡 Tip: Run 'sk completion install' to enable Tab completion for server names.");
                }
                process::exit(0);
            }
            cli::connect::run(Some(target), fmt, verbose)
        }
    };

    if let Err(e) = result {
        eprintln!("❌ {}", e);
        process::exit(e.exit_code());
    }
}

/// 检查 shell 补全是否已安装（通过检查 rc 文件中是否有标记）
fn check_completion_installed(marker: &str) -> bool {
    let rc_files = if cfg!(windows) {
        std::env::var("PROFILE").map(|p| vec![p.into()]).unwrap_or_default()
    } else {
        let home = dirs::home_dir();
        let mut files = Vec::new();
        if let Some(h) = &home {
            files.push(h.join(".bashrc"));
            files.push(h.join(".zshrc"));
            files.push(h.join(".config/fish/config.fish"));
        }
        files
    };
    rc_files.iter().any(|f| fs::read_to_string(f).map(|c| c.contains(marker)).unwrap_or(false))
}
