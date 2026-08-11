use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use super::persist::PersistenceError;

/// Operating-system operations used by the higher-level persistence protocol.
///
/// The protocol owns staging, identity, state transitions, and error policy;
/// this trait owns only the host filesystem operations needed to carry them
/// out. Tests can inject failures without changing protocol code.
pub(crate) trait PersistenceOps {
    fn write_new(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    fn claim_file(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn claim_dir(&self, target: &Path) -> io::Result<()>;
    fn claim_dir_no_replace(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    fn sync_path(&self, path: &Path) -> io::Result<()>;
    fn sync_tree(&self, path: &Path) -> io::Result<()> {
        self.sync_path(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemOps;

impl PersistenceOps for SystemOps {
    fn write_new(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        file.write_all(content)?;
        file.sync_all()
    }

    fn claim_file(&self, staged: &Path, target: &Path) -> io::Result<()> {
        fs::hard_link(staged, target)
    }

    fn claim_dir(&self, target: &Path) -> io::Result<()> {
        fs::create_dir(target)
    }

    fn claim_dir_no_replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        atomic_directory_claim(staged, target)
    }

    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        fs::rename(staged, target)
    }

    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(content)?;
        file.sync_all()
    }

    fn sync_path(&self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }

    fn sync_tree(&self, path: &Path) -> io::Result<()> {
        sync_tree(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

#[cfg(target_os = "linux")]
const AT_FDCWD: std::os::raw::c_int = -100;
#[cfg(target_os = "linux")]
const RENAME_NOREPLACE: std::os::raw::c_uint = 1;
#[cfg(target_os = "macos")]
const RENAME_EXCL: std::os::raw::c_uint = 0x0000_0004;

#[cfg(target_os = "linux")]
extern "C" {
    fn renameat2(
        olddirfd: std::os::raw::c_int,
        oldpath: *const std::os::raw::c_char,
        newdirfd: std::os::raw::c_int,
        newpath: *const std::os::raw::c_char,
        flags: std::os::raw::c_uint,
    ) -> std::os::raw::c_int;
}

#[cfg(target_os = "macos")]
extern "C" {
    fn renamex_np(
        from: *const std::os::raw::c_char,
        to: *const std::os::raw::c_char,
        flags: std::os::raw::c_uint,
    ) -> std::os::raw::c_int;
}

/// Claim an absent final directory with the platform's explicit no-replace
/// primitive. There is intentionally no ordinary `rename` fallback here.
fn atomic_directory_claim(staged: &Path, target: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let staged = CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stage path contains NUL"))?;
        let target = CString::new(target.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
        let result = unsafe {
            renameat2(
                AT_FDCWD,
                staged.as_ptr(),
                AT_FDCWD,
                target.as_ptr(),
                RENAME_NOREPLACE,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "macos")]
    {
        let staged = CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stage path contains NUL"))?;
        let target = CString::new(target.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
        let result = unsafe { renamex_np(staged.as_ptr(), target.as_ptr(), RENAME_EXCL) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (staged, target);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "target has no proven atomic absent-final directory primitive",
        ))
    }
}

pub(crate) fn classify_atomic_directory_error(path: &Path, error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::AlreadyExists {
        return PersistenceError::Conflict {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into();
    }
    let unsupported = match error.raw_os_error() {
        #[cfg(target_os = "linux")]
        Some(code) => matches!(code, 18 | 22 | 38 | 95), // EXDEV/EINVAL/ENOSYS/ENOTSUP
        #[cfg(target_os = "macos")]
        Some(code) => matches!(code, 18 | 22 | 45), // EXDEV/EINVAL/ENOTSUP
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Some(_) => true,
        None => error.kind() == io::ErrorKind::Unsupported,
    };
    if unsupported
        || matches!(
            error.kind(),
            io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
        )
    {
        PersistenceError::Unsupported {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into()
    } else {
        PersistenceError::Incomplete {
            path: path.to_path_buf(),
            detail: format!("atomic directory claim failed: {error}"),
        }
        .into()
    }
}

fn sync_tree(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot durably sync a symlink",
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    } else if metadata.is_file() {
        fs::File::open(path)?.sync_all()?;
        return Ok(());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot durably sync a non-regular packet entry",
        ));
    }
    fs::File::open(path)?.sync_all()
}
