//! 系统钥匙串后端
//!
//! 仅在编译时开启了 keyring-backend feature 时可用。
//! 使用 keyring crate 封装平台差异：
//! - macOS: Keychain Services
//! - Windows: Credential Manager
//! - Linux: Secret Service (libdbus / gnome-keyring / kwallet)

use crate::domain::config::model::SecretString;
use crate::domain::password::store::PasswordStore;
use crate::error::{SkError, SkResult};

/// 服务名称，用于在钥匙串中标识 sk 的条目
const SERVICE_NAME: &str = "sk-ssh-manager";

/// 系统钥匙串存储后端
pub struct KeychainStore;

impl KeychainStore {
    /// 创建钥匙串存储后端
    pub fn new() -> Self {
        Self
    }
}

impl PasswordStore for KeychainStore {
    fn save(&self, name: &str, password: &SecretString) -> SkResult<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, name).map_err(|e| {
            SkError::PasswordStore(format!("Unable to access system keychain: {}", e))
        })?;

        entry
            .set_password(password.as_str())
            .map_err(|e| SkError::PasswordStore(format!("Unable to write to system keychain: {}", e)))?;

        Ok(())
    }

    fn get(&self, name: &str) -> SkResult<SecretString> {
        let entry = keyring::Entry::new(SERVICE_NAME, name).map_err(|e| {
            SkError::PasswordStore(format!("Unable to access system keychain: {}", e))
        })?;

        let password = entry
            .get_password()
            .map_err(|e| SkError::PasswordStore(format!("Unable to read password from system keychain: {}", e)))?;

        Ok(SecretString::new(password))
    }

    fn delete(&self, name: &str) -> SkResult<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, name).map_err(|e| {
            SkError::PasswordStore(format!("Unable to access system keychain: {}", e))
        })?;

        entry
            .delete_credential()
            .map_err(|e| SkError::PasswordStore(format!("Unable to delete password from system keychain: {}", e)))?;

        Ok(())
    }

    fn is_available(&self) -> bool {
        // 尝试创建一个测试条目来验证钥匙串可用性
        keyring::Entry::new(SERVICE_NAME, "__sk_probe__").is_ok()
    }

    fn backend_name(&self) -> &'static str {
        "keychain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keychain_store_backend_name() {
        let store = KeychainStore::new();
        assert_eq!(store.backend_name(), "keychain");
    }

    #[test]
    fn test_keychain_store_availability() {
        let store = KeychainStore::new();
        // 取决于平台，钥匙串可能可用也可能不可用
        // 只是在不应 panic
        let _ = store.is_available();
    }
}
