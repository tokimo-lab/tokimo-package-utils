//! VFS 来源类型分类。
//!
//! 单一事实源：判断 `vfs.r#type` 是否走「本地文件系统」通路。
//!
//! - `true` ⇒ 调用方应当用 `tokio::fs` / FFmpeg / yt-dlp 等本地工具，
//!   并通过 [`crate::path::internal_to_native`] 转换内部 Unix 风格路径。
//! - `false` ⇒ 调用方必须走 VFS 协议层（SMB / SFTP / S3 / WebDAV / FTP / NFS / 网盘等）。

/// 本地驱动的 type 字符串。
pub const LOCAL_SOURCE_TYPE: &str = "local";

/// 是否是本地文件系统驱动。
#[must_use]
pub fn is_local_source(source_type: &str) -> bool {
    source_type == LOCAL_SOURCE_TYPE
}
