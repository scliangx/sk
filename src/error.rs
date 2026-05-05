//! 统一错误类型与退出码映射
//!
//! 定义了 sk 工具所有可能的错误类型，以及对应的进程退出码。
//! 退出码语义与设计文档完全对齐。

use std::path::PathBuf;

/// sk 的统一错误类型
///
/// 每个变体对应一个明确的退出码，便于脚本化使用。
#[derive(Debug, thiserror::Error)]
pub enum SkError {
    /// 网络连接失败（退出码 1）
    #[error("Connection failed: {0}")]
    Network(String),

    /// SSH 认证失败（退出码 2）
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// 文件写入失败（退出码 3）
    #[error("Unable to write to {path}: {reason}")]
    FileWrite {
        /// 目标文件路径
        path: PathBuf,
        /// 失败原因
        reason: String,
    },

    /// 参数校验失败（退出码 4）
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// 依赖缺失（exit code 5）
    #[allow(dead_code)]
    #[error("Missing dependency: {0}")]
    MissingDependency(String),

    /// 配置解析失败（退出码 6）
    #[error("Configuration error: {0}")]
    Config(String),

    /// 密钥操作失败（退出码 7）
    #[error("Key operation failed: {0}")]
    KeyOperation(String),

    /// 密码存储操作失败（退出码 8）
    #[error("Password store failed: {0}")]
    PasswordStore(String),

    /// 内部错误（退出码 99）
    #[error("Internal error: {0}")]
    Internal(String),
}

impl SkError {
    /// 将错误映射到进程退出码
    ///
    /// 退出码含义：
    /// - 0: 成功
    /// - 1: 网络错误
    /// - 2: 认证错误
    /// - 3: 文件写入错误
    /// - 4: 参数错误
    /// - 5: 依赖缺失
    /// - 6: 配置错误
    /// - 7: 密钥操作失败
    /// - 8: 密码存储失败
    /// - 99: 内部/未知错误
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Network(_) => 1,
            Self::Auth(_) => 2,
            Self::FileWrite { .. } => 3,
            Self::InvalidArgument(_) => 4,
            Self::MissingDependency(_) => 5,
            Self::Config(_) => 6,
            Self::KeyOperation(_) => 7,
            Self::PasswordStore(_) => 8,
            Self::Internal(_) => 99,
        }
    }
}

/// 统一的 Result 类型别名
pub type SkResult<T> = Result<T, SkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error_exit_code() {
        let err = SkError::Network("timeout".to_string());
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn test_auth_error_exit_code() {
        let err = SkError::Auth("bad password".to_string());
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn test_file_write_error_exit_code() {
        let err = SkError::FileWrite {
            path: PathBuf::from("/tmp/test"),
            reason: "permission denied".to_string(),
        };
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn test_invalid_argument_error_exit_code() {
        let err = SkError::InvalidArgument("port must be 1-65535".to_string());
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn test_missing_dependency_error_exit_code() {
        let err = SkError::MissingDependency("ssh-keygen not found".to_string());
        assert_eq!(err.exit_code(), 5);
    }

    #[test]
    fn test_config_error_exit_code() {
        let err = SkError::Config("invalid config format".to_string());
        assert_eq!(err.exit_code(), 6);
    }

    #[test]
    fn test_key_operation_error_exit_code() {
        let err = SkError::KeyOperation("key generation failed".to_string());
        assert_eq!(err.exit_code(), 7);
    }

    #[test]
    fn test_password_store_error_exit_code() {
        let err = SkError::PasswordStore("keychain unavailable".to_string());
        assert_eq!(err.exit_code(), 8);
    }

    #[test]
    fn test_internal_error_exit_code() {
        let err = SkError::Internal("unexpected state".to_string());
        assert_eq!(err.exit_code(), 99);
    }

    #[test]
    fn test_error_display_format() {
        let err = SkError::Network("connection refused".to_string());
        assert_eq!(format!("{}", err), "Connection failed: connection refused");
    }
}
