//! 平台检测与能力检查
//!
//! 提供跨平台抽象，让上层代码通过统一接口查询当前平台和可用能力。

#![allow(dead_code)]
use std::process::Command;

/// 目标平台枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS (Darwin)
    MacOS,
    /// Linux 桌面版（有 Desktop Environment）
    LinuxDesktop,
    /// Linux 服务器版（无 GUI）
    LinuxHeadless,
    /// Windows
    Windows,
}

impl Platform {
    /// 检测当前平台
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "linux") {
            // 检测是否有桌面环境（Secret Service 可用性）
            if is_desktop_linux() {
                Self::LinuxDesktop
            } else {
                Self::LinuxHeadless
            }
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            // 未知平台，按 Linux Headless 处理
            Self::LinuxHeadless
        }
    }

    /// 是否为 Unix 系（macOS + Linux）
    pub fn is_unix(&self) -> bool {
        matches!(self, Self::MacOS | Self::LinuxDesktop | Self::LinuxHeadless)
    }

    /// 是否支持文件权限设置（Unix 系支持，Windows 不支持）
    pub fn supports_file_permissions(&self) -> bool {
        self.is_unix()
    }

    /// 系统钥匙串是否可用
    pub fn keychain_available(&self) -> bool {
        match self {
            Self::MacOS => true,                 // macOS Keychain 始终可用
            Self::LinuxDesktop => is_secret_service_available(),
            Self::LinuxHeadless => false,        // 无桌面环境，不可用
            Self::Windows => true,               // Windows Credential Manager 始终可用
        }
    }

    /// 获取平台的显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MacOS => "macOS",
            Self::LinuxDesktop => "Linux (Desktop)",
            Self::LinuxHeadless => "Linux (Headless)",
            Self::Windows => "Windows",
        }
    }

    /// 获取推荐的后端
    pub fn recommended_keyring_backend(&self) -> &'static str {
        match self {
            Self::MacOS => "keychain",
            Self::LinuxDesktop => "secret-service",
            Self::LinuxHeadless => "encrypted-file",
            Self::Windows => "credential-manager",
        }
    }
}

/// 检测 Linux 是否为桌面环境
///
/// 通过检查 XDG_SESSION_TYPE 或 DBUS_SESSION_BUS_ADDRESS 环境变量判断。
fn is_desktop_linux() -> bool {
    // 检查环境变量
    let has_session = std::env::var("XDG_SESSION_TYPE").is_ok()
        || std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok();

    // 同时检查 DBus 是否可用
    has_session && is_secret_service_available()
}

/// 检测 Linux Secret Service（DBus）是否可用
fn is_secret_service_available() -> bool {
    std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
}

/// 检测 ssh 客户端是否安装
///
/// 返回版本字符串（如 "OpenSSH_8.9p1"），如果未安装返回 None。
pub fn detect_ssh_version() -> Option<String> {
    Command::new("ssh")
        .arg("-V")
        .output()
        .ok()
        .and_then(|output| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // ssh -V 输出到 stderr
            let version = stderr.trim().to_string();
            if version.is_empty() {
                None
            } else {
                Some(version)
            }
        })
}

/// 检测 ssh-keygen 是否可用
pub fn detect_ssh_keygen() -> bool {
    Command::new("ssh-keygen")
        .arg("-?")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_current_does_not_panic() {
        // 确保 Platform::current() 不会 panic
        let platform = Platform::current();
        assert!(!platform.display_name().is_empty());
    }

    #[test]
    fn test_platform_is_unix() {
        assert!(Platform::MacOS.is_unix());
        assert!(Platform::LinuxDesktop.is_unix());
        assert!(Platform::LinuxHeadless.is_unix());
        assert!(!Platform::Windows.is_unix());
    }

    #[test]
    fn test_platform_supports_file_permissions() {
        assert!(Platform::MacOS.supports_file_permissions());
        assert!(Platform::LinuxDesktop.supports_file_permissions());
        assert!(!Platform::Windows.supports_file_permissions());
    }

    #[test]
    fn test_platform_display_name() {
        assert_eq!(Platform::MacOS.display_name(), "macOS");
        assert_eq!(Platform::Windows.display_name(), "Windows");
        assert_eq!(Platform::LinuxHeadless.display_name(), "Linux (Headless)");
    }

    #[test]
    fn test_platform_keychain_available() {
        // macOS keychain 始终可用
        assert!(Platform::MacOS.keychain_available());
        // Headless Linux 不可用（无桌面环境）
        assert!(!Platform::LinuxHeadless.keychain_available());
    }

    #[test]
    fn test_recommended_backend() {
        assert_eq!(Platform::MacOS.recommended_keyring_backend(), "keychain");
        assert_eq!(
            Platform::LinuxHeadless.recommended_keyring_backend(),
            "encrypted-file"
        );
        assert_eq!(
            Platform::Windows.recommended_keyring_backend(),
            "credential-manager"
        );
    }

    #[test]
    fn test_detect_ssh_version() {
        // 如果系统安装了 ssh，应返回版本信息
        let version = detect_ssh_version();
        if version.is_some() {
            assert!(version.unwrap().contains("OpenSSH"));
        }
    }

    #[test]
    fn test_detect_ssh_keygen() {
        // 基本检测，不应 panic
        let _ = detect_ssh_keygen();
    }
}
