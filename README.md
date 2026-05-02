# tokimo-package-utils

Common utilities for [tokimo](https://github.com/tokimo-lab) packages.

## Modules

- **`path`** — Cross-OS local file path handling. Internal format is always Unix-style (`/` separators); Windows drive letters map to `/c/...` ↔ `C:\...`.
- **`source`** — VFS source type classification (`is_local_source`).

## Usage

```rust
use tokimo_package_utils::path::{normalize_local_path, internal_to_native};
use tokimo_package_utils::is_local_source;

// Normalize and convert paths
let normalized = normalize_local_path("/c/Users/foo").unwrap();
let native = internal_to_native(&normalized);

// Check VFS source type
assert!(is_local_source("local"));
assert!(!is_local_source("smb"));
```

## License

MIT
