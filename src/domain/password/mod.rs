//! 密码安全存储模块
//!
//! 提供跨平台密码存储能力：
//! - 系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）
//! - AES-256-GCM 加密文件降级方案
//! - SecretString：Zeroizing 包装，自动安全擦除

pub mod store;
pub mod secret;
#[cfg(feature = "keyring-backend")]
pub mod keychain;
pub mod file;
