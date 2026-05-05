//! 安全字符串类型（重新导出）
//!
//! SecretString 定义在 domain::config::model 中，
//! 此模块重新导出以便 password 子模块使用。

// 重新导出（供其他模块使用，编译器可能标记为 unused，实际已被引用）
#[allow(unused_imports)]
pub use crate::domain::config::model::SecretString;
