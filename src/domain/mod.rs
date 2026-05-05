//! Domain 层：各领域模块的纯逻辑，不依赖 CLI 层
//!
//! 包含以下子模块：
//! - config: SSH 配置管理（解析、写入、元数据）
//! - key:    密钥管理（ED25519 生成）
//! - ssh:    SSH 操作（连接、认证、公钥推送）
//! - password: 密码安全存储（钥匙串/加密文件）
//! - export: 配置导入导出

pub mod config;
pub mod key;
pub mod ssh;
pub mod password;
pub mod export;
