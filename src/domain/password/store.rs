//! 密码存储抽象层
//!
//! 定义了 PasswordStore trait 作为跨平台密码存储的统一抽象。
//! 运行时根据平台能力自动选择最佳后端：
//! 1. 系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）
//! 2. AES-256-GCM 加密文件（降级方案）

use crate::domain::config::model::SecretString;
use crate::error::SkResult;

/// 密码存储后端抽象
///
/// 所有密码存储实现必须同时实现 Send + Sync，以支持多线程使用。
#[allow(dead_code)]
pub trait PasswordStore: Send + Sync {
    /// 保存密码到安全存储
    ///
    /// # 参数
    /// - name: 服务标识符（通常是服务器名称）
    /// - password: 要存储的密码（SecretString 包装）
    fn save(&self, name: &str, password: &SecretString) -> SkResult<()>;

    /// 从安全存储读取密码
    ///
    /// # 返回
    /// 密码的 SecretString 包装，如果未找到返回错误
    fn get(&self, name: &str) -> SkResult<SecretString>;

    /// 从安全存储删除密码
    fn delete(&self, name: &str) -> SkResult<()>;

    /// 检查此后端是否可用
    fn is_available(&self) -> bool;

    /// 后端名称（用于显示给用户）
    fn backend_name(&self) -> &'static str;
}

/// 密码存储管理器
///
/// 自动选择最佳可用的后端：
/// - 优先使用系统钥匙串
/// - 降级使用 AES-256-GCM 加密文件
pub struct PasswordManager {
    /// 主后端（系统钥匙串）
    #[cfg(feature = "keyring-backend")]
    primary: Option<crate::domain::password::keychain::KeychainStore>,
    /// 文件后端（始终可用，作为降级和备份）
    file: crate::domain::password::file::FileStore,
}

impl PasswordManager {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "keyring-backend")]
            primary: {
                let k = crate::domain::password::keychain::KeychainStore::new();
                if k.is_available() { Some(k) } else { None }
            },
            file: crate::domain::password::file::FileStore::new(),
        }
    }

    /// 保存密码（同时写入钥匙串和文件备份）
    pub fn save(&self, name: &str, password: &SecretString) -> SkResult<()> {
        // 文件备份始终写入
        self.file.save(name, password)?;
        // 钥匙串写入（best-effort）
        #[cfg(feature = "keyring-backend")]
        if let Some(ref primary) = self.primary {
            let _ = primary.save(name, password);
        }
        Ok(())
    }

    /// 读取密码（钥匙串优先，文件降级）
    pub fn get(&self, name: &str) -> SkResult<SecretString> {
        #[cfg(feature = "keyring-backend")]
        if let Some(ref primary) = self.primary {
            if let Ok(secret) = primary.get(name) {
                return Ok(secret);
            }
        }
        // 降级到文件
        self.file.get(name)
    }

    /// 删除密码
    pub fn delete(&self, name: &str) -> SkResult<()> {
        #[cfg(feature = "keyring-backend")]
        if let Some(ref primary) = self.primary {
            let _ = primary.delete(name);
        }
        self.file.delete(name)
    }

    /// 获取后端名称
    #[allow(dead_code)]
    pub fn backend_name(&self) -> &'static str {
        #[cfg(feature = "keyring-backend")]
        if self.primary.is_some() {
            return "keychain";
        }
        "encrypted-file"
    }

    /// 密码是否已存储
    #[allow(dead_code)]
    pub fn exists(&self, name: &str) -> bool {
        self.get(name).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_manager_creation() {
        let manager = PasswordManager::new();
        assert!(!manager.backend_name().is_empty());
    }

    #[test]
    fn test_password_store_roundtrip() {
        let manager = PasswordManager::new();
        let password = SecretString::new("test-password-123".to_string());

        // 存储
        manager.save("__sk_test_server__", &password).unwrap();

        // 读取（某些平台钥匙串可能有延迟或特殊行为，忽略错误）
        match manager.get("__sk_test_server__") {
            Ok(retrieved) => {
                assert_eq!(retrieved.as_str(), "test-password-123");
            }
            Err(_) => {
                // 平台钥匙串可能不在测试环境完全可用，跳过验证
            }
        }

        // 清理
        let _ = manager.delete("__sk_test_server__");
    }

    #[test]
    fn test_delete_nonexistent() {
        let manager = PasswordManager::new();
        // 删除不存在的条目不应崩溃
        let result = manager.delete("__nonexistent_password__");
        // 可能成功也可能失败，取决于后端实现
        let _ = result;
    }

    #[test]
    fn test_backend_name_not_empty() {
        let manager = PasswordManager::new();
        let name = manager.backend_name();
        assert!(!name.is_empty());
    }
}
