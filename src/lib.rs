//! 通用工具函数：跨 OS 本地路径处理等。
//!
//! 该 crate 故意保持轻量（仅依赖 `serde`），不引入 ts-rs / axum 等。
//! 前端可见的 DTO 由 `rust-server` 内部 wrapper 通过 ts-rs 导出。

pub mod path;
pub mod source;

pub use source::is_local_source;
