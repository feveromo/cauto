use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

pub fn ensure_private_dir(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(path).map_err(|source| AppError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    #[cfg(unix)]
    {
        let permissions = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(path, permissions).map_err(|source| AppError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    options
}

/// Writes a same-directory temporary file, syncs it, and atomically renames it.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| AppError::Cache {
        path: path.to_path_buf(),
        message: "cache path has no parent".into(),
    })?;
    ensure_private_dir(parent)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("cache");
    let lock_path = parent.join(format!(".{file_name}.lock"));
    let lock = private_file_options()
        .open(&lock_path)
        .map_err(|error| AppError::Cache {
            path: lock_path.clone(),
            message: error.to_string(),
        })?;
    File::lock(&lock).map_err(|error| AppError::Cache {
        path: lock_path.clone(),
        message: format!("failed to lock cache: {error}"),
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).map_err(|error| AppError::Cache {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
        file.write_all(bytes).map_err(|error| AppError::Cache {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
        file.flush().map_err(|error| AppError::Cache {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
        file.sync_data().map_err(|error| AppError::Cache {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
        std::fs::rename(&temporary, path).map_err(|error| AppError::Cache {
            path: path.to_path_buf(),
            message: format!("atomic rename failed: {error}"),
        })?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_data();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    let _ = File::unlock(&lock);
    result
}
