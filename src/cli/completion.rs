//! Shell 自动补全 — 生成脚本 + 一键安装
//!
//! `sk completion`      → 自动检测 shell 并安装
//! `sk completion bash` → 输出 bash 补全脚本

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::args::Cli;
use crate::error::SkResult;

pub fn run(shell: Option<&str>) -> SkResult<()> {
    match shell {
        None | Some("install") => install(),
        Some("uninstall") => uninstall(),
        Some("bash") => completion_bash(),
        Some("zsh") => completion_zsh(),
        Some("fish") => completion_fish(),
        Some("powershell") | Some("pwsh") => completion_powershell(),
        _ => {
            eprintln!("Unsupported shell. Use: bash, zsh, fish, powershell, install, uninstall");
        }
    }
    Ok(())
}

// ===== 自动安装 =====

fn install() {
    let shell = detect_shell();
    let (script, rc_path) = match shell.as_str() {
        "powershell" | "pwsh" => {
            let script = build_powershell_script();
            let profile = std::env::var("PROFILE").unwrap_or_else(|_| {
                let home = dirs::home_dir().map(|p| p.display().to_string()).unwrap_or_default();
                format!("{}\\Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1", home)
            });
            (script, PathBuf::from(profile))
        }
        "bash" => {
            let rc = dirs::home_dir().map(|h| h.join(".bashrc")).unwrap_or_else(|| PathBuf::from(".bashrc"));
            (build_bash_script(), rc)
        }
        "zsh" => {
            let rc = dirs::home_dir().map(|h| h.join(".zshrc")).unwrap_or_else(|| PathBuf::from(".zshrc"));
            (build_zsh_script(), rc)
        }
        "fish" => {
            let rc = dirs::home_dir()
                .map(|h| h.join(".config/fish/config.fish"))
                .unwrap_or_else(|| PathBuf::from("config.fish"));
            (build_fish_script(), rc)
        }
        _ => {
            eprintln!("Could not detect shell. Specify manually: sk completion <bash|zsh|fish|powershell>");
            return;
        }
    };

    // 检查是否已安装
    let marker = "# sk completion";
    if let Ok(existing) = fs::read_to_string(&rc_path) {
        if existing.contains(marker) {
            println!("✅ Completion already installed in {}", rc_path.display());
            return;
        }
    }

    // 检查父目录是否存在
    if let Some(parent) = rc_path.parent() {
        if !parent.exists() {
            println!("The directory {} does not exist.", parent.display());
            if shell == "powershell" {
                println!("PowerShell profile directory not found — PowerShell may not have been started yet.");
                println!("Open PowerShell once to create it, or install manually:");
            } else {
                println!("Create it first (mkdir -p {}), or install manually:", parent.display());
            }
            println!("  sk completion {} | Out-String | Invoke-Expression  (current session only)", shell);
            println!("  sk completion {}  (print script)", shell);
            return;
        }
    }

    // 追加到 rc 文件
    let content = format!("\n{}\n{}\n", marker, script);
    match fs::OpenOptions::new().create(true).append(true).open(&rc_path) {
        Ok(mut f) => {
            let _ = f.write_all(content.as_bytes());
            println!("✅ Completion installed to {}", rc_path.display());
            println!("   Restart your shell or run: . {}", rc_path.display());
        }
        Err(e) => {
            eprintln!("❌ Cannot write to {}: {}", rc_path.display(), e);
            println!("   Install manually: sk completion {} > {}  (append to your rc file)", shell, rc_path.display());
        }
    }
}

fn detect_shell() -> String {
    // PowerShell detection (works cross-platform)
    if std::env::var("PSModulePath").is_ok()
        || std::env::var("PROFILE").is_ok()
        || std::env::var("POWERSHELL_VERSION").is_ok()
    {
        return "powershell".into();
    }
    // Unix shell detection
    if let Ok(s) = std::env::var("SHELL") {
        if s.contains("zsh") { return "zsh".into(); }
        if s.contains("fish") { return "fish".into(); }
        if s.contains("bash") { return "bash".into(); }
    }
    // Default per platform
    if cfg!(windows) { "powershell" } else { "bash" }.into()
}

// ===== 卸载 =====

const MARKER: &str = "# sk completion";

fn uninstall() {
    let rc_paths = get_rc_paths();
    let mut removed = false;

    for rc_path in &rc_paths {
        match fs::read_to_string(rc_path) {
            Ok(content) if content.contains(MARKER) => {
                let cleaned: String = content
                    .lines()
                    .take_while(|line| !line.contains(MARKER))
                    .collect::<Vec<_>>()
                    .join("\n");

                if let Err(e) = fs::write(rc_path, cleaned) {
                    eprintln!("❌ Cannot update {}: {}", rc_path.display(), e);
                } else {
                    println!("✅ Completion removed from {}", rc_path.display());
                    removed = true;
                }
            }
            _ => {}
        }
    }

    if !removed {
        eprintln!("No sk completion found in checked files:");
        for p in &rc_paths {
            eprintln!("  - {}", p.display());
        }
    }
}

fn get_rc_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let home = dirs::home_dir();

    if cfg!(windows) {
        if let Ok(p) = std::env::var("PROFILE") { paths.push(p.into()); }
    }
    if let Some(h) = &home {
        paths.push(h.join(".bashrc"));
        paths.push(h.join(".zshrc"));
        paths.push(h.join(".config/fish/config.fish"));
    }
    paths
}

// ===== 各 shell 补全脚本 =====

fn build_bash_script() -> String {
    let mut buf = Vec::new();
    write_clap_completion(Shell::Bash, &mut buf);
    let base = String::from_utf8_lossy(&buf);
    format!("{}\n{}\n{}", base,
        r#"_sk_servers() { local cur="$1"; local s; s=$(sk __complete-servers "$cur" 2>/dev/null | cut -f1); COMPREPLY=($(compgen -W "$s" -- "$cur")); }"#,
        r#"complete -F _sk_servers -o default sk"#)
}

fn build_zsh_script() -> String {
    let mut buf = Vec::new();
    write_clap_completion(Shell::Zsh, &mut buf);
    let base = String::from_utf8_lossy(&buf);
    format!("{}\n{}\n{}", base,
        r#"_sk_servers() { local -a s; s=(${(f)"$(sk __complete-servers 2>/dev/null | cut -f1)"}); _describe 'server' s; }"#,
        r#"compdef _sk_servers sk 2>/dev/null"#)
}

fn build_fish_script() -> String {
    let mut buf = Vec::new();
    write_clap_completion(Shell::Fish, &mut buf);
    let base = String::from_utf8_lossy(&buf);
    format!("{}\n{}", base,
        r#"complete -c sk -n "not __fish_seen_subcommand_from add remove list test import export batch sync completion doctor" -a "(sk __complete-servers 2>/dev/null | cut -f1)" -d Server"#)
}

fn build_powershell_script() -> String {
    let mut buf = Vec::new();
    write_clap_completion(Shell::PowerShell, &mut buf);
    let base = String::from_utf8_lossy(&buf);
    format!("{}\n{}", base, r#"
Register-ArgumentCompleter -CommandName sk -ScriptBlock {
    param($wordToComplete, $commandAst)
    $subs = @('add','a','remove','rm','list','ls','test','t','import','export','batch','sync','completion','doctor')
    if ($commandAst.CommandElements.Count -gt 1) {
        if ($subs -contains $commandAst.CommandElements[1].Value) { return }
    }
    sk __complete-servers $wordToComplete 2>$null | ForEach-Object {
        $p = $_ -split "\t"
        [System.Management.Automation.CompletionResult]::new($p[0], $p[0], 'ParameterValue', $p[1])
    }
}
"#)
}

// ===== 单独输出 =====

fn completion_bash()   { println!("{}", build_bash_script()); }
fn completion_zsh()    { println!("{}", build_zsh_script()); }
fn completion_fish()   { println!("{}", build_fish_script()); }
fn completion_powershell() { println!("{}", build_powershell_script()); }

fn write_clap_completion(shell: Shell, w: &mut dyn Write) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, &name, w);
}
