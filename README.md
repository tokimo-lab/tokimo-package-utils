# tokimo-package-utils

通用工具函数库 for [Tokimo OS](https://github.com/tokimo-lab/tokimo.io)。

## Features

- **跨 OS 路径处理** — 内部统一 Unix 风格 (`/`)，Windows 盘符映射为一级目录 (`/c/Users/...` ↔ `C:\Users\...`)
- **路径校验** — 拒绝 `..` 遍历、空路径、相对路径
- **根目录枚举** — Linux: `["/"]`，Windows: 自动探测可访问盘符

## Usage

```rust
use tokimo_package_utils::path::{normalize_local_path, internal_to_native, native_to_internal};

// 校验并规范化路径
let normalized = normalize_local_path("/foo/./bar//baz").unwrap();
assert_eq!(normalized, "/foo/bar/baz");

// 内部格式 ↔ OS 原生路径转换
#[cfg(windows)]
{
    assert_eq!(internal_to_native("/c/Users/foo"), "C:\\Users\\foo");
    assert_eq!(native_to_internal("C:\\Users\\foo"), "/c/Users/foo");
}
```

## Cargo

```toml
tokimo-package-utils = { git = "https://github.com/tokimo-lab/tokimo-package-utils" }
```

## License

MIT
