#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct Operation;
impl Operation {
    pub fn acquire() -> Result<Self, String> {
        ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self)
            .map_err(|_| "A wipe operation is already running".into())
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::SeqCst);
    }
}
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub fn validate_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(invalid("Select an absolute local path"));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        match path.components().next() {
            Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
            _ => return Err(invalid("Network and device paths are not supported")),
        }
        if path
            .components()
            .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().contains(':')))
        {
            return Err(invalid("Alternate data streams are not supported"));
        }
    }
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("Symbolic links are not supported"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("Reparse points are not supported"));
            }
        }
    }
    Ok(())
}

pub struct WipeFile {
    pub file: File,
    #[cfg(unix)]
    parent: std::os::fd::OwnedFd,
    #[cfg(unix)]
    name: std::ffi::OsString,
    #[cfg(windows)]
    parents: Vec<File>,
}

impl WipeFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        validate_path(path)?;
        #[cfg(unix)]
        {
            use rustix::fs::{open, openat, Mode, OFlags};
            let directory_flags =
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let mut parent = open("/", directory_flags, Mode::empty())?;
            let path_parent = path
                .parent()
                .ok_or_else(|| invalid("Cannot wipe a filesystem root"))?;
            for component in path_parent.components() {
                match component {
                    Component::RootDir => {}
                    Component::Normal(name) => {
                        parent = openat(&parent, name, directory_flags, Mode::empty())?;
                    }
                    _ => return Err(invalid("Path must not contain dot components")),
                }
            }
            let name = path
                .file_name()
                .ok_or_else(|| invalid("Missing filename"))?
                .to_owned();
            let fd = openat(
                &parent,
                &name,
                OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            let file = File::from(fd);
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.nlink() != 1 {
                return Err(invalid(
                    "Only regular files with one hard link can be wiped",
                ));
            }
            Ok(Self { file, parent, name })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };
            let mut parents = Vec::new();
            let mut ancestors: Vec<_> = path
                .parent()
                .ok_or_else(|| invalid("Missing parent"))?
                .ancestors()
                .collect();
            ancestors.reverse();
            for ancestor in ancestors {
                let file = OpenOptions::new()
                    .read(true)
                    .share_mode(3)
                    .custom_flags(0x02200000)
                    .open(ancestor)?;
                if file.metadata()?.file_attributes() & 0x400 != 0 {
                    return Err(invalid("Reparse points are not supported"));
                }
                parents.push(file);
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .access_mode(0xC0010000) // GENERIC_READ | GENERIC_WRITE | DELETE
                .share_mode(0)
                .custom_flags(0x00200000)
                .open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("Only regular files can be wiped"));
            }
            // SAFETY: the live File owns this handle; info is writable for the documented output structure.
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if info.nNumberOfLinks != 1 {
                return Err(invalid("Hard-linked files are not supported"));
            }
            Ok(Self { file, parents })
        }
    }

    pub fn remove(self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use rustix::fs::{statat, unlinkat, AtFlags};
            use std::os::unix::fs::MetadataExt;
            let current = statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW)?;
            let original = self.file.metadata()?;
            if current.st_ino != original.ino() || current.st_dev != original.dev() {
                return Err(invalid("File changed during wipe; replacement retained"));
            }
            unlinkat(&self.parent, &self.name, AtFlags::empty())?;
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
            };
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: the live handle has DELETE access, and info has the API's documented size and lifetime.
            let result = unsafe {
                SetFileInformationByHandle(
                    self.file.as_raw_handle(),
                    FileDispositionInfo,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            drop(self.file);
            drop(self.parents);
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub paths: Vec<String>,
    pub algorithm: String,
    pub passes: u32,
    pub operation: String,
    pub window: String,
}
static CONFIRMATION: std::sync::Mutex<Option<(Request, std::time::Instant)>> =
    std::sync::Mutex::new(None);

pub fn remember_confirmation(request: Request) -> Result<(), String> {
    *CONFIRMATION
        .lock()
        .map_err(|_| "Confirmation state unavailable")? =
        Some((request, std::time::Instant::now()));
    Ok(())
}
pub fn consume_confirmation(request: Request) -> Result<(), String> {
    let confirmed = CONFIRMATION
        .lock()
        .map_err(|_| "Confirmation state unavailable")?
        .take();
    match confirmed {
        Some((stored, time))
            if stored == request && time.elapsed() < std::time::Duration::from_secs(120) =>
        {
            Ok(())
        }
        _ => Err("Confirm this exact wipe request before starting".into()),
    }
}

pub fn remove_empty_tree(path: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for entry in walkdir::WalkDir::new(path)
        .contents_first(true)
        .follow_links(false)
    {
        match entry {
            Ok(entry) if entry.file_type().is_dir() => {
                if let Err(error) = fs::remove_dir(entry.path()) {
                    errors.push(format!(
                        "Directory retained {}: {}",
                        entry.path().display(),
                        error
                    ));
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(error.to_string()),
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_preserves_failed_and_new_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("empty")).unwrap();
        let file = dir.path().join("unwiped.txt");
        fs::write(&file, b"keep me").unwrap();
        assert!(!remove_empty_tree(dir.path()).is_empty());
        assert_eq!(fs::read(&file).unwrap(), b"keep me");
        assert!(!dir.path().join("empty").exists());
    }
    #[test]
    fn confirmation_is_exact_and_single_use() {
        let request = || Request {
            paths: vec!["example".into()],
            algorithm: "Random".into(),
            passes: 1,
            operation: "files".into(),
            window: "main".into(),
        };
        remember_confirmation(request()).unwrap();
        let mut changed = request();
        changed.paths.push("another".into());
        assert!(consume_confirmation(changed).is_err());
        remember_confirmation(request()).unwrap();
        assert!(consume_confirmation(request()).is_ok());
        assert!(consume_confirmation(request()).is_err());
    }
    #[test]
    fn operations_are_exclusive_and_release_on_drop() {
        let first = Operation::acquire().unwrap();
        assert!(Operation::acquire().is_err());
        drop(first);
        assert!(Operation::acquire().is_ok());
    }
}
