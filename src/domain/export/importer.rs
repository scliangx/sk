//! SSH config 导入器
//!
//! 解析现有 ~/.ssh/config 中的 Host 块，提取为 sk 可管理的结构。


use crate::domain::config::model::Server;
use crate::domain::config::parser::SshConfigParser;
use crate::error::SkResult;

/// 导入结果
#[derive(Debug)]
pub struct ImportResult {
    /// 成功解析的服务器
    pub servers: Vec<Server>,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// SSH 配置导入器
pub struct Importer;

impl Importer {
    /// 从指定文件导入 SSH 配置
    ///
    /// 返回解析出的所有有效 Host 块。
    pub fn import_file(path: &std::path::Path) -> SkResult<ImportResult> {
        let parser = SshConfigParser::new(path.to_path_buf());
        let (servers, warnings) = parser.parse_str(
            &crate::infra::fs::read_file(path).unwrap_or_default(),
        );

        Ok(ImportResult {
            servers,
            warnings: warnings.iter().map(|w| format!("L{}: {}", w.line, w.message)).collect(),
        })
    }

    /// 从默认路径 ~/.ssh/config 导入
    pub fn import_default() -> SkResult<ImportResult> {
        let path = crate::infra::fs::ssh_config_path()?;
        if !path.exists() {
            return Ok(ImportResult {
                servers: vec![],
                warnings: vec!["SSH config file not found.".to_string()],
            });
        }
        Self::import_file(&path)
    }

    /// 将导入的服务器批量写入 sk 管理的配置
    ///
    /// 跳过已存在的同名服务器。
    pub fn add_to_managed(servers: &[Server], force: bool) -> SkResult<(usize, usize)> {
        let writer = crate::domain::config::writer::SshConfigWriter::default_path()?;
        let mut meta = crate::domain::config::metadata::MetadataManager::load_default()?;
        let mut added = 0;
        let mut skipped = 0;

        for server in servers {
            if writer.exists(&server.name)? {
                if force {
                    writer.update(&server.name, server)?;
                    added += 1;
                } else {
                    skipped += 1;
                }
            } else {
                writer.append(server)?;
                meta.upsert_server(&server.name, false, "none");
                added += 1;
            }
        }

        meta.save()?;
        Ok((added, skipped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_import_from_string() {
        let config = "Host webserver\n    HostName example.com\n    User admin\n    Port 2222\n";
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_config");
        std::fs::write(&path, config).unwrap();

        let result = Importer::import_file(&path).unwrap();
        assert_eq!(result.servers.len(), 1);
        assert_eq!(result.servers[0].name, "webserver");
        assert_eq!(result.servers[0].port, 2222);
    }

    #[test]
    fn test_import_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty_config");
        std::fs::write(&path, "").unwrap();

        let result = Importer::import_file(&path).unwrap();
        assert_eq!(result.servers.len(), 0);
    }
}
