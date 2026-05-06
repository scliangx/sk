//! 核心业务编排器

use std::path::PathBuf;

use crate::domain::config::metadata::MetadataManager;
use crate::domain::config::model::{Server, ServerStatus};
use crate::domain::config::store;
use crate::error::{SkError, SkResult};

pub struct Orchestrator;

impl Orchestrator {
    pub fn add_server(
        name: &str, host: &str, user: &str, port: u16,
        identity_file: Option<&str>, force: bool,
    ) -> SkResult<Server> {
        if name.trim().is_empty() || name.contains(char::is_whitespace) {
            return Err(SkError::InvalidArgument("Server name cannot be empty or contain spaces".into()));
        }
        if host.trim().is_empty() {
            return Err(SkError::InvalidArgument("Host address cannot be empty".into()));
        }
        if user.trim().is_empty() {
            return Err(SkError::InvalidArgument("Username cannot be empty".into()));
        }
        if port == 0 {
            return Err(SkError::InvalidArgument("Port must be 1-65535".into()));
        }

        let mut server = Server::new(name.to_string(), host.to_string(), user.to_string());
        server.port = port;
        if let Some(kp) = identity_file { server.identity_file = Some(PathBuf::from(kp)); }

        if store::exists(name)? && !force {
            return Err(SkError::Config(format!(
                "Server '{}' already exists. Use --force to overwrite.", name
            )));
        }

        store::add(&server)?;

        let mut meta = MetadataManager::load_default()?;
        meta.upsert_server(name, false, "none");
        meta.save()?;

        Ok(server)
    }

    pub fn remove_server(name: &str, delete_keys: bool) -> SkResult<(bool, bool)> {
        let removed = store::remove(name)?;

        let mut keys_deleted = false;
        if delete_keys {
            if let Ok(pk) = crate::infra::fs::server_key_path(name) { if pk.exists() { let _ = std::fs::remove_file(&pk); keys_deleted = true; } }
            if let Ok(pubk) = crate::infra::fs::server_pubkey_path(name) { if pubk.exists() { let _ = std::fs::remove_file(&pubk); } }
        }

        let pm = crate::domain::password::store::PasswordManager::new();
        let _ = pm.delete(name);

        let mut meta = MetadataManager::load_default()?;
        meta.remove_server(name);
        meta.save()?;

        Ok((removed, keys_deleted))
    }

    pub fn list_servers(filter: Option<&str>) -> SkResult<Vec<(Server, ServerStatus)>> {
        let servers = store::load_all()?;
        let meta = MetadataManager::load_default()?;

        Ok(servers.into_iter()
            .filter(|s| filter.map_or(true, |f| s.name.to_lowercase().contains(&f.to_lowercase())))
            .map(|mut s| {
                if let Some(m) = meta.get_server(&s.name) { s.password_stored = m.password_stored; }
                let status = Self::determine_status(&s);
                (s, status)
            })
            .collect())
    }

    fn determine_status(server: &Server) -> ServerStatus {
        if let Some(ref key) = server.identity_file {
            let expanded = crate::domain::config::parser::expand_tilde(&key.to_string_lossy());
            if std::path::Path::new(&expanded).exists() {
                return ServerStatus::KeyConfigured(std::path::PathBuf::from(expanded));
            }
        }
        if server.password_stored { ServerStatus::PasswordStored } else { ServerStatus::Bare }
    }

    pub fn get_server(name: &str) -> SkResult<Option<Server>> {
        let mut server = store::find(name)?;
        if let Some(ref mut s) = server {
            if let Ok(meta) = MetadataManager::load_default() {
                if let Some(m) = meta.get_server(name) { s.password_stored = m.password_stored; }
            }
        }
        Ok(server)
    }

    #[allow(dead_code)]
    pub fn check_name_available(name: &str) -> SkResult<bool> { Ok(!store::exists(name)?) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_add_invalid_name() { assert!(Orchestrator::add_server("", "h", "u", 22, None, false).is_err()); }
    #[test]
    fn test_add_invalid_host() { assert!(Orchestrator::add_server("n", "", "u", 22, None, false).is_err()); }
    #[test]
    fn test_add_invalid_user() { assert!(Orchestrator::add_server("n", "h", "", 22, None, false).is_err()); }
    #[test]
    fn test_add_invalid_port() { assert!(Orchestrator::add_server("n", "h", "u", 0, None, false).is_err()); }

    #[test]
    fn test_determine_status_bare() {
        assert_eq!(Orchestrator::determine_status(&Server::new("t".into(), "h".into(), "u".into())), ServerStatus::Bare);
    }

    #[test]
    fn test_determine_status_password() {
        let mut s = Server::new("t".into(), "h".into(), "u".into());
        s.password_stored = true;
        assert_eq!(Orchestrator::determine_status(&s), ServerStatus::PasswordStored);
    }

    #[test]
    fn test_check_name_available() {
        assert!(Orchestrator::check_name_available("very-unique-name-12345").is_ok());
    }

    #[test]
    fn test_determine_status_key() {
        let dir = TempDir::new().unwrap();
        let kp = dir.path().join("key");
        std::fs::write(&kp, "k").unwrap();
        let s = Server::new("t".into(), "h".into(), "u".into()).with_identity_file(kp.clone());
        assert_eq!(Orchestrator::determine_status(&s), ServerStatus::KeyConfigured(kp));
    }
}
