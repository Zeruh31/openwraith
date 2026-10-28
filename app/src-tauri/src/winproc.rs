//! Limited process access for the per-game profiles (design B7): the list
//! of running processes and, for one process, its start time and path. The
//! only `unsafe` of the app: the Win32 calls through `windows-sys`.
//!
//! * [`processes`] reads one Toolhelp32 snapshot of the processes
//!   (`TH32CS_SNAPPROCESS`): the process id, the parent's process id and the
//!   file name of each. It opens no process. The list decides whether a
//!   process runs: one that has exited is no longer listed even while
//!   another program still holds a handle to it (seen on Windows 11), when
//!   it could still be opened by its id.
//! * [`start_time`] (`GetProcessTimes`) and [`image_path`]
//!   (`QueryFullProcessImageNameW`) open the process with
//!   `PROCESS_QUERY_LIMITED_INFORMATION` and nothing else (no memory read,
//!   no full query right), and close the handle before they return. A
//!   process that refuses even that right (a protected process, some
//!   anti-cheat protection) or an id no process has gives `None`.
//! * Each handle is owned by an `OwnedHandle` as soon as the call returns
//!   it, so it is closed exactly once, also on an early return. The failure
//!   values (`INVALID_HANDLE_VALUE` for the snapshot, NULL for
//!   `OpenProcess`) are never used or closed.
//!
//! Elsewhere than on Windows there is no process list (`None`): the feed
//! then reports no exits and lists no programs.

#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(windows), allow(dead_code))]

use std::ffi::OsString;

pub use platform::{image_path, processes, start_time};

/// A process of the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// Its process id.
    pub pid: u32,
    /// The process id of the process that started it (which may have
    /// exited since; its id may then belong to another process).
    pub parent: u32,
    /// Its file name, without the folder.
    pub exe: OsString,
}

/// The buffer sizes tried for a path, in UTF-16 units: enough for nearly
/// every path, then the longest path Windows has (32 767 characters and the
/// NUL).
const PATH_BUFFERS: [usize; 2] = [1024, 32_768];

/// FILETIME ticks (100 ns) per second.
const TICKS_PER_SECOND: u64 = 10_000_000;

/// Seconds from 1601-01-01 (the FILETIME epoch) to 1970-01-01.
const UNIX_EPOCH_IN_FILETIME_SECONDS: u64 = 11_644_473_600;

/// The UTF-16 units before the first NUL (all of them without one).
fn until_nul(buffer: &[u16]) -> &[u16] {
    buffer.split(|unit| *unit == 0).next().unwrap_or_default()
}

/// The first `written` units of `buffer`, as an API reports them (without
/// the NUL it wrote after them); `None` when nothing was written or the
/// count does not fit the buffer with its NUL.
fn written(buffer: &[u16], written: u32) -> Option<&[u16]> {
    let len = usize::try_from(written).ok()?;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    buffer.get(..len)
}

/// A FILETIME (100 ns ticks since 1601) as seconds since the Unix epoch;
/// `None` (unknown) for a time before 1970-01-01 00:00:01, such as an
/// all-zero FILETIME.
fn unix_seconds(high: u32, low: u32) -> Option<u64> {
    let ticks = (u64::from(high) << 32) | u64::from(low);
    (ticks / TICKS_PER_SECOND)
        .checked_sub(UNIX_EPOCH_IN_FILETIME_SECONDS)
        .filter(|seconds| *seconds > 0)
}

#[cfg(windows)]
mod platform {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::{AsRawHandle, HandleOrInvalid, HandleOrNull, OwnedHandle};
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{
        ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_FILES, FALSE, FILETIME, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    use super::{PATH_BUFFERS, ProcessEntry, unix_seconds, until_nul, written};

    /// The calling thread's last Win32 error (read right after the failed
    /// call).
    fn last_error() -> Option<u32> {
        std::io::Error::last_os_error()
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok())
    }

    /// The running processes, from one snapshot; `None` when the snapshot
    /// cannot be taken or read to its end. Opens no process.
    pub fn processes() -> Option<Vec<ProcessEntry>> {
        // SAFETY: CreateToolhelp32Snapshot takes two plain values (no
        // pointer). It returns a new snapshot handle owned by nobody else,
        // or INVALID_HANDLE_VALUE.
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        // Never returned for a snapshot, but no handle either.
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is either INVALID_HANDLE_VALUE, which the
        // conversion below rejects without closing it, or the open snapshot
        // just returned, which only this function owns and which is closed
        // with CloseHandle (as its documentation says), once, on drop.
        let raw = unsafe { HandleOrInvalid::from_raw_handle(raw) };
        let snapshot = OwnedHandle::try_from(raw).ok()?;
        let size = u32::try_from(size_of::<PROCESSENTRY32W>()).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size,
            ..Default::default()
        };
        let mut list = Vec::new();
        // SAFETY: `snapshot` is an open process snapshot for the whole
        // function; `entry` is a writable PROCESSENTRY32W whose `dwSize` is
        // its own size, so the call writes inside it only.
        let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) } != FALSE;
        if !found {
            return None;
        }
        while found {
            list.push(ProcessEntry {
                pid: entry.th32ProcessID,
                parent: entry.th32ParentProcessID,
                exe: OsString::from_wide(until_nul(&entry.szExeFile)),
            });
            entry.dwSize = size;
            // SAFETY: the same snapshot, still open, and the same writable
            // `entry` with `dwSize` set to its own size.
            found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) } != FALSE;
        }
        // The end of the list, not a failure halfway (a shorter list would
        // look like exits).
        (last_error() == Some(ERROR_NO_MORE_FILES)).then_some(list)
    }

    /// `pid` opened with `PROCESS_QUERY_LIMITED_INFORMATION`, the only
    /// right the app ever asks a process for; `None` when it is refused or
    /// no process has that id. The handle is closed when dropped.
    fn open(pid: u32) -> Option<OwnedHandle> {
        // The System Idle Process cannot be opened (ERROR_INVALID_PARAMETER).
        if pid == 0 {
            return None;
        }
        // SAFETY: OpenProcess takes plain values (no pointer). It returns a
        // new process handle owned by nobody else, or NULL.
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
        // Never returned by OpenProcess; as the pseudo handle of the current
        // process it would describe OpenWraith instead.
        if raw == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: `raw` is either NULL, which the conversion below rejects
        // without closing it, or the process handle just returned, which
        // only this function owns and which is closed with CloseHandle,
        // once, on drop.
        let raw = unsafe { HandleOrNull::from_raw_handle(raw) };
        OwnedHandle::try_from(raw).ok()
    }

    /// When `pid` started, in seconds since the Unix epoch; `None` when it
    /// cannot be read.
    pub fn start_time(pid: u32) -> Option<u64> {
        let process = open(pid)?;
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: `process` is an open process handle with
        // PROCESS_QUERY_LIMITED_INFORMATION, which GetProcessTimes needs;
        // the four pointers are distinct, writable FILETIMEs that outlive
        // the call.
        let ok = unsafe {
            GetProcessTimes(
                process.as_raw_handle(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        if ok == FALSE {
            return None;
        }
        unix_seconds(created.dwHighDateTime, created.dwLowDateTime)
    }

    /// The full path of `pid`'s program (Win32 form); `None` when it cannot
    /// be read.
    pub fn image_path(pid: u32) -> Option<PathBuf> {
        let process = open(pid)?;
        for capacity in PATH_BUFFERS {
            let mut buffer = vec![0u16; capacity];
            let mut size = u32::try_from(buffer.len()).ok()?;
            // SAFETY: `process` is an open process handle with
            // PROCESS_QUERY_LIMITED_INFORMATION, which the call needs;
            // `buffer` holds exactly `size` writable UTF-16 units (the size
            // passed in) and `size` is a writable u32; both outlive the call.
            let ok = unsafe {
                QueryFullProcessImageNameW(
                    process.as_raw_handle(),
                    PROCESS_NAME_WIN32,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            };
            if ok != FALSE {
                return written(&buffer, size).map(|path| PathBuf::from(OsString::from_wide(path)));
            }
            if last_error() != Some(ERROR_INSUFFICIENT_BUFFER) {
                return None;
            }
        }
        None
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::PathBuf;

    use super::ProcessEntry;

    /// No process list elsewhere.
    pub fn processes() -> Option<Vec<ProcessEntry>> {
        None
    }

    /// Not known elsewhere.
    pub fn start_time(_pid: u32) -> Option<u64> {
        None
    }

    /// Not known elsewhere.
    pub fn image_path(_pid: u32) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_end_at_the_first_nul() {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let mut buffer = wide("game.exe");
        buffer.extend([0, b'x'.into(), 0]);
        assert_eq!(until_nul(&buffer), wide("game.exe").as_slice());
        assert_eq!(
            until_nul(&wide("no-nul.exe")),
            wide("no-nul.exe").as_slice()
        );
        assert!(until_nul(&[0, 1, 2]).is_empty());
        assert!(until_nul(&[]).is_empty());
    }

    #[test]
    fn a_written_count_must_fit_the_buffer_and_its_nul() {
        let buffer = [b'C'.into(), b':'.into(), 0, 0];
        assert_eq!(written(&buffer, 2), Some(&buffer[..2]));
        assert_eq!(written(&buffer, 3), Some(&buffer[..3]));
        assert_eq!(written(&buffer, 4), None, "no room for the NUL");
        assert_eq!(written(&buffer, 0), None);
        assert_eq!(written(&buffer, u32::MAX), None);
        assert_eq!(written(&[], 0), None);
    }

    #[test]
    fn filetimes_become_unix_seconds() {
        let split = |ticks: u64| ((ticks >> 32) as u32, ticks as u32);
        // 1970-01-01 00:00:00 + 1 s, and 2026-10-28 00:00:00.
        let (high, low) = split((UNIX_EPOCH_IN_FILETIME_SECONDS + 1) * TICKS_PER_SECOND);
        assert_eq!(unix_seconds(high, low), Some(1));
        let (high, low) =
            split((UNIX_EPOCH_IN_FILETIME_SECONDS + 1_793_145_600) * TICKS_PER_SECOND + 9_999_999);
        assert_eq!(unix_seconds(high, low), Some(1_793_145_600));
        // Nothing written, or a time before 1970: unknown.
        assert_eq!(unix_seconds(0, 0), None);
        let (high, low) = split(UNIX_EPOCH_IN_FILETIME_SECONDS * TICKS_PER_SECOND);
        assert_eq!(unix_seconds(high, low), None);
        assert_eq!(
            unix_seconds(u32::MAX, u32::MAX),
            Some(u64::MAX / TICKS_PER_SECOND - UNIX_EPOCH_IN_FILETIME_SECONDS)
        );
    }

    /// Windows only: these read the real process list and open processes
    /// with the limited right (this test process and its own child only).
    #[cfg(windows)]
    mod windows {
        use std::path::Path;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        use super::super::*;

        fn listed(pid: u32) -> Option<ProcessEntry> {
            processes()
                .expect("the process snapshot")
                .into_iter()
                .find(|p| p.pid == pid)
        }

        #[test]
        fn this_process_is_listed_with_its_name_start_and_path() {
            let pid = std::process::id();
            let own = std::env::current_exe().expect("the test's own path");
            let entry = listed(pid).expect("this process is listed");
            assert!(
                own.file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&entry.exe)),
                "{entry:?} vs {own:?}"
            );
            let start = start_time(pid).expect("the start time");
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("after 1970")
                .as_secs();
            assert!(start > 0 && start <= now + 1, "{start} vs {now}");
            let path = image_path(pid).expect("the path");
            assert!(path.is_absolute(), "{path:?}");
            assert!(
                path.file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&entry.exe)),
                "{path:?}"
            );
            assert_eq!(
                std::fs::canonicalize(&path).ok(),
                std::fs::canonicalize(&own).ok()
            );
        }

        #[test]
        fn pid_0_and_a_pid_that_does_not_exist_give_nothing() {
            assert_eq!(start_time(0), None);
            assert_eq!(image_path(0), None);
            // Windows process ids are multiples of 4 and far below this.
            let missing = u32::MAX - 3;
            assert!(listed(missing).is_none());
            assert_eq!(start_time(missing), None);
            assert_eq!(image_path(missing), None);
        }

        #[test]
        fn a_child_is_found_and_then_gone_after_it_is_killed() {
            let mut child = Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("ping starts");
            let pid = child.id();
            let entry = listed(pid);
            let start = start_time(pid);
            let path = image_path(pid);
            // Kill it before any assertion can fail.
            let _ = child.kill();
            let _ = child.wait();
            drop(child);

            let entry = entry.expect("the child is listed");
            assert!(entry.exe.eq_ignore_ascii_case("ping.exe"), "{entry:?}");
            assert_eq!(entry.parent, std::process::id());
            assert!(start.is_some_and(|s| s > 0), "{start:?}");
            let path = path.expect("the child's path");
            assert!(
                path.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("ping.exe"))
                    && path.parent().is_some_and(Path::is_absolute),
                "{path:?}"
            );

            // Gone once Windows has removed it (no handle of ours is left).
            let deadline = Instant::now() + Duration::from_secs(10);
            while listed(pid).is_some_and(|p| p.exe.eq_ignore_ascii_case("ping.exe")) {
                assert!(
                    Instant::now() < deadline,
                    "the killed child is still listed"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
