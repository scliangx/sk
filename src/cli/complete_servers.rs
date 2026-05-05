//! 动态补全：返回服务器名称列表（供 shell 函数调用）
//!
//! 设计文档 12.1.2 节描述的内部接口：
//!   sk __complete-servers        → 输出所有服务器名称
//!   sk __complete-servers <prefix> → 过滤匹配前缀的名称
//!
//! 在 shell 补全函数中调用此命令获取候选列表。

use crate::app::orchestrator::Orchestrator;
use crate::domain::config::metadata::MetadataManager;
use crate::error::SkResult;

pub fn run(prefix: Option<&str>) -> SkResult<()> {
    let servers = Orchestrator::list_servers(None).unwrap_or_default();
    let meta = MetadataManager::load_default().unwrap_or_else(|_| {
        // 返回空元数据
        MetadataManager::load(
            &std::path::PathBuf::from("/nonexistent"),
        )
        .unwrap()
    });

    for (server, _status) in &servers {
        if let Some(p) = prefix {
            if !server.name.starts_with(p) {
                continue;
            }
        }
        // 输出格式：名称 + 描述（供 zsh _describe 使用）
        let desc = meta
            .get_server(&server.name)
            .map(|m| {
                if m.password_stored {
                    "stored-password"
                } else {
                    "configured"
                }
            })
            .unwrap_or("configured");

        println!("{}\t{}", server.name, desc);
    }

    Ok(())
}
