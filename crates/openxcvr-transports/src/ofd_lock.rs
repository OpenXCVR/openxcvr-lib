//! Internal Linux x86_64 shared-inode OFD primitive; not wired to public opens.
//! Cooperative exclusion only: not device selection identity or full exclusivity.
//! Administrator provisions one regular file and supplies its exact ownership/mode.
#![allow(dead_code)] // Integration is deliberately out of scope.

use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::{Error, ErrorKind, Result};

/// Trusted administrator-supplied *exact* metadata; no values are inferred
/// from the current user or repaired at runtime. Extended POSIX ACLs are not
/// supported by this primitive; a provisioned group/mode may grant both UIDs.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub directory_uid: u32,
    pub directory_gid: u32,
    pub directory_mode: u32,
    pub directory_dev: u64,
    pub directory_ino: u64,
    pub file_uid: u32,
    pub file_gid: u32,
    pub file_mode: u32,
    /// Recorded at provisioning; prevents an identically permissioned new inode
    /// from forming a second, independent namespace while old guards exist.
    pub file_dev: u64,
    pub file_ino: u64,
}

pub(crate) struct Guard {
    // Each acquire opens afresh; dup() would share an OFD and not contend.
    file: File,
    creator_pid: libc::pid_t,
}

impl Guard {
    /// Device must already be open and retained by the future session. No
    /// pathname is consulted for the device or synthesized for the lock.
    pub(crate) fn acquire(device: BorrowedFd<'_>, path: &Path, policy: &Policy) -> Result<Self> {
        if mem::size_of::<libc::off_t>() != 8 {
            return Err(setup("64-bit off_t required"));
        }
        let mut stat: libc::stat = unsafe { mem::zeroed() };
        if unsafe { libc::fstat(device.as_raw_fd(), &mut stat) } != 0 {
            return Err(io_error("device fstat", io::Error::last_os_error()));
        }
        if stat.st_mode & libc::S_IFMT != libc::S_IFCHR {
            return Err(Error::new(
                ErrorKind::Invalid,
                "device fd is not a character device",
            ));
        }
        let major = ((stat.st_rdev >> 8) & 0xfff) | ((stat.st_rdev >> 32) & !0xfff);
        let minor = (stat.st_rdev & 0xff) | ((stat.st_rdev >> 12) & !0xff);
        let offset = range(major, minor)?;
        let (parent, name) = path
            .parent()
            .zip(path.file_name())
            .filter(|(parent, name)| {
                parent.is_absolute() && *name != OsStr::new(".") && *name != OsStr::new("..")
            })
            .ok_or_else(|| setup("lock path must be an absolute file path"))?;
        // All ancestors are opened one component at a time: O_NOFOLLOW also
        // refuses intermediate symlinks. No writable ancestor for other UIDs;
        // root-owned sticky /tmp is permitted for a private test directory.
        let dir = open_parent(parent)?;
        let md = dir
            .metadata()
            .map_err(|e| io_error("lock parent fstat", e))?;
        if !md.file_type().is_dir()
            || md.uid() != policy.directory_uid
            || md.gid() != policy.directory_gid
            || md.mode() & 0o7777 != policy.directory_mode
            || md.mode() & 0o022 != 0
            || md.nlink() < 2
            || md.dev() != policy.directory_dev
            || md.ino() != policy.directory_ino
        {
            return Err(setup("untrusted lock parent metadata"));
        }
        reject_acl(&dir, "lock parent")?;
        let file = openat(
            dir.as_raw_fd(),
            name,
            libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            "shared lock file",
        )?;
        validate_file(&dir, &file, name, policy)?;
        let mut lock = libc::flock {
            l_type: libc::F_WRLCK as i16,
            l_whence: libc::SEEK_SET as i16,
            l_start: offset,
            l_len: 1,
            l_pid: 0,
        };
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_SETLK, &mut lock) } != 0 {
            let e = io::Error::last_os_error();
            return Err(
                if matches!(e.raw_os_error(), Some(libc::EAGAIN | libc::EACCES)) {
                    Error::new(ErrorKind::Busy, format!("shared OFD range held: {e}"))
                } else {
                    io_error("shared OFD lock unavailable", e)
                },
            );
        }
        validate_file(&dir, &file, name, policy)?;
        let current = open_parent(parent)?;
        let now = current
            .metadata()
            .map_err(|e| io_error("parent recheck", e))?;
        if now.dev() != policy.directory_dev || now.ino() != policy.directory_ino {
            return Err(setup("shared lock parent replaced"));
        }
        Ok(Self {
            file,
            creator_pid: unsafe { libc::getpid() },
        })
    }

    /// An integrating transport MUST check this before each bus/report action.
    /// The check uses no allocator: in a multithreaded parent's forked child,
    /// only async-signal-safe functions are allowed before exec or _exit.
    /// A false result means the child must not use the inherited transport;
    /// it may only close its inherited descriptors or exit. Do not construct
    /// an owned Error in that child before exec.
    pub(crate) fn is_creator_process(&self) -> bool {
        // SAFETY: getpid has no preconditions and is async-signal-safe.
        unsafe { libc::getpid() == self.creator_pid }
    }
}

fn open_parent(parent: &Path) -> Result<File> {
    let mut dir = open_root()?;
    let root = dir.metadata().map_err(|e| io_error("root fstat", e))?;
    if root.uid() != 0 || root.mode() & 0o022 != 0 {
        return Err(setup("untrusted root directory"));
    }
    reject_acl(&dir, "root directory")?;
    let components: Vec<_> = parent.components().skip(1).collect();
    for (index, component) in components.iter().enumerate() {
        use std::path::Component;
        let Component::Normal(part) = component else {
            return Err(setup("non-normal lock directory component"));
        };
        let next = openat(
            dir.as_raw_fd(),
            part,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            "lock directory",
        )?;
        let md = next
            .metadata()
            .map_err(|e| io_error("lock directory fstat", e))?;
        if !md.file_type().is_dir() {
            return Err(setup("non-directory lock path component"));
        }
        if index + 1 != components.len() {
            // Only root may own an ancestor. A sticky root-owned /tmp
            // can contain a separately trusted non-writable parent.
            let trusted = md.uid() == 0 && (md.mode() & 0o022 == 0 || md.mode() & 0o1777 == 0o1777);
            if !trusted {
                return Err(setup("untrusted lock directory ancestor"));
            }
        }
        reject_acl(&next, "lock directory ancestor")?;
        dir = next;
    }
    Ok(dir)
}

fn range(major: u64, minor: u64) -> Result<libc::off_t> {
    if major > 4095 || minor > 1_048_575 {
        return Err(Error::new(
            ErrorKind::Invalid,
            "device number outside 12/20-bit OFD range",
        ));
    }
    let value = major
        .checked_shl(20)
        .and_then(|m| m.checked_add(minor))
        .and_then(|v| v.checked_add(1))
        .and_then(|v| i64::try_from(v).ok())
        .ok_or_else(|| setup("OFD offset overflow"))?;
    Ok(value - 1)
}

fn open_root() -> Result<File> {
    openat(
        libc::AT_FDCWD,
        OsStr::new("/"),
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        "root",
    )
}
fn openat(dir: i32, name: &OsStr, flags: i32, label: &str) -> Result<File> {
    let name = CString::new(name.as_bytes()).map_err(|_| setup("NUL in lock path"))?;
    let fd = unsafe { libc::openat(dir, name.as_ptr(), flags) };
    if fd < 0 {
        return Err(io_error(label, io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn validate_file(dir: &File, file: &File, name: &OsStr, policy: &Policy) -> Result<()> {
    let md = file
        .metadata()
        .map_err(|e| io_error("shared lock fstat", e))?;
    if !md.file_type().is_file()
        || md.uid() != policy.file_uid
        || md.gid() != policy.file_gid
        || md.mode() & 0o7777 != policy.file_mode
        || md.nlink() != 1
        || md.mode() & 0o002 != 0
        || md.dev() != policy.file_dev
        || md.ino() != policy.file_ino
    {
        return Err(setup("untrusted shared lock file metadata"));
    }
    reject_acl(file, "shared lock file")?;
    let name = CString::new(name.as_bytes()).map_err(|_| setup("NUL in lock name"))?;
    let mut stat: libc::stat = unsafe { mem::zeroed() };
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io_error(
            "shared lock path lookup",
            io::Error::last_os_error(),
        ));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG
        || stat.st_dev != md.dev()
        || stat.st_ino != md.ino()
    {
        return Err(setup("shared lock inode changed or path is a symlink"));
    }
    Ok(())
}
fn reject_acl(file: &File, label: &str) -> Result<()> {
    for key in [
        b"system.posix_acl_access\0".as_slice(),
        b"system.posix_acl_default\0".as_slice(),
    ] {
        let rc = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                key.as_ptr().cast(),
                std::ptr::null_mut(),
                0,
            )
        };
        if rc >= 0 {
            return Err(setup(&format!("{label}: extended ACL is not supported")));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENODATA) {
            return Err(io_error(&format!("{label} ACL validation"), error));
        }
    }
    Ok(())
}
fn setup(detail: &str) -> Error {
    Error::new(ErrorKind::Io, format!("invalid shared OFD setup: {detail}"))
}
fn io_error(label: &str, e: io::Error) -> Error {
    let kind = if matches!(e.raw_os_error(), Some(libc::EACCES | libc::EPERM)) {
        ErrorKind::PermissionDenied
    } else {
        ErrorKind::Io
    };
    Error::new(kind, format!("{label}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::fs::{symlink, DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture {
        root: std::path::PathBuf,
        lock: std::path::PathBuf,
        policy: Policy,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = loop {
                let path = std::env::temp_dir().join(format!(
                    "openxcvr-ofd-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("exclusive fixture root: {e}"),
                }
            };
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let lock = root.join("shared");
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
                .unwrap();
            fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
            let md = fs::metadata(&lock).unwrap();
            let parent = fs::metadata(&root).unwrap();
            let policy = Policy {
                directory_uid: unsafe { libc::geteuid() },
                directory_gid: unsafe { libc::getegid() },
                directory_mode: 0o700,
                directory_dev: parent.dev(),
                directory_ino: parent.ino(),
                file_uid: unsafe { libc::geteuid() },
                file_gid: unsafe { libc::getegid() },
                file_mode: 0o600,
                file_dev: md.dev(),
                file_ino: md.ino(),
            };
            Self { root, lock, policy }
        }
        fn acquire(&self, device: &File) -> Result<Guard> {
            Guard::acquire(device.as_fd(), &self.lock, &self.policy)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn device() -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap()
    }
    fn kind<T>(result: Result<T>) -> ErrorKind {
        match result {
            Ok(_) => panic!("expected error"),
            Err(e) => e.kind,
        }
    }
    #[test]
    fn independent_opens_aliases_and_cleanup() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        let a = f.acquire(&dev).unwrap();
        assert_ne!(
            unsafe { libc::fcntl(a.file.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Busy);
        let alias_path = f.root.join("device-alias");
        symlink("/dev/null", &alias_path).unwrap();
        let alias = OpenOptions::new()
            .read(true)
            .write(true)
            .open(alias_path)
            .unwrap();
        assert_eq!(kind(f.acquire(&alias)), ErrorKind::Busy);
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/zero")
            .unwrap();
        let b = f.acquire(&other).unwrap();
        drop(b);
        drop(a);
        f.acquire(&dev).unwrap();
    }
    #[test]
    fn separate_process_contends() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        let a = f.acquire(&dev).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ofd_lock::tests::child_contender", "--nocapture"])
            .env("OPENXCVR_OFD_TEST_LOCK", &f.lock)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        drop(a);
        f.acquire(&dev).unwrap();
    }
    #[test]
    fn child_contender() {
        if let Some(lock) = std::env::var_os("OPENXCVR_OFD_TEST_LOCK") {
            let uid = unsafe { libc::geteuid() };
            let gid = unsafe { libc::getegid() };
            let md = fs::metadata(&lock).unwrap();
            let parent = fs::metadata(Path::new(&lock).parent().unwrap()).unwrap();
            let policy = Policy {
                directory_uid: uid,
                directory_gid: gid,
                directory_mode: 0o700,
                directory_dev: parent.dev(),
                directory_ino: parent.ino(),
                file_uid: uid,
                file_gid: gid,
                file_mode: 0o600,
                file_dev: md.dev(),
                file_ino: md.ino(),
            };
            assert_eq!(
                kind(Guard::acquire(device().as_fd(), Path::new(&lock), &policy)),
                ErrorKind::Busy
            );
        }
    }
    #[test]
    fn mapping_boundaries_and_alias_rdev() {
        let _guard = crate::test_guard();
        assert_eq!(range(0, 0).unwrap(), 0);
        assert_eq!(range(4095, 1_048_575).unwrap(), 4_294_967_295);
        assert_ne!(range(1, 0).unwrap(), range(0, 1).unwrap());
        assert_ne!(range(1, 0).unwrap(), range(2, 0).unwrap());
        assert_eq!(kind(range(4096, 0)), ErrorKind::Invalid);
        assert_eq!(kind(range(1, 1_048_576)), ErrorKind::Invalid);
        let f = Fixture::new();
        assert_eq!(
            kind(f.acquire(&File::open(&f.lock).unwrap())),
            ErrorKind::Invalid
        );
    }
    #[test]
    fn provision_and_replacement_refuse_without_fallback() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        let a = f.acquire(&dev).unwrap();
        fs::rename(&f.lock, f.root.join("old")).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&f.lock)
            .unwrap();
        fs::set_permissions(&f.lock, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io); // new inode, same mode
        fs::set_permissions(&f.lock, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        fs::remove_file(&f.lock).unwrap();
        symlink("old", &f.lock).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        fs::remove_file(&f.lock).unwrap();
        fs::hard_link(f.root.join("old"), &f.lock).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        drop(a);
    }
    #[test]
    fn parent_and_file_policy_fail_closed() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        fs::set_permissions(&f.root, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&f.lock, fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::PermissionDenied);
        fs::set_permissions(&f.lock, fs::Permissions::from_mode(0o600)).unwrap();
        let mut policy = f.policy;
        policy.file_uid = policy.file_uid.wrapping_add(1);
        assert_eq!(
            kind(Guard::acquire(dev.as_fd(), &f.lock, &policy)),
            ErrorKind::Io
        );
        let mut wrong_inode = f.policy;
        wrong_inode.file_ino = wrong_inode.file_ino.wrapping_add(1);
        assert_eq!(
            kind(Guard::acquire(dev.as_fd(), &f.lock, &wrong_inode)),
            ErrorKind::Io
        );
        let link = f.root.join("alias");
        symlink(&f.root, &link).unwrap();
        assert_eq!(
            kind(Guard::acquire(dev.as_fd(), &link.join("shared"), &f.policy)),
            ErrorKind::Io
        );
    }
    #[test]
    fn parent_replacement_refuses() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        let moved = f.root.with_extension("moved");
        fs::rename(&f.root, &moved).unwrap();
        fs::create_dir(&f.root).unwrap();
        fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Io);
        fs::remove_dir(&f.root).unwrap();
        fs::rename(moved, &f.root).unwrap();
        f.acquire(&dev).unwrap();
    }
    #[test]
    fn inherited_child_close_never_unlocks_parent() {
        let _guard = crate::test_guard();
        let f = Fixture::new();
        let dev = device();
        let guard = f.acquire(&dev).unwrap();
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            let refused = !guard.is_creator_process();
            drop(guard); // File::drop closes the inherited descriptor; no explicit unlock.
            unsafe { libc::_exit(if refused { 0 } else { 1 }) };
        }
        let mut status = 0;
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
        assert_eq!(status, 0);
        assert!(guard.is_creator_process());
        assert_eq!(kind(f.acquire(&dev)), ErrorKind::Busy);
        drop(guard);
        f.acquire(&dev).unwrap();
    }
    // The ignored parent is the only test permitted to provision administrator-owned
    // state. The separately named helper cannot recursively enter the parent.
    const CHILD_PREFIX: &str = "OPENXCVR_OFD_CHILD_";
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    fn parameter(name: &str) -> u32 {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("missing cross-UID fixture parameter {name}"))
            .parse()
            .unwrap_or_else(|_| panic!("invalid cross-UID fixture parameter {name}"))
    }
    fn measured(name: &str) -> u64 {
        std::env::var(name).unwrap().parse().unwrap()
    }
    fn reject_inherited_controls() {
        assert!(
            !std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with(CHILD_PREFIX)),
            "inherited cross-UID helper control environment"
        );
    }
    fn child(policy: &Policy, lock: &Path, uid: u32, gid: u32, role: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "ofd_lock::tests::cross_uid_helper",
                "--nocapture",
            ])
            .env_clear()
            .env("OPENXCVR_OFD_CHILD_ROLE", role)
            .env("OPENXCVR_OFD_CHILD_UID", uid.to_string())
            .env("OPENXCVR_OFD_CHILD_GID", gid.to_string())
            .env("OPENXCVR_OFD_CHILD_LOCK", lock)
            .env(
                "OPENXCVR_OFD_CHILD_DIR_UID",
                policy.directory_uid.to_string(),
            )
            .env("OPENXCVR_OFD_CHILD_FILE_UID", policy.file_uid.to_string())
            .env(
                "OPENXCVR_OFD_CHILD_DIR_MODE",
                policy.directory_mode.to_string(),
            )
            .env("OPENXCVR_OFD_CHILD_FILE_MODE", policy.file_mode.to_string())
            .env(
                "OPENXCVR_OFD_CHILD_DIR_DEV",
                policy.directory_dev.to_string(),
            )
            .env(
                "OPENXCVR_OFD_CHILD_DIR_INO",
                policy.directory_ino.to_string(),
            )
            .env("OPENXCVR_OFD_CHILD_FILE_DEV", policy.file_dev.to_string())
            .env("OPENXCVR_OFD_CHILD_FILE_INO", policy.file_ino.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if unsafe { libc::geteuid() } == 0 {
            // Explicit order: discard inherited groups before dropping root.
            unsafe {
                command.pre_exec(move || {
                    if libc::setgroups(0, std::ptr::null()) != 0
                        || libc::setgid(gid) != 0
                        || libc::setuid(uid) != 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        command
    }
    fn nonblocking(fd: i32) -> io::Result<()> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    // A child is owned from the instant spawn returns; errors and panics kill it
    // before the fixture's later Drop can remove its inode.
    struct OwnedChild(std::process::Child);
    impl OwnedChild {
        fn spawn(command: &mut Command) -> io::Result<Self> {
            let child = Self(command.spawn()?);
            nonblocking(child.0.stdout.as_ref().unwrap().as_raw_fd())?;
            nonblocking(child.0.stderr.as_ref().unwrap().as_raw_fd())?;
            Ok(child)
        }
        fn pid(&self) -> u32 {
            self.0.id()
        }
        fn ready(&mut self, deadline: std::time::Instant) -> io::Result<()> {
            let mut line = Vec::new();
            let fd = self.0.stderr.as_ref().unwrap().as_raw_fd();
            loop {
                let mut byte = [0];
                match unsafe { libc::read(fd, byte.as_mut_ptr().cast(), 1) } {
                    1 => {
                        line.push(byte[0]);
                        if byte[0] == b'\n' {
                            if line == b"OFD_HELD\n" {
                                return Ok(());
                            }
                            return Err(io::Error::other(format!(
                                "holder did not acquire: {}",
                                String::from_utf8_lossy(&line)
                            )));
                        }
                        if line.len() > 256 {
                            return Err(io::Error::other("holder readiness line too long"));
                        }
                    }
                    0 => return Err(io::Error::other("holder exited before readiness")),
                    -1 if io::Error::last_os_error().kind() == io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "holder never ready",
                            ));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    _ => return Err(io::Error::last_os_error()),
                }
            }
        }
        fn complete(&mut self, deadline: std::time::Instant, expected: &str) -> io::Result<()> {
            loop {
                if let Some(status) = self.0.try_wait()? {
                    let mut output = String::new();
                    // Pipes are nonblocking: never wait for an inherited writer.
                    match self.0.stdout.as_mut().unwrap().read_to_string(&mut output) {
                        Ok(_) => {}
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e),
                    }
                    if status.success() && output.contains(expected) {
                        if output.contains("OFD_GROUPS") && unsafe { libc::geteuid() } == 0 {
                            print!("{output}");
                        }
                        return Ok(());
                    }
                    return Err(io::Error::other(format!(
                        "child {status} missing {expected}: {output}"
                    )));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "child stalled"));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            // try_wait reaps an already exited child. On failure/unwind kill first.
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let deadline = std::time::Instant::now() + TIMEOUT;
                while std::time::Instant::now() < deadline {
                    if matches!(self.0.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                // The OS can hold an uninterruptible process. Never block teardown.
            }
        }
    }
    #[allow(clippy::too_many_arguments)] // Exact identities, roles, and observed PIDs are independent test inputs.
    fn exercise(
        policy: &Policy,
        lock: &Path,
        a: u32,
        b: u32,
        gid: u32,
        hold_role: &str,
        busy_role: &str,
        fail_after_ready: bool,
        pids: &mut Vec<u32>,
    ) -> io::Result<()> {
        let mut holder = OwnedChild::spawn(&mut child(policy, lock, a, gid, hold_role))?;
        pids.push(holder.pid());
        holder.ready(std::time::Instant::now() + TIMEOUT)?;
        if fail_after_ready {
            return Err(io::Error::other(
                "injected parent failure after holder starts",
            ));
        }
        let mut contender = OwnedChild::spawn(&mut child(policy, lock, b, gid, busy_role))?;
        pids.push(contender.pid());
        contender.complete(std::time::Instant::now() + TIMEOUT, "OFD_BUSY")?;
        drop(contender);
        drop(holder.0.stdin.take());
        holder.complete(std::time::Instant::now() + TIMEOUT, "1 passed")?;
        drop(holder);
        let mut released = OwnedChild::spawn(&mut child(policy, lock, b, gid, "released"))?;
        pids.push(released.pid());
        released.complete(std::time::Instant::now() + TIMEOUT, "OFD_ACQUIRED")
    }
    #[test]
    #[ignore = "internal helper; selected explicitly by cross-UID parent"]
    fn cross_uid_helper() {
        let role = std::env::var("OPENXCVR_OFD_CHILD_ROLE").expect("helper role required");
        if role == "never_ready" {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        if role == "stall" {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let lock = std::path::PathBuf::from(std::env::var_os("OPENXCVR_OFD_CHILD_LOCK").unwrap());
        let uid = parameter("OPENXCVR_OFD_CHILD_UID");
        let gid = parameter("OPENXCVR_OFD_CHILD_GID");
        let policy = Policy {
            directory_uid: parameter("OPENXCVR_OFD_CHILD_DIR_UID"),
            directory_gid: gid,
            directory_mode: parameter("OPENXCVR_OFD_CHILD_DIR_MODE"),
            directory_dev: measured("OPENXCVR_OFD_CHILD_DIR_DEV"),
            directory_ino: measured("OPENXCVR_OFD_CHILD_DIR_INO"),
            file_uid: parameter("OPENXCVR_OFD_CHILD_FILE_UID"),
            file_gid: gid,
            file_mode: parameter("OPENXCVR_OFD_CHILD_FILE_MODE"),
            file_dev: measured("OPENXCVR_OFD_CHILD_FILE_DEV"),
            file_ino: measured("OPENXCVR_OFD_CHILD_FILE_INO"),
        };
        assert_eq!(unsafe { libc::geteuid() }, uid);
        assert_eq!(unsafe { libc::getegid() }, gid);
        let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        assert!(count >= 0, "getgroups failed");
        let mut groups = vec![0; count as usize];
        assert_eq!(
            unsafe { libc::getgroups(count, groups.as_mut_ptr()) },
            count
        );
        println!("OFD_GROUPS primary_gid={gid} supplementary={groups:?}");
        let dev = device();
        match role.as_str() {
            "hold" => {
                let guard = Guard::acquire(dev.as_fd(), &lock, &policy).unwrap();
                eprintln!("OFD_HELD");
                std::io::stderr().flush().unwrap();
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input).unwrap();
                assert!(input.is_empty(), "unexpected release signal");
                drop(guard);
            }
            "busy" => {
                assert_eq!(
                    kind(Guard::acquire(dev.as_fd(), &lock, &policy)),
                    ErrorKind::Busy
                );
                println!("OFD_BUSY");
            }
            "released" => {
                let _guard = Guard::acquire(dev.as_fd(), &lock, &policy).unwrap();
                println!("OFD_ACQUIRED");
            }
            _ => panic!("unknown cross-UID child role"),
        }
    }
    #[test]
    fn cross_uid_parent_rejects_inherited_helper_environment() {
        let _guard = crate::test_guard();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "ofd_lock::tests::cross_uid_provisioned_fixture_required",
                "--nocapture",
            ])
            .env("OPENXCVR_OFD_CHILD_ROLE", "hold");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        let result = child.complete(std::time::Instant::now() + TIMEOUT, "1 passed");
        assert_eq!(
            result
                .expect_err("parent accepted inherited control")
                .kind(),
            io::ErrorKind::Other
        );
        let mut stderr = String::new();
        let _ = child.0.stderr.as_mut().unwrap().read_to_string(&mut stderr);
        assert!(
            stderr.contains("inherited cross-UID helper control environment"),
            "{stderr}"
        );
    }
    #[test]
    fn cross_uid_child_failures_reap_and_clean_owned_fixture() {
        let _guard = crate::test_guard();
        for (hold, busy, fail) in [
            ("never_ready", "busy", false),
            ("hold", "stall", false),
            ("hold", "busy", true),
        ] {
            let mut pids = Vec::new();
            let path;
            let start = std::time::Instant::now();
            {
                let fixture = Fixture::new();
                path = fixture.root.clone();
                let result = exercise(
                    &fixture.policy,
                    &fixture.lock,
                    fixture.policy.file_uid,
                    fixture.policy.file_uid,
                    fixture.policy.file_gid,
                    hold,
                    busy,
                    fail,
                    &mut pids,
                );
                let error = result.expect_err("fault scenario unexpectedly succeeded");
                assert_eq!(
                    error.kind(),
                    if fail {
                        io::ErrorKind::Other
                    } else {
                        io::ErrorKind::TimedOut
                    },
                    "wrong failure for {hold}/{busy}: {error}"
                );
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(12),
                "failure unbounded"
            );
            assert!(!path.exists(), "owned fixture not cleaned");
            for pid in pids {
                assert_eq!(
                    unsafe { libc::kill(pid as i32, 0) },
                    -1,
                    "child {pid} not reaped"
                );
                assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
            }
        }
    }
    #[test]
    fn cross_uid_parent_unwind_reaps_holder_before_fixture_cleanup() {
        let _guard = crate::test_guard();
        let fixture = Fixture::new();
        let path = fixture.root.clone();
        let mut pid = None;
        let start = std::time::Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut holder = OwnedChild::spawn(&mut child(
                &fixture.policy,
                &fixture.lock,
                fixture.policy.file_uid,
                fixture.policy.file_gid,
                "hold",
            ))
            .unwrap();
            pid = Some(holder.pid());
            holder.ready(std::time::Instant::now() + TIMEOUT).unwrap();
            panic!("injected parent assertion after holder starts");
        }));
        assert!(result.is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(12));
        let pid = pid.unwrap();
        assert_eq!(
            unsafe { libc::kill(pid as i32, 0) },
            -1,
            "child not reaped on unwind"
        );
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
        drop(fixture);
        assert!(!path.exists(), "owned fixture not cleaned after unwind");
    }
    #[test]
    fn cross_uid_harness_same_uid_smoke() {
        let _guard = crate::test_guard();
        let fixture = Fixture::new();
        exercise(
            &fixture.policy,
            &fixture.lock,
            fixture.policy.file_uid,
            fixture.policy.file_uid,
            fixture.policy.file_gid,
            "hold",
            "busy",
            false,
            &mut Vec::new(),
        )
        .unwrap();
    }
    #[test]
    #[ignore = "requires root and OPENXCVR_OFD_UID_A, OPENXCVR_OFD_UID_B, OPENXCVR_OFD_GID"]
    fn cross_uid_provisioned_fixture_required() {
        let _guard = crate::test_guard();
        reject_inherited_controls(); // Before privilege or fixture prerequisites.
        assert_eq!(
            unsafe { libc::geteuid() },
            0,
            "cross-UID fixture requires administrator identity"
        );
        let a = parameter("OPENXCVR_OFD_UID_A");
        let b = parameter("OPENXCVR_OFD_UID_B");
        let gid = parameter("OPENXCVR_OFD_GID");
        assert!(
            a != 0 && b != 0 && a != b && gid != 0,
            "two distinct nonroot UIDs and a nonroot shared GID required"
        );
        assert!(
            unsafe { !libc::getpwuid(a).is_null() && !libc::getpwuid(b).is_null() },
            "cross-UID fixture requires two existing accounts"
        );
        assert!(
            unsafe { !libc::getgrgid(gid).is_null() },
            "cross-UID fixture requires an existing shared group"
        );
        let tmp = fs::symlink_metadata("/tmp").unwrap();
        assert!(
            tmp.file_type().is_dir() && tmp.uid() == 0 && tmp.mode() & 0o7777 == 0o1777,
            "fixture requires a root-owned sticky /tmp"
        );
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let path = Path::new("/tmp").join(format!(
                "openxcvr-cross-uid-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => break path,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("exclusive cross-UID fixture root: {e}"),
            }
        };
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(self.0.join("shared"));
                let _ = fs::remove_dir(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let lock = root.join("shared");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&lock)
            .unwrap();
        let croot = CString::new(root.as_os_str().as_bytes()).unwrap();
        let clock = CString::new(lock.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            unsafe { libc::chown(croot.as_ptr(), 0, gid) },
            0,
            "fixture directory chown failed"
        );
        assert_eq!(
            unsafe { libc::chown(clock.as_ptr(), 0, gid) },
            0,
            "fixture file chown failed"
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o660)).unwrap();
        let dir_md = fs::metadata(&root).unwrap();
        let file_md = fs::metadata(&lock).unwrap();
        let policy = Policy {
            directory_uid: 0,
            directory_gid: gid,
            directory_mode: 0o750,
            directory_dev: dir_md.dev(),
            directory_ino: dir_md.ino(),
            file_uid: 0,
            file_gid: gid,
            file_mode: 0o660,
            file_dev: file_md.dev(),
            file_ino: file_md.ino(),
        };
        let mut pids = Vec::new();
        exercise(&policy, &lock, a, b, gid, "hold", "busy", false, &mut pids).unwrap();
        println!("cross-UID fixture: UIDs {a} -> {b}, shared GID {gid}, supplementary groups reported by each child; inode {}:{}, independent child opens; Busy then acquired after release", file_md.dev(), file_md.ino());
    }
}
