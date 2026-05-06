//! SSH 配置管理模块
//!
//! 负责 ~/.ssh/config 和 sk.yaml 元数据的读写操作。
//! 支持 Host 块的解析、追加、删除、更新。

pub mod model;
pub mod store;
pub mod parser;
pub mod writer;
pub mod metadata;
