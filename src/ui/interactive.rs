//! 交互式用户界面
//!
//! 基于 dialoguer 库提供交互式输入能力：
//! - 二次确认提示
//! - 密码安全输入
//! - 服务器列表选择（用于 connect 命令）

use dialoguer::{Confirm, Password, Select};

use crate::domain::config::model::SecretString;

/// 交互式 UI 管理器
#[allow(dead_code)]
pub struct Interactive;

#[allow(dead_code)]
impl Interactive {
    /// 二次确认：询问用户是否继续
    ///
    /// 返回 true 表示用户确认。
    pub fn confirm(message: &str) -> bool {
        Confirm::new()
            .with_prompt(message)
            .default(false)
            .interact()
            .unwrap_or(false)
    }

    /// 二次确认（默认答案为是）
    pub fn confirm_yes(message: &str) -> bool {
        Confirm::new()
            .with_prompt(message)
            .default(true)
            .interact()
            .unwrap_or(false)
    }

    /// 安全读取密码
    ///
    /// 密码输入时不回显，立即封装为 SecretString 以保护内存安全。
    pub fn read_password(prompt: &str) -> Option<SecretString> {
        Password::new()
            .with_prompt(prompt)
            .interact()
            .ok()
            .map(SecretString::new)
    }

    /// 安全读取密码（带确认）
    ///
    /// 要求用户输入两次密码，确保一致。
    #[allow(dead_code)]
    pub fn read_password_with_confirm(prompt: &str) -> Option<SecretString> {
        let password = Password::new()
            .with_prompt(prompt)
            .interact()
            .ok()?;

        let confirm = Password::new()
            .with_prompt("Please confirm your password")
            .interact()
            .ok()?;

        if password == confirm {
            Some(SecretString::new(password))
        } else {
            eprintln!("Passwords do not match. Please try again.");
            None
        }
    }

    /// 从列表中选择一项（返回索引）
    ///
    /// 用于 connect 命令的服务器选择。
    pub fn select_item(prompt: &str, items: &[String]) -> Option<usize> {
        if items.is_empty() {
            eprintln!("No items to select.");
            return None;
        }

        Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(0)
            .interact()
            .ok()
    }

    /// 询问是否强制覆盖已有配置
    #[allow(dead_code)]
    pub fn confirm_overwrite(name: &str) -> bool {
        let message = format!(
            "Server '{}' already exists. Overwrite? (This will update the existing configuration)",
            name
        );
        Self::confirm(&message)
    }

    /// 询问是否删除密钥文件
    pub fn confirm_delete_keys(name: &str) -> bool {
        let message = format!("Also delete key files for server '{}'?", name);
        Self::confirm_yes(&message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_empty_list() {
        let result = Interactive::select_item("Select", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_secret_string_from_password() {
        // 测试 SecretString 的创建和基本行为
        let secret = SecretString::new("test123".to_string());
        assert!(!secret.is_empty());
        assert_eq!(secret.as_str(), "test123");
    }
}
