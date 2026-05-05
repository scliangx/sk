//! 进度条封装
//!
//! 基于 indicatif 库提供统一的进度显示能力。
//! 仅在终端为交互式 TTY 时显示进度条（管道或重定向时自动禁用）。

use indicatif::{ProgressBar, ProgressStyle};

/// 创建旋转式进度条（用于不确定耗时的操作）
///
/// 适用场景：
/// - TCP 连接测试
/// - 密钥推送到远程服务器
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// 创建带有明确步数的进度条
///
/// 适用场景：
/// - 密钥生成
/// - 批量操作
#[allow(dead_code)]
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_spinner_does_not_panic() {
        let spinner = create_spinner("Testing spinner...");
        spinner.finish_and_clear();
    }

    #[test]
    fn test_create_progress_bar_does_not_panic() {
        let pb = create_progress_bar(100, "Testing progress...");
        pb.inc(50);
        pb.finish_and_clear();
    }
}
