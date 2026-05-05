//! AES-256-GCM 加密文件降级后端
//!
//! 当系统钥匙串不可用时（如无桌面的 Linux 服务器），
//! 使用 AES-256-GCM 加密将密码存储在文件中。
//!
//! 安全设计（纵深防御）：
//! - 密钥派生：Argon2id(19MB, 3轮) + 多指纹盐值 → 256-bit key
//!   盐值 = SHA256(hostname|username|HOME|OS机器ID|固定盐)
//! - 加密：AES-256-GCM（认证加密 + 随机 nonce）
//! - 文件路径：~/.ssh/sk/passwords/{name}.enc
//! - 文件权限：0o600
//! - 机器绑定：复制到其他机器无法解密

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Argon2;
use sha2::Digest;

use crate::domain::config::model::SecretString;
use crate::domain::password::store::PasswordStore;
use crate::error::{SkError, SkResult};
use crate::infra::fs;

/// 加密数据的前缀（magic bytes），用于验证文件格式
const FILE_MAGIC: &[u8; 4] = b"SKV1";

/// AES-256-GCM 加密文件后端
pub struct FileStore;

impl FileStore {
    /// 创建新的文件存储后端
    pub fn new() -> Self {
        Self
    }

    /// 从机器指纹派生加密密钥
    ///
    /// 使用 Argon2id（高内存成本）进行密钥派生，
    /// 结合 hostname、用户名、HOME 路径、OS 机器 ID 等多个系统指纹作为盐值。
    fn derive_key() -> [u8; 32] {
        let machine_id = get_machine_identifier();
        let salt = sha2::Sha256::digest(machine_id.as_bytes());

        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19456, 3, 1, Some(32))  // 19MB memory, 3 iterations
                .expect("Argon2id params valid"),
        );

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(b"sk-master-key-v2", &salt, &mut key)
            .expect("Argon2id key derivation should not fail");

        key
    }

    /// 加密单个密码值
    fn encrypt_password(value: &str) -> SkResult<Vec<u8>> {
        let key_bytes = Self::derive_key();
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // 生成随机 nonce
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        // 加密
        let ciphertext = cipher
            .encrypt(&nonce, value.as_bytes())
            .map_err(|e| SkError::PasswordStore(format!("Password encryption failed: {}", e)))?;

        // 格式：magic (4) || nonce (12) || ciphertext
        let mut result = Vec::with_capacity(4 + 12 + ciphertext.len());
        result.extend_from_slice(FILE_MAGIC);
        result.extend_from_slice(nonce.as_slice());
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// 解密密码值
    fn decrypt_password(data: &[u8]) -> SkResult<String> {
        // 解析 magic
        if data.len() < 16 {
            return Err(SkError::PasswordStore("Invalid encrypted data format".to_string()));
        }

        let magic = &data[..4];
        if magic != FILE_MAGIC {
            return Err(SkError::PasswordStore(
                "Encrypted data format mismatch, file may be corrupted".to_string(),
            ));
        }

        let nonce_bytes = &data[4..16];
        let ciphertext = &data[16..];

        let key_bytes = Self::derive_key();
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SkError::PasswordStore("Password decryption failed, data may be corrupted or key mismatch".to_string()))?;

        String::from_utf8(plaintext)
            .map_err(|e| SkError::PasswordStore(format!("Invalid password data encoding: {}", e)))
    }
}

impl PasswordStore for FileStore {
    fn save(&self, name: &str, password: &SecretString) -> SkResult<()> {
        let encrypted = Self::encrypt_password(password.as_str())?;
        let file_path = fs::sk_password_file_path(name)?;
        fs::ensure_dir(file_path.parent().unwrap())?;
        let hex_data = hex::encode(&encrypted);
        fs::atomic_write(&file_path, &hex_data)?;
        Ok(())
    }

    fn get(&self, name: &str) -> SkResult<SecretString> {
        let file_path = fs::sk_password_file_path(name)?;
        let hex_data = fs::read_file(&file_path)?;
        let encrypted = hex::decode(hex_data.trim())
            .map_err(|e| SkError::PasswordStore(format!("Invalid password file format: {}", e)))?;
        let plaintext = Self::decrypt_password(&encrypted)?;
        Ok(SecretString::new(plaintext))
    }

    fn delete(&self, name: &str) -> SkResult<()> {
        let file_path = fs::sk_password_file_path(name)?;
        if file_path.exists() {
            std::fs::remove_file(&file_path).map_err(|e| SkError::FileWrite {
                path: file_path,
                reason: format!("Failed to delete password file: {}", e),
            })?;
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        true // 文件后端始终可用
    }

    fn backend_name(&self) -> &'static str {
        "encrypted-file"
    }
}

/// 获取机器标识符（用于密钥派生）
///
/// 收集多个系统指纹组合成唯一种子，确保密钥与当前机器绑定。
/// 即使文件被复制到其他机器也无法解密。
fn get_machine_identifier() -> String {
    let mut parts: Vec<String> = Vec::new();

    // 1. 主机名
    if let Ok(h) = std::process::Command::new("hostname").output() {
        parts.push(String::from_utf8_lossy(&h.stdout).trim().to_string());
    }

    // 2. 用户名
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    parts.push(user);

    // 3. 用户 HOME 路径
    if let Some(home) = dirs::home_dir() {
        parts.push(home.to_string_lossy().to_string());
    }

    // 4. OS 特定的机器 ID
    #[cfg(target_os = "linux")]
    {
        if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
            parts.push(id.trim().to_string());
        }
        if let Ok(id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            parts.push(id.trim().to_string());
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = s.lines().find(|l| l.contains("IOPlatformUUID")) {
                parts.push(line.trim().to_string());
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = std::process::Command::new("reg")
            .args(["query", r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = s.lines().find(|l| l.contains("MachineGuid")) {
                parts.push(line.trim().to_string());
            }
        }
    }

    // 5. 固定盐值（防止空输入）
    parts.push("sk-v2-machine-binding".to_string());

    parts.join("|")
}

// hex 编解码（避免额外依赖，使用内置实现）
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("hex string length must be even".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex character: {}", e))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_store_backend_name() {
        let store = FileStore::new();
        assert_eq!(store.backend_name(), "encrypted-file");
        assert!(store.is_available());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let password = "my-secret-password";
        let encrypted = FileStore::encrypt_password(password).unwrap();
        let decrypted = FileStore::decrypt_password(&encrypted).unwrap();
        assert_eq!(decrypted, password);
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = FileStore::new();
        let password = SecretString::new("test-db-password".to_string());

        // 存储
        store.save("__test_db__", &password).unwrap();

        // 读取
        let retrieved = store.get("__test_db__").unwrap();
        assert_eq!(retrieved.as_str(), "test-db-password");

        // 删除
        store.delete("__test_db__").unwrap();

        // 确认已删除
        assert!(store.get("__test_db__").is_err());
    }

    #[test]
    fn test_encrypt_different_passwords_produce_different_ciphertexts() {
        let data1 = FileStore::encrypt_password("password1").unwrap();
        let data2 = FileStore::encrypt_password("password2").unwrap();
        // 即使明文不同，输出也应该有差异（nonce 随机，密文不同）
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_hex_encode_decode() {
        let original = b"hello world";
        let encoded = hex::encode(original);
        let decoded = hex::decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_hex_decode_invalid_length() {
        assert!(hex::decode("abc").is_err()); // 奇数长度
    }

    #[test]
    fn test_hex_decode_invalid_chars() {
        assert!(hex::decode("zz").is_err()); // 无效字符
    }
}
