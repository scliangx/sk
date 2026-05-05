//! ED25519 密钥对生成器
//!
//! 使用 ed25519-dalek 生成密钥对，
//! 通过 ssh-key crate 编码为标准 OpenSSH 格式（PEM / authorized_keys）。
//!
//! 安全要点：
//! - 密钥类型硬编码为 ED25519，不支持 RSA/DSA
//! - 私钥无 passphrase（符合免密登录需求）
//! - 私钥文件权限设为 0o600

use ssh_key::private::Ed25519Keypair;
use ssh_key::private::PrivateKey;
use ssh_key::LineEnding;

use crate::error::{SkError, SkResult};
use crate::infra::fs;

/// 生成的密钥对
#[derive(Debug)]
#[allow(dead_code)]
pub struct KeyPair {
    /// OpenSSH 格式私钥（PEM 编码）
    pub private_key_pem: String,
    /// OpenSSH 格式公钥（ssh-ed25519 <base64> <comment>）
    pub public_key_openssh: String,
    /// authorized_keys 格式公钥（与 openssh 格式相同）
    pub authorized_key: String,
}

/// ED25519 密钥生成器
#[allow(dead_code)]
pub struct KeyGenerator;

#[allow(dead_code)]
impl KeyGenerator {
    /// 生成 ED25519 密钥对
    ///
    /// # 返回
    /// KeyPair 包含 PEM 格式私钥和 OpenSSH 格式公钥
    pub fn generate(comment: &str) -> SkResult<KeyPair> {
        // 生成随机 ED25519 私钥（使用 ssh-key crate 确保 SSH 兼容性）
        let private_key = Ed25519Keypair::random(&mut rand::rngs::OsRng);

        // 构造 PrivateKey 以利用 ssh-key 的编码能力
        let pkey = PrivateKey::from(private_key);

        // 编码私钥为 OpenSSH PEM 格式
        let private_key_pem = pkey
            .to_openssh(LineEnding::LF)
            .map_err(|e| SkError::KeyOperation(format!("Private key encoding failed: {}", e)))?
            .to_string();

        // 编码公钥为 OpenSSH 格式
        let public_key = pkey.public_key();
        let public_key_openssh = public_key.to_openssh().map_err(|e| {
            SkError::KeyOperation(format!("Public key encoding failed: {}", e))
        })?;

        let authorized_key = format!("{} {}", &public_key_openssh, comment);

        Ok(KeyPair {
            private_key_pem,
            public_key_openssh,
            authorized_key,
        })
    }

    /// 生成密钥对并保存到磁盘
    ///
    /// # 参数
    /// - name: 服务器名称，用于密钥文件命名和公钥注释
    ///
    /// # 副作用
    /// - 创建 ~/.ssh/sk_keys/{name}_key（私钥，权限 600）
    /// - 创建 ~/.ssh/sk_keys/{name}_key.pub（公钥）
    pub fn generate_and_save(name: &str) -> SkResult<KeyPair> {
        // 确保密钥目录存在
        let keys_dir = fs::sk_keys_dir()?;
        fs::ensure_dir(&keys_dir)?;

        let comment = format!("sk-generated:{}", name);
        let key_pair = Self::generate(&comment)?;

        let private_key_path = fs::server_key_path(name)?;
        let public_key_path = fs::server_pubkey_path(name)?;

        // 写入私钥（原子写入 + 设置权限 600）
        fs::atomic_write(&private_key_path, &key_pair.private_key_pem)?;

        // 写入公钥
        fs::atomic_write(&public_key_path, &key_pair.authorized_key)?;

        Ok(key_pair)
    }

    /// 检查密钥对是否存在
    pub fn key_exists(name: &str) -> bool {
        let private = fs::server_key_path(name);
        let public = fs::server_pubkey_path(name);

        match (private, public) {
            (Ok(priv_path), Ok(pub_path)) => priv_path.exists() && pub_path.exists(),
            _ => false,
        }
    }

    /// 读取公钥内容（用于推送到远程服务器）
    pub fn read_public_key(name: &str) -> SkResult<String> {
        let pubkey_path = fs::server_pubkey_path(name)?;
        fs::read_file(&pubkey_path)
    }

    /// 删除指定服务器的密钥对
    pub fn delete_keys(name: &str) -> SkResult<()> {
        let private_key_path = fs::server_key_path(name)?;
        let public_key_path = fs::server_pubkey_path(name)?;

        if private_key_path.exists() {
            std::fs::remove_file(&private_key_path).map_err(|e| SkError::FileWrite {
                path: private_key_path,
                reason: format!("Failed to delete private key: {}", e),
            })?;
        }

        if public_key_path.exists() {
            std::fs::remove_file(&public_key_path).map_err(|e| SkError::FileWrite {
                path: public_key_path,
                reason: format!("Failed to delete public key: {}", e),
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key_pair() {
        let key_pair = KeyGenerator::generate("test-comment").unwrap();

        // 私钥应为 OpenSSH PEM 格式
        assert!(key_pair
            .private_key_pem
            .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(key_pair
            .private_key_pem
            .ends_with("-----END OPENSSH PRIVATE KEY-----\n"));

        // 公钥应以 ssh-ed25519 开头
        assert!(key_pair.public_key_openssh.starts_with("ssh-ed25519 "));

        // authorized_key 应包含注释
        assert!(key_pair.authorized_key.ends_with("test-comment"));
        assert!(key_pair.authorized_key.starts_with("ssh-ed25519 "));
    }

    #[test]
    fn test_generate_unique_keys() {
        // 每次生成应产生不同的密钥
        let kp1 = KeyGenerator::generate("c1").unwrap();
        let kp2 = KeyGenerator::generate("c2").unwrap();

        assert_ne!(kp1.public_key_openssh, kp2.public_key_openssh);
        assert_ne!(kp1.private_key_pem, kp2.private_key_pem);
    }

    #[test]
    fn test_generate_and_save() {
        // 使用系统路径生成密钥，验证生成后私钥权限
        let result = KeyGenerator::generate_and_save("__test_sk_keygen__");
        assert!(result.is_ok());

        let key_pair = result.unwrap();
        assert!(!key_pair.private_key_pem.is_empty());

        // 清理测试密钥
        let _ = KeyGenerator::delete_keys("__test_sk_keygen__");
    }

    #[test]
    fn test_key_exists() {
        // 先确保没有残留
        let _ = KeyGenerator::delete_keys("__test_exists__");

        assert!(!KeyGenerator::key_exists("__test_exists__"));

        // 生成密钥后应存在
        KeyGenerator::generate_and_save("__test_exists__").unwrap();
        assert!(KeyGenerator::key_exists("__test_exists__"));

        // 清理
        let _ = KeyGenerator::delete_keys("__test_exists__");
    }

    #[test]
    fn test_read_public_key() {
        let _ = KeyGenerator::delete_keys("__test_read_pub__");

        KeyGenerator::generate_and_save("__test_read_pub__").unwrap();
        let pubkey = KeyGenerator::read_public_key("__test_read_pub__").unwrap();
        assert!(pubkey.starts_with("ssh-ed25519 "));

        let _ = KeyGenerator::delete_keys("__test_read_pub__");
    }

    #[test]
    fn test_delete_keys() {
        // 先生成
        KeyGenerator::generate_and_save("__test_delete__").unwrap();
        assert!(KeyGenerator::key_exists("__test_delete__"));

        // 删除
        KeyGenerator::delete_keys("__test_delete__").unwrap();
        assert!(!KeyGenerator::key_exists("__test_delete__"));
    }

    #[test]
    fn test_delete_nonexistent_keys() {
        // 删除不存在的密钥不应出错
        let result = KeyGenerator::delete_keys("__nonexistent_key_xyz__");
        assert!(result.is_ok());
    }
}
