//! 跨平台文件操作
//!
//! 提供 SSH 目录路径管理、文件锁、权限设置等底层能力。
//! 所有路径操作均处理 Unix/Windows 差异。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::{SkError, SkResult};

/// 获取用户 SSH 目录路径 ~/.ssh
///
/// # 跨平台行为
/// - Unix: ~/.ssh
/// - Windows: ~/.ssh（OpenSSH for Windows 使用此路径）
pub fn ssh_dir() -> SkResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| SkError::Internal("Cannot determine user HOME directory".to_string()))?;
    Ok(home.join(".ssh"))
}

/// 获取 SSH config 文件路径 ~/.ssh/config
pub fn ssh_config_path() -> SkResult<PathBuf> {
    Ok(ssh_dir()?.join("config"))
}

/// 获取 sk 数据目录 ~/.sk/
pub fn sk_dir() -> SkResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| SkError::Internal("Cannot determine user HOME directory".to_string()))?;
    Ok(home.join(".sk"))
}

/// 获取 sk 元数据文件路径 ~/.ssh/sk/metadata.yaml
pub fn sk_metadata_path() -> SkResult<PathBuf> {
    Ok(sk_dir()?.join("metadata.yaml"))
}

/// 获取 sk 密码存储目录 ~/.ssh/sk/passwords/
pub fn sk_passwords_dir() -> SkResult<PathBuf> {
    Ok(sk_dir()?.join("passwords"))
}

/// 获取指定服务器的加密密码文件路径 ~/.ssh/sk/passwords/{name}.enc
pub fn sk_password_file_path(name: &str) -> SkResult<PathBuf> {
    Ok(sk_passwords_dir()?.join(format!("{}.enc", name)))
}

/// 获取 sk 密钥存储目录 ~/.ssh/sk/keys/
pub fn sk_keys_dir() -> SkResult<PathBuf> {
    Ok(sk_dir()?.join("keys"))
}

/// 获取指定服务器的密钥文件路径 ~/.ssh/sk/keys/{name}_key
pub fn server_key_path(name: &str) -> SkResult<PathBuf> {
    Ok(sk_keys_dir()?.join(format!("{}_key", name)))
}

/// 获取指定服务器的公钥文件路径 ~/.ssh/sk/keys/{name}_key.pub
pub fn server_pubkey_path(name: &str) -> SkResult<PathBuf> {
    Ok(sk_keys_dir()?.join(format!("{}_key.pub", name)))
}

/// 确保目录存在，如果不存在则创建
///
/// 同时设置正确的目录权限（Unix: 700）。
pub fn ensure_dir(path: &Path) -> SkResult<()> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|e| SkError::FileWrite {
            path: path.to_path_buf(),
            reason: format!("Cannot create directory: {}", e),
        })?;

        // 设置目录权限为 700（仅 Unix 生效）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)
                .map_err(|e| SkError::FileWrite {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })?
                .permissions();
            perms.set_mode(0o700);
            fs::set_permissions(path, perms).map_err(|e| SkError::FileWrite {
                path: path.to_path_buf(),
                reason: format!("Cannot set directory permissions: {}", e),
            })?;
        }
    }
    Ok(())
}

/// 设置文件权限为 600（仅所有者可读写）
///
/// # 跨平台行为
/// - Unix: 设置 mode 为 0o600
/// - Windows: 此操作无效，给出 info 级别提示（不阻塞流程）
pub fn set_permissions_600(path: &Path) -> SkResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| SkError::FileWrite {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| SkError::FileWrite {
            path: path.to_path_buf(),
            reason: format!("Cannot set file permissions: {}", e),
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// 原子写入文件
///
/// 先写入临时文件，成功后再 rename 到目标路径，确保写入的原子性。
/// 写入完成后自动设置文件权限为 600（Unix）或给出警告。
pub fn atomic_write(path: &Path, content: &str) -> SkResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_dir(parent)?;

    // 写入临时文件
    let tmp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp_path).map_err(|e| SkError::FileWrite {
        path: tmp_path.clone(),
        reason: format!("Cannot create temp file: {}", e),
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| SkError::FileWrite {
            path: tmp_path.clone(),
            reason: format!("Write failed: {}", e),
        })?;

    file.flush().map_err(|e| SkError::FileWrite {
        path: tmp_path.clone(),
        reason: format!("Flush failed: {}", e),
    })?;

    // 原子 rename
    fs::rename(&tmp_path, path).map_err(|e| SkError::FileWrite {
        path: path.to_path_buf(),
        reason: format!("Cannot move to target path: {}", e),
    })?;

    // 设置权限为 600
    set_permissions_600(path)?;

    Ok(())
}

/// 文件锁
///
/// 使用简单的临时锁文件实现，跨平台兼容。
/// 写入前获取锁，写入后释放。
pub struct FileLock {
    lock_path: PathBuf,
}

impl FileLock {
    /// 创建文件锁（不获取）
    pub fn new(target_path: &Path) -> Self {
        let lock_path = target_path.with_extension("lock");
        Self { lock_path }
    }

    /// 尝试获取锁
    ///
    /// 通过创建锁文件实现。如果锁文件已存在，检查是否过期（超过 30 秒）。
    pub fn acquire(&self) -> SkResult<()> {
        // 检查是否有过期锁
        if self.lock_path.exists() {
            if let Ok(metadata) = fs::metadata(&self.lock_path) {
                if let Ok(modified) = metadata.modified() {
                    let elapsed = modified
                        .elapsed()
                        .unwrap_or(std::time::Duration::from_secs(0));
                    // 超过 30 秒认为锁已过期，删除
                    if elapsed > std::time::Duration::from_secs(30) {
                        let _ = fs::remove_file(&self.lock_path);
                    } else {
                        return Err(SkError::FileWrite {
                            path: self.lock_path.clone(),
                            reason: "File is locked by another process. Please try again later.".to_string(),
                        });
                    }
                }
            }
        }

        // 创建锁文件
        let pid = std::process::id();
        fs::write(&self.lock_path, pid.to_string()).map_err(|e| {
            SkError::FileWrite {
                path: self.lock_path.clone(),
                reason: format!("Cannot create lock file: {}", e),
            }
        })?;

        Ok(())
    }

    /// 释放锁
    pub fn release(&self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        self.release();
    }
}

/// 检查路径是否存在且为文件
#[allow(dead_code)]
pub fn is_file(path: &Path) -> bool {
    path.is_file()
}

/// 检查路径是否存在且为目录
#[allow(dead_code)]
pub fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

/// 读取文件内容为字符串
pub fn read_file(path: &Path) -> SkResult<String> {
    fs::read_to_string(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => SkError::Config(format!(
            "File not found: {}. Use 'sk add' to add a server.",
            path.display()
        )),
        _ => SkError::FileWrite {
            path: path.to_path_buf(),
            reason: format!("Failed to read file: {}", e),
        },
    })
}

/// 初始化 SSH 环境
///
/// 确保 ~/.ssh/ 和 ~/.sk/ 子目录结构存在。
pub fn init_ssh_env() -> SkResult<()> {
    ensure_dir(&ssh_dir()?)?;
    ensure_dir(&sk_dir()?)?;
    ensure_dir(&sk_keys_dir()?)?;
    ensure_dir(&sk_passwords_dir()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_ssh_dir_returns_valid_path() {
        let path = ssh_dir().unwrap();
        assert!(path.ends_with(".ssh"));
    }

    #[test]
    fn test_ssh_config_path_ends_with_config() {
        let path = ssh_config_path().unwrap();
        assert!(path.ends_with("config"));
    }

    #[test]
    fn test_server_key_path_naming() {
        let path = server_key_path("myserver").unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "myserver_key");
    }

    #[test]
    fn test_server_pubkey_path_naming() {
        let path = server_pubkey_path("myserver").unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "myserver_key.pub");
    }

    #[test]
    fn test_ensure_dir_creates_directory() {
        let dir = TempDir::new().unwrap();
        let new_dir = dir.path().join("test_subdir");
        assert!(!new_dir.exists());
        ensure_dir(&new_dir).unwrap();
        assert!(new_dir.exists());
        assert!(new_dir.is_dir());
    }

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        let content = "Hello, World!";

        atomic_write(&file_path, content).unwrap();
        assert!(file_path.exists());

        let read_back = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("overwrite.txt");

        atomic_write(&file_path, "original").unwrap();
        atomic_write(&file_path, "updated").unwrap();

        let read_back = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_back, "updated");
    }

    #[test]
    fn test_file_lock_acquire_and_release() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("data.txt");

        {
            let lock = FileLock::new(&target);
            lock.acquire().unwrap();
            assert!(dir.path().join("data.lock").exists());
        }
        // lock dropped, should be released
        assert!(!dir.path().join("data.lock").exists());
    }

    #[test]
    fn test_is_file_and_is_dir() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        assert!(is_file(&file_path));
        assert!(!is_dir(&file_path));
        assert!(is_dir(dir.path()));
        assert!(!is_file(dir.path()));
    }

    #[test]
    fn test_read_file_success() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("readme.txt");
        fs::write(&file_path, "content here").unwrap();

        assert_eq!(read_file(&file_path).unwrap(), "content here");
    }

    #[test]
    fn test_read_file_not_found() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("nonexistent.txt");
        let result = read_file(&file_path);
        assert!(result.is_err());
    }
}
