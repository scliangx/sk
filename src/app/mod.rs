//! Application 层：业务逻辑编排
//!
//! 负责编排各领域模块完成用户操作，管理事务与回滚。

pub mod orchestrator;
pub mod transaction;
