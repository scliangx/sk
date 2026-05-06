//! Shell 补全（clap-dyn-autocomplete）
//!
//! 动态补全：sk <Tab> → 服务器名称 / 子命令 / 参数
//!
//! sk completion install → 自动检测 shell 并安装

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap_dyn_autocomplete::{emit_completion_stub, Shell};

use crate::error::SkResult;

const MARKER: &str = "# sk completion";

pub fn run(shell: Option<&str>) -> SkResult<()> {
    match shell {
        None | Some("install") => install(),
        Some("uninstall") => uninstall(),
        Some("powershell") | Some("pwsh") => emit_stub(Shell::Powershell),
        Some("zsh") => emit_stub(Shell::Zsh),
        Some("fish") => emit_stub(Shell::Fish),
        Some("bash") => emit_bash(),
        Some("elvish") => emit_elvish(),
        _ => eprintln!("Supported: bash, zsh, fish, powershell, elvish, install, uninstall"),
    }
    Ok(())
}

fn emit_stub(shell: Shell) {
    emit_completion_stub(shell, "sk", "__complete", &mut std::io::stdout()).ok();
}

// ===== 安装/卸载 =====

fn install() {
    let (shell, rc_path) = detect_shell_rc();
    let mut buf = Vec::new();
    emit_completion_stub(shell.clone(), "sk", "__complete", &mut buf).ok();
    let script = String::from_utf8_lossy(&buf);

    if let Some(parent) = rc_path.parent() {
        if !parent.exists() {
            println!("Directory {} does not exist.", parent.display());
            println!("Create it first, or install manually:");
            println!("  sk completion {} | Out-String | Invoke-Expression  (current session)", shell_name_to_str(shell.clone()));
            return;
        }
    }

    if let Ok(existing) = fs::read_to_string(&rc_path) {
        if existing.contains(MARKER) {
            println!("✅ Completion already installed in {}", rc_path.display());
            return;
        }
    }

    let content = format!("\n{}\n{}\n", MARKER, script);
    match fs::OpenOptions::new().create(true).append(true).open(&rc_path) {
        Ok(mut f) => {
            let _ = f.write_all(content.as_bytes());
            println!("✅ Completion installed to {}", rc_path.display());
            println!("   Restart your shell or run: . {}", rc_path.display());
        }
        Err(e) => {
            eprintln!("❌ Cannot write: {}", e);
            println!("   Manual: sk completion powershell >> $PROFILE");
        }
    }
}

fn uninstall() {
    let rc_paths = get_rc_paths();
    let mut removed = false;
    for rc_path in &rc_paths {
        if let Ok(content) = fs::read_to_string(rc_path) {
            if content.contains(MARKER) {
                let cleaned: String = content.lines().take_while(|l| !l.contains(MARKER)).collect::<Vec<_>>().join("\n");
                if fs::write(rc_path, cleaned).is_ok() {
                    println!("✅ Removed from {}", rc_path.display());
                    removed = true;
                }
            }
        }
    }
    if !removed {
        eprintln!("No completion found in:");
        for p in &rc_paths { eprintln!("  - {}", p.display()); }
    }
}

fn detect_shell_rc() -> (Shell, PathBuf) {
    if std::env::var("PSModulePath").is_ok() || std::env::var("PROFILE").is_ok() {
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| {
            dirs::home_dir().map(|h| h.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1").display().to_string()).unwrap_or_default()
        });
        return (Shell::Powershell, PathBuf::from(profile));
    }
    if let Ok(s) = std::env::var("SHELL") {
        if s.contains("zsh") { return (Shell::Zsh, dirs::home_dir().unwrap_or_default().join(".zshrc")); }
        if s.contains("fish") { return (Shell::Fish, dirs::home_dir().unwrap_or_default().join(".config/fish/config.fish")); }
        if s.contains("bash") {
            println!("Bash auto-install not supported. Run: sk completion bash >> ~/.bashrc");
            std::process::exit(0);
        }
    }
    (Shell::Powershell, PathBuf::from("profile.ps1"))
}

fn get_rc_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("PROFILE") { v.push(p.into()); }
    if let Some(h) = dirs::home_dir() { v.push(h.join(".zshrc")); v.push(h.join(".config/fish/config.fish")); }
    v
}

fn shell_name_to_str(s: Shell) -> &'static str {
    match s { Shell::Powershell => "powershell", Shell::Zsh => "zsh", Shell::Fish => "fish" }
}

// ===== Bash (clap_complete + 动态补全) =====

fn emit_bash() {
    use clap::CommandFactory;
    use clap_complete::{generate, Shell as ClapShell};
    let mut cmd = crate::cli::args::Cli::command();
    generate(ClapShell::Bash, &mut cmd, "sk", &mut std::io::stdout());
    println!(r#"
_sk_servers() {{
    local cur="$1"
    local s=$(sk __complete-servers "$cur" 2>/dev/null | cut -f1)
    COMPREPLY=($(compgen -W "$s" -- "$cur"))
}}
complete -F _sk_servers -o default sk
"#);
}

fn emit_elvish() {
    use clap::CommandFactory;
    use clap_complete::{generate, Shell as ClapShell};
    let mut cmd = crate::cli::args::Cli::command();
    generate(ClapShell::Elvish, &mut cmd, "sk", &mut std::io::stdout());
}
