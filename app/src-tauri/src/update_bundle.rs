//! Atomic bundle replacement. The caller must authenticate and validate the staged bundle first.
use std::{
    ffi::CString,
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

fn exchange(left: &Path, right: &Path) -> io::Result<()> {
    let left = CString::new(left.as_os_str().as_bytes())?;
    let right = CString::new(right.as_os_str().as_bytes())?;
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    // Linux exercises the same transaction with the kernel's atomic exchange operation.
    #[cfg(all(test, target_os = "linux"))]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub struct BundleTransaction {
    installed: PathBuf,
    candidate: PathBuf,
    staging: Option<tempfile::TempDir>,
    swapped: bool,
}

impl BundleTransaction {
    pub fn begin(
        installed: &Path,
        staging: tempfile::TempDir,
        candidate: &Path,
    ) -> io::Result<Self> {
        // Both trees must be real directories. Do not exchange a user's symlink or arbitrary file.
        for path in [installed, candidate] {
            if !path.symlink_metadata()?.file_type().is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "O pacote precisa ser uma pasta real.",
                ));
            }
        }
        if candidate.parent() != Some(staging.path())
            || installed.parent() != staging.path().parent()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Prepare a atualização ao lado do aplicativo.",
            ));
        }
        exchange(installed, candidate)?;
        Ok(Self {
            installed: installed.into(),
            candidate: candidate.into(),
            staging: Some(staging),
            swapped: true,
        })
    }

    /// Call only after the new daemon has been started and its version verified.
    pub fn commit(mut self) {
        self.swapped = false;
    }

    pub fn rollback(&mut self) -> io::Result<()> {
        if self.swapped {
            exchange(&self.installed, &self.candidate)?;
            self.swapped = false;
        }
        Ok(())
    }
}

impl Drop for BundleTransaction {
    fn drop(&mut self) {
        if let Err(error) = self.rollback() {
            // Keep the old app recoverable even if a permission/volume change prevents rollback.
            if let Some(staging) = self.staging.take() {
                let backup = staging.keep();
                eprintln!(
                    "Não foi possível restaurar o aplicativo: {error}. Backup preservado em {}",
                    backup.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture() -> (tempfile::TempDir, PathBuf, tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("Open Island.app");
        fs::create_dir(&installed).unwrap();
        fs::write(installed.join("version"), "old").unwrap();
        let staging = tempfile::tempdir_in(root.path()).unwrap();
        let candidate = staging.path().join("Open Island.app");
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join("version"), "new").unwrap();
        (root, installed, staging, candidate)
    }
    #[test]
    fn commit_retains_new_bundle_and_removes_old_bundle() {
        let (_root, installed, staging, candidate) = fixture();
        let transaction = BundleTransaction::begin(&installed, staging, &candidate).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(candidate.join("version")).unwrap(),
            "old"
        );
        transaction.commit();
        assert!(!candidate.exists());
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
    }
    #[test]
    fn failed_verification_and_panic_restore_old_bundle() {
        for panic in [false, true] {
            let (_root, installed, staging, candidate) = fixture();
            let _ = std::panic::catch_unwind(|| {
                let _transaction =
                    BundleTransaction::begin(&installed, staging, &candidate).unwrap();
                if panic {
                    panic!("simulated failure after replacement");
                }
            });
            assert_eq!(
                fs::read_to_string(installed.join("version")).unwrap(),
                "old"
            );
            assert!(!candidate.exists());
        }
    }
    #[test]
    fn missing_candidate_and_symlink_leave_installed_bundle_intact() {
        for symlink in [false, true] {
            let (_root, installed, staging, candidate) = fixture();
            fs::remove_dir_all(&candidate).unwrap();
            if symlink {
                std::os::unix::fs::symlink(&installed, &candidate).unwrap();
            }
            assert!(BundleTransaction::begin(&installed, staging, &candidate).is_err());
            assert_eq!(
                fs::read_to_string(installed.join("version")).unwrap(),
                "old"
            );
        }
    }
    #[test]
    fn failed_rollback_preserves_the_backup_for_recovery() {
        let (root, installed, staging, candidate) = fixture();
        let transaction = BundleTransaction::begin(&installed, staging, &candidate).unwrap();
        // Simulate another program moving the installed app before rollback.
        fs::rename(&installed, root.path().join("moved.app")).unwrap();
        drop(transaction);
        assert_eq!(
            fs::read_to_string(candidate.join("version")).unwrap(),
            "old"
        );
    }
}
