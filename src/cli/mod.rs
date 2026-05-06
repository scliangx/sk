//! CLI 层：命令参数解析与输出格式化
//!
//! 基于 clap v4 derive 模式构建，提供所有子命令的定义、
//! 参数校验、以及人类可读/JSON 格式的输出。

pub mod args;
pub mod add;
pub mod remove;
pub mod list;
pub mod test;
pub mod connect;
pub mod import_cmd;
pub mod export_cmd;
pub mod batch;
pub mod sync;
pub mod completion;
pub mod doctor;
pub mod custom_complete;
