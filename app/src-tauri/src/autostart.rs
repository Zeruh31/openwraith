//! "Start with Windows" (design B5): OpenWraith's own code over the
//! per-user Run key, through the safe API of `windows-registry` (no
//! `unsafe` here).
//!
//! * The entry is the value `OpenWraith` in
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`: a `REG_SZ`
//!   `"<full exe path>" --tray`, the path always quoted (unquoted, a path
//!   with spaces names another program first) and the command line at most
//!   260 characters (Microsoft Learn, "Run and RunOnce Registry Keys").
//!   Never the machine-wide `HKLM` key, also not when OpenWraith runs as
//!   administrator. Turning it off deletes the value; a missing value is
//!   fine.
//! * Task Manager > Startup apps keeps the user's choice in
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`,
//!   value `OpenWraith` (`REG_BINARY`). Microsoft does not document that
//!   format (none of its pages describes it, as far as we know; Microsoft
//!   Learn checked on 2026-10-28), so [`task_manager_allows`] follows what
//!   was observed: first byte 0x02/0x06 = enabled, 0x03/0x07 = disabled; an
//!   odd first byte means disabled; a missing value means enabled.
//!   OpenWraith only reads it and never writes it: it is the user's choice,
//!   and turning the switch off and on does not change it. Settings >
//!   Background shows what Windows will do instead ([`WindowsStartState`],
//!   read again every time the settings are read or saved, through the
//!   core's `ShellHook`).
//! * It needs the tray icon: the core turns it off with the tray. A start
//!   without the tray shows the window anyway
//!   ([`window::starts_hidden`](crate::window::starts_hidden)).
//! * At start, [`reconcile`] makes the entry match the settings: wanted and
//!   missing or starting another program path (an older unquoted entry,
//!   another copy) -> written; not wanted and present -> deleted. It reads
//!   first, so a start writes only when the entry differs. A start from the
//!   Run entry itself (`--tray`) never writes the Run key while Windows
//!   still processes it at logon (the same Microsoft Learn page: a program
//!   run from these keys should not write to the key during its execution):
//!   it waits [`LOGON_DELAY`] and reads the settings again first. The check
//!   reads the settings under the lock the switch changes the entry under,
//!   so a switch turned meanwhile has the last word.
//! * A developer simulator build never touches the registry: its changes
//!   are accepted and ignored (an entry would start the debug build at every
//!   logon) and the state is `unavailable`. Test builds cannot reach the
//!   real keys at all (`registry` has none for them): the tests use a fake
//!   registry, and the one test of the real API a key under
//!   `HKCU\Software\OpenWraithTests` that it removes again.
//! * The NSIS uninstaller deletes the Run value (`windows/hooks.nsh`; not
//!   when it runs for an update), and Task Manager's value only with the
//!   application data: a newer setup program uninstalls the old version
//!   first and keeps the settings, so the next start writes the Run value
//!   again, and an entry disabled in Task Manager must stay disabled.
//!   Windows Installer has no standard way to delete a single value on
//!   uninstall, so the MSI leaves the Run value (the README says how to
//!   remove it).

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use openwraith_app_core::dto::{AppError, AppErrorKind, WindowsStartState};

/// How long a start from the Run entry waits before it checks the entry.
pub const LOGON_DELAY: Duration = Duration::from_secs(120);

// The keys, the value name and the values read are used by the real
// registry only, which simulator builds and builds off Windows do not have
// (the README builds the workspace on Linux too).

/// The per-user Run key (under `HKCU`).
#[cfg_attr(any(not(windows), feature = "sim"), allow(dead_code))]
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Where Task Manager keeps the user's choice for each Run value (under
/// `HKCU`; read only).
#[cfg_attr(any(not(windows), feature = "sim"), allow(dead_code))]
pub const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// The name of OpenWraith's value in both keys (the NSIS uninstaller hook
/// deletes the same name).
#[cfg_attr(any(not(windows), all(feature = "sim", not(test))), allow(dead_code))]
pub const VALUE_NAME: &str = "OpenWraith";

/// The longest command line a Run value may hold.
pub const MAX_COMMAND_LINE: usize = 260;

/// When the start-up check of the Run entry happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileWhen {
    /// Now (a normal start).
    Now,
    /// After [`LOGON_DELAY`], with the settings read again (a start from
    /// the Run entry at logon).
    Later,
}

/// When to check the Run entry at start (`args`: the program's arguments).
pub fn reconcile_when<I, S>(args: I) -> ReconcileWhen
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if args
        .into_iter()
        .any(|a| a.as_ref() == crate::window::TRAY_FLAG)
    {
        ReconcileWhen::Later
    } else {
        ReconcileWhen::Now
    }
}

/// The Run value for the program at `exe`: the path in quotes, then
/// `--tray`. `None` when no entry can start it: an empty path, a path with
/// a quote, or a command line longer than [`MAX_COMMAND_LINE`] characters.
pub fn run_command(exe: &std::path::Path) -> Option<String> {
    let path = exe.to_str()?;
    if path.is_empty() || path.contains('"') {
        return None;
    }
    let command = format!("\"{path}\" {}", crate::window::TRAY_FLAG);
    (command.encode_utf16().count() <= MAX_COMMAND_LINE).then_some(command)
}

/// Whether the Run value `value` is the entry `command` (Windows paths
/// ignore case).
pub fn starts_this(value: &str, command: &str) -> bool {
    value.trim().to_lowercase() == command.to_lowercase()
}

/// Whether Task Manager lets Windows start the entry, from the bytes of the
/// `StartupApproved\Run` value (`None`: no value). Not documented by
/// Microsoft; observed: first byte 0x02/0x06 = enabled, 0x03/0x07 =
/// disabled; an odd first byte means disabled; a missing (or empty) value
/// means enabled.
pub fn task_manager_allows(approved: Option<&[u8]>) -> bool {
    approved
        .and_then(|bytes| bytes.first())
        .is_none_or(|first| first % 2 == 0)
}

/// The Run value `OpenWraith` as read (only the real registry reads values,
/// which simulator builds and builds off Windows do not have).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(any(not(windows), all(feature = "sim", not(test))), allow(dead_code))]
pub enum RunValue {
    /// There is none.
    Missing,
    /// A `REG_SZ` command line.
    Text(String),
    /// A value of another type, or text that is not valid UTF-16: not one
    /// OpenWraith wrote (Windows may still run it).
    Other,
}

/// What the start-up check changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing: the entry matches the settings (or no entry can be made).
    Nothing,
    /// Write the entry.
    Write,
    /// Delete the entry.
    Remove,
}

/// The start-up check's decision: `wanted` by the settings, the Run value
/// `run` as read, and this program's entry `command` (`None`: none can be
/// made).
pub fn reconcile_change(wanted: bool, run: &RunValue, command: Option<&str>) -> Change {
    match (wanted, run, command) {
        (false, RunValue::Missing, _) => Change::Nothing,
        (false, _, _) => Change::Remove,
        (true, _, None) => Change::Nothing,
        (true, RunValue::Text(value), Some(command)) if starts_this(value, command) => {
            Change::Nothing
        }
        (true, _, Some(_)) => Change::Write,
    }
}

/// What Windows will do at the next sign-in: `wanted` by the settings, the
/// Run value `run`, the Task Manager bytes `approved`, this program's entry
/// `command`.
pub fn start_state(
    wanted: bool,
    run: &RunValue,
    approved: Option<&[u8]>,
    command: Option<&str>,
) -> WindowsStartState {
    let allowed = task_manager_allows(approved);
    if !wanted {
        return if *run == RunValue::Missing || !allowed {
            WindowsStartState::Off
        } else {
            WindowsStartState::StillOn
        };
    }
    let Some(command) = command else {
        return WindowsStartState::Unavailable;
    };
    match run {
        RunValue::Text(value) if starts_this(value, command) => {
            if allowed {
                WindowsStartState::On
            } else {
                WindowsStartState::OffInTaskManager
            }
        }
        _ => WindowsStartState::EntryMissing,
    }
}

/// The two values OpenWraith reads and the one it writes (`HKCU` only),
/// behind a seam: the tests use a fake.
pub trait Registry {
    /// The Run value `OpenWraith`.
    fn run_value(&self) -> Result<RunValue, String>;
    /// The bytes of the `StartupApproved\Run` value `OpenWraith`, if there
    /// is a `REG_BINARY` one.
    fn approved_value(&self) -> Result<Option<Vec<u8>>, String>;
    /// Writes the Run value (`REG_SZ`).
    fn write_run_value(&self, command: &str) -> Result<(), String>;
    /// Deletes the Run value; a missing value (or key) is fine.
    fn remove_run_value(&self) -> Result<(), String>;
}

fn error(message: String) -> AppError {
    AppError::new(AppErrorKind::Io, message)
}

/// Adds (`true`, always written: the user asked) or removes (`false`) the
/// entry `command` in `registry`. What Windows will do afterwards is
/// [`state_in`]'s answer, not an error.
pub fn change_in(
    registry: &dyn Registry,
    command: Option<&str>,
    enabled: bool,
) -> Result<(), AppError> {
    if !enabled {
        return registry
            .remove_run_value()
            .map_err(|e| error(format!("cannot remove the Windows start entry: {e}")));
    }
    let Some(command) = command else {
        return Err(error(format!(
            "cannot add the Windows start entry: the path of OpenWraith is unknown or makes the \
             command line longer than {MAX_COMMAND_LINE} characters"
        )));
    };
    registry
        .write_run_value(command)
        .map_err(|e| error(format!("cannot write the Windows start entry: {e}")))
}

/// The start-up check in `registry`: reads, then writes or deletes only
/// what [`reconcile_change`] says. Returns the change made.
pub fn reconcile_in(
    registry: &dyn Registry,
    command: Option<&str>,
    wanted: bool,
) -> Result<Change, String> {
    let run = registry.run_value()?;
    let change = reconcile_change(wanted, &run, command);
    match (change, command) {
        (Change::Write, Some(command)) => registry.write_run_value(command)?,
        (Change::Remove, _) => registry.remove_run_value()?,
        _ => {}
    }
    Ok(change)
}

/// Serialises the changes of the entry: the switch ([`set_enabled`]) and
/// the start-up check ([`reconcile`]).
static RUN_ENTRY: Mutex<()> = Mutex::new(());

/// [`change_in`] under `lock`.
pub fn change_under(
    lock: &Mutex<()>,
    registry: &dyn Registry,
    command: Option<&str>,
    enabled: bool,
) -> Result<(), AppError> {
    let _entry = lock.lock().unwrap_or_else(PoisonError::into_inner);
    change_in(registry, command, enabled)
}

/// [`reconcile_in`] with the settings read by `wanted` under `lock`. The
/// settings are saved before the switch changes the entry under the same
/// lock, so either the check reads the new setting or the switch's change
/// comes after the check's: a switch turned while the check runs is never
/// undone.
pub fn reconcile_under(
    lock: &Mutex<()>,
    registry: &dyn Registry,
    command: Option<&str>,
    wanted: impl FnOnce() -> bool,
) -> Result<Change, String> {
    let _entry = lock.lock().unwrap_or_else(PoisonError::into_inner);
    reconcile_in(registry, command, wanted())
}

/// What Windows will do at the next sign-in, read from `registry` (reads
/// only; `unavailable` when a read fails).
pub fn state_in(registry: &dyn Registry, command: Option<&str>, wanted: bool) -> WindowsStartState {
    match (registry.run_value(), registry.approved_value()) {
        (Ok(run), Ok(approved)) => start_state(wanted, &run, approved.as_deref(), command),
        _ => WindowsStartState::Unavailable,
    }
}

/// The registry of this build: the user's real keys in a build for the
/// keyboard on Windows; none in simulator builds, in test builds (no test
/// can reach the real Run or StartupApproved key) and off Windows.
#[cfg(all(windows, not(test), not(feature = "sim")))]
fn registry() -> Option<Box<dyn Registry>> {
    Some(Box::new(UserRegistry::real()))
}

/// The registry of this build: none here (simulator, test, not Windows).
#[cfg(not(all(windows, not(test), not(feature = "sim"))))]
fn registry() -> Option<Box<dyn Registry>> {
    None
}

/// This program's Run value (`None`: no entry can start it).
fn this_command() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| run_command(&exe))
}

/// Adds (`true`) or removes (`false`) the Run entry, as the user chose in
/// Settings > Background. Simulator builds: accepted and ignored.
pub fn set_enabled(enabled: bool) -> Result<(), AppError> {
    if cfg!(feature = "sim") {
        return Ok(());
    }
    match registry() {
        Some(registry) => change_under(
            &RUN_ENTRY,
            registry.as_ref(),
            this_command().as_deref(),
            enabled,
        ),
        None => Err(error(
            "start with Windows is not available in this build".to_string(),
        )),
    }
}

/// At start: makes the entry match the settings, which `wanted` reads
/// (whether they want the entry; read under the switch's lock, see
/// [`reconcile_under`]). Best effort: a failure shows as the state in
/// Settings > Background.
pub fn reconcile(wanted: impl FnOnce() -> bool) {
    if let Some(registry) = registry() {
        let _ = reconcile_under(
            &RUN_ENTRY,
            registry.as_ref(),
            this_command().as_deref(),
            wanted,
        );
    }
}

/// What Windows will do at the next sign-in (`wanted`: the settings want
/// the entry). `unavailable` in simulator builds.
pub fn state(wanted: bool) -> WindowsStartState {
    registry().map_or(WindowsStartState::Unavailable, |registry| {
        state_in(registry.as_ref(), this_command().as_deref(), wanted)
    })
}

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`: no such key or value.
#[cfg(all(windows, any(test, not(feature = "sim"))))]
const NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// The current user's registry (`HKCU`) at the real keys or, in tests,
/// under a test key. Opens the Run key for writing only to set or delete
/// OpenWraith's value; opens the StartupApproved key for reading only.
#[cfg(all(windows, any(test, not(feature = "sim"))))]
pub struct UserRegistry {
    run_key: String,
    approved_key: String,
}

#[cfg(all(windows, any(test, not(feature = "sim"))))]
impl UserRegistry {
    /// The real keys (not in test builds).
    #[cfg(not(test))]
    fn real() -> Self {
        UserRegistry {
            run_key: RUN_KEY.to_string(),
            approved_key: APPROVED_KEY.to_string(),
        }
    }
}

#[cfg(all(windows, any(test, not(feature = "sim"))))]
impl Registry for UserRegistry {
    fn run_value(&self) -> Result<RunValue, String> {
        use windows_registry::{CURRENT_USER, Type};
        let key = match CURRENT_USER.open(&self.run_key) {
            Ok(key) => key,
            Err(e) if e.code().0 == NOT_FOUND => return Ok(RunValue::Missing),
            Err(e) => return Err(e.to_string()),
        };
        match key.get_value(VALUE_NAME) {
            Ok(value) if value.ty() == Type::String => {
                Ok(String::try_from(value).map_or(RunValue::Other, RunValue::Text))
            }
            Ok(_) => Ok(RunValue::Other),
            Err(e) if e.code().0 == NOT_FOUND => Ok(RunValue::Missing),
            Err(e) => Err(e.to_string()),
        }
    }

    fn approved_value(&self) -> Result<Option<Vec<u8>>, String> {
        use windows_registry::{CURRENT_USER, Type};
        let key = match CURRENT_USER.open(&self.approved_key) {
            Ok(key) => key,
            Err(e) if e.code().0 == NOT_FOUND => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        match key.get_value(VALUE_NAME) {
            Ok(value) if value.ty() == Type::Bytes => Ok(Some(value.to_vec())),
            Ok(_) => Ok(None),
            Err(e) if e.code().0 == NOT_FOUND => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write_run_value(&self, command: &str) -> Result<(), String> {
        use windows_registry::CURRENT_USER;
        CURRENT_USER
            .options()
            .write()
            .create()
            .open(&self.run_key)
            .and_then(|key| key.set_string(VALUE_NAME, command))
            .map_err(|e| e.to_string())
    }

    fn remove_run_value(&self) -> Result<(), String> {
        use windows_registry::CURRENT_USER;
        match CURRENT_USER
            .options()
            .write()
            .open(&self.run_key)
            .and_then(|key| key.remove_value(VALUE_NAME))
        {
            Ok(()) => Ok(()),
            Err(e) if e.code().0 == NOT_FOUND => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;

    use super::*;

    const EXE: &str = r"C:\Program Files\OpenWraith\openwraith-app.exe";
    const COMMAND: &str = r#""C:\Program Files\OpenWraith\openwraith-app.exe" --tray"#;

    /// A registry in memory: what it holds, what was written, and whether
    /// reads or writes fail.
    #[derive(Default)]
    struct Fake {
        run: RefCell<Option<RunValue>>,
        approved: Option<Vec<u8>>,
        writes: RefCell<Vec<String>>,
        removes: RefCell<usize>,
        read_fails: bool,
        write_fails: bool,
    }

    impl Fake {
        fn holding(run: Option<RunValue>, approved: Option<&[u8]>) -> Self {
            Fake {
                run: RefCell::new(run),
                approved: approved.map(<[u8]>::to_vec),
                ..Fake::default()
            }
        }
    }

    impl Registry for Fake {
        fn run_value(&self) -> Result<RunValue, String> {
            if self.read_fails {
                return Err("access denied".to_string());
            }
            Ok(self.run.borrow().clone().unwrap_or(RunValue::Missing))
        }

        fn approved_value(&self) -> Result<Option<Vec<u8>>, String> {
            if self.read_fails {
                return Err("access denied".to_string());
            }
            Ok(self.approved.clone())
        }

        fn write_run_value(&self, command: &str) -> Result<(), String> {
            if self.write_fails {
                return Err("access denied".to_string());
            }
            self.writes.borrow_mut().push(command.to_string());
            *self.run.borrow_mut() = Some(RunValue::Text(command.to_string()));
            Ok(())
        }

        fn remove_run_value(&self) -> Result<(), String> {
            if self.write_fails {
                return Err("access denied".to_string());
            }
            *self.removes.borrow_mut() += 1;
            *self.run.borrow_mut() = None;
            Ok(())
        }
    }

    fn text(value: &str) -> RunValue {
        RunValue::Text(value.to_string())
    }

    #[test]
    fn a_start_from_the_run_entry_checks_it_only_later() {
        let tray = crate::window::TRAY_FLAG;
        assert_eq!(
            reconcile_when(["openwraith-app.exe", tray]),
            ReconcileWhen::Later
        );
        assert_eq!(reconcile_when(["openwraith-app.exe"]), ReconcileWhen::Now);
    }

    #[test]
    fn the_run_value_quotes_the_path_and_fits_a_run_entry() {
        assert_eq!(run_command(Path::new(EXE)).as_deref(), Some(COMMAND));
        // Without spaces too: always quoted.
        assert_eq!(
            run_command(Path::new(r"D:\OW\openwraith-app.exe")).as_deref(),
            Some(r#""D:\OW\openwraith-app.exe" --tray"#)
        );
        // Characters outside ASCII are kept as they are.
        assert_eq!(
            run_command(Path::new(r"C:\Users\Çağrı\OpenWraith\openwraith-app.exe")).as_deref(),
            Some(r#""C:\Users\Çağrı\OpenWraith\openwraith-app.exe" --tray"#)
        );
        assert_eq!(run_command(Path::new("")), None);
        assert_eq!(run_command(Path::new(r#"C:\a"b\openwraith-app.exe"#)), None);
        // At most 260 characters with the quotes and ` --tray` (9 more).
        let fits = format!(r"C:\{}.exe", "a".repeat(MAX_COMMAND_LINE - 9 - 7));
        let command = run_command(Path::new(&fits)).unwrap();
        assert_eq!(command.encode_utf16().count(), MAX_COMMAND_LINE);
        let too_long = format!(r"C:\{}.exe", "a".repeat(MAX_COMMAND_LINE - 9 - 6));
        assert_eq!(run_command(Path::new(&too_long)), None);
    }

    #[test]
    fn the_entry_is_recognised_ignoring_case_only() {
        assert!(starts_this(COMMAND, COMMAND));
        assert!(starts_this(&COMMAND.to_uppercase(), COMMAND));
        assert!(starts_this(&format!(" {COMMAND} "), COMMAND));
        // The plugin of v0.2's first builds wrote the path without quotes.
        assert!(!starts_this(&format!("{EXE} --tray"), COMMAND));
        assert!(!starts_this(
            r#""D:\Old\openwraith-app.exe" --tray"#,
            COMMAND
        ));
        assert!(!starts_this(
            r#""C:\Program Files\OpenWraith\openwraith-app.exe""#,
            COMMAND
        ));
    }

    #[test]
    fn task_manager_bytes_are_read_by_their_first_byte() {
        let stamp = [0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0x01];
        let with = |first: u8| {
            let mut bytes = vec![first, 0, 0, 0];
            bytes.extend_from_slice(&stamp);
            bytes
        };
        assert!(task_manager_allows(None));
        assert!(task_manager_allows(Some(&[])));
        assert!(task_manager_allows(Some(&with(0x02))));
        assert!(task_manager_allows(Some(&with(0x06))));
        assert!(task_manager_allows(Some(&[0x02; 12])));
        assert!(!task_manager_allows(Some(&with(0x03))));
        assert!(!task_manager_allows(Some(&with(0x07))));
        assert!(!task_manager_allows(Some(&[0x01])));
    }

    #[test]
    fn the_start_check_writes_or_deletes_only_what_differs() {
        let ours = text(COMMAND);
        let unquoted = text(&format!("{EXE} --tray"));
        let elsewhere = text(r#""D:\Old\openwraith-app.exe" --tray"#);
        let cmd = Some(COMMAND);
        // Wanted: written when missing, another program path, or not text.
        assert_eq!(reconcile_change(true, &ours, cmd), Change::Nothing);
        assert_eq!(
            reconcile_change(true, &RunValue::Missing, cmd),
            Change::Write
        );
        assert_eq!(reconcile_change(true, &unquoted, cmd), Change::Write);
        assert_eq!(reconcile_change(true, &elsewhere, cmd), Change::Write);
        assert_eq!(reconcile_change(true, &RunValue::Other, cmd), Change::Write);
        // No usable path: nothing is written.
        assert_eq!(
            reconcile_change(true, &RunValue::Missing, None),
            Change::Nothing
        );
        // Not wanted: any value of that name goes, whatever it starts.
        assert_eq!(
            reconcile_change(false, &RunValue::Missing, cmd),
            Change::Nothing
        );
        for run in [&ours, &unquoted, &elsewhere, &RunValue::Other] {
            assert_eq!(reconcile_change(false, run, cmd), Change::Remove);
            assert_eq!(reconcile_change(false, run, None), Change::Remove);
        }
    }

    #[test]
    fn the_state_says_what_windows_will_do() {
        use WindowsStartState as S;
        let ours = text(COMMAND);
        let elsewhere = text(r#""D:\Old\openwraith-app.exe" --tray"#);
        let off = [0x03, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        let on = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let cmd = Some(COMMAND);
        assert_eq!(start_state(true, &ours, None, cmd), S::On);
        assert_eq!(start_state(true, &ours, Some(&on), cmd), S::On);
        assert_eq!(
            start_state(true, &ours, Some(&off), cmd),
            S::OffInTaskManager
        );
        assert_eq!(
            start_state(true, &RunValue::Missing, None, cmd),
            S::EntryMissing
        );
        assert_eq!(start_state(true, &elsewhere, None, cmd), S::EntryMissing);
        assert_eq!(
            start_state(true, &RunValue::Other, None, cmd),
            S::EntryMissing
        );
        assert_eq!(start_state(true, &ours, None, None), S::Unavailable);
        assert_eq!(start_state(false, &RunValue::Missing, None, cmd), S::Off);
        assert_eq!(start_state(false, &ours, Some(&off), cmd), S::Off);
        assert_eq!(start_state(false, &ours, None, cmd), S::StillOn);
        assert_eq!(start_state(false, &elsewhere, Some(&on), None), S::StillOn);
    }

    #[test]
    fn turning_it_on_writes_the_quoted_entry_and_off_deletes_it() {
        let registry = Fake::default();
        change_in(&registry, Some(COMMAND), true).unwrap();
        assert_eq!(*registry.writes.borrow(), [COMMAND]);
        assert_eq!(
            state_in(&registry, Some(COMMAND), true),
            WindowsStartState::On
        );
        change_in(&registry, Some(COMMAND), false).unwrap();
        assert_eq!(registry.run_value().unwrap(), RunValue::Missing);
        assert_eq!(
            state_in(&registry, Some(COMMAND), false),
            WindowsStartState::Off
        );
        // Deleting a missing value is fine.
        change_in(&registry, Some(COMMAND), false).unwrap();
        assert_eq!(*registry.removes.borrow(), 2);
        // Without a usable path nothing is written, and it says why.
        let error = change_in(&registry, None, true).unwrap_err();
        assert!(error.message.contains("260"), "{}", error.message);
        assert_eq!(registry.writes.borrow().len(), 1);
        // A failed write is an error; the state shows the entry missing.
        let failing = Fake {
            write_fails: true,
            ..Fake::default()
        };
        assert!(change_in(&failing, Some(COMMAND), true).is_err());
        assert!(change_in(&failing, Some(COMMAND), false).is_err());
        assert_eq!(
            state_in(&failing, Some(COMMAND), true),
            WindowsStartState::EntryMissing
        );
    }

    #[test]
    fn turning_it_on_never_overrides_task_manager() {
        // Task Manager has it off: the entry is written, the choice stays,
        // and the state says so (the fake has no StartupApproved write).
        let registry = Fake::holding(None, Some(&[0x03, 0, 0, 0, 9, 9, 9, 9, 9, 9, 9, 9]));
        change_in(&registry, Some(COMMAND), true).unwrap();
        assert_eq!(
            state_in(&registry, Some(COMMAND), true),
            WindowsStartState::OffInTaskManager
        );
        change_in(&registry, Some(COMMAND), false).unwrap();
        change_in(&registry, Some(COMMAND), true).unwrap();
        assert_eq!(
            state_in(&registry, Some(COMMAND), true),
            WindowsStartState::OffInTaskManager
        );
    }

    #[test]
    fn the_start_check_reads_first_and_changes_only_what_differs() {
        // As wanted: nothing written.
        let registry = Fake::holding(Some(text(COMMAND)), None);
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), true).unwrap(),
            Change::Nothing
        );
        assert!(registry.writes.borrow().is_empty());
        assert_eq!(*registry.removes.borrow(), 0);
        // An unquoted entry of an earlier build: written anew, quoted.
        let registry = Fake::holding(Some(text(&format!("{EXE} --tray"))), None);
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), true).unwrap(),
            Change::Write
        );
        assert_eq!(*registry.writes.borrow(), [COMMAND]);
        // Missing: written. Left behind while not wanted: deleted.
        let registry = Fake::default();
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), true).unwrap(),
            Change::Write
        );
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), false).unwrap(),
            Change::Remove
        );
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), false).unwrap(),
            Change::Nothing
        );
        // Task Manager's choice is not a reason to write.
        let registry = Fake::holding(Some(text(COMMAND)), Some(&[0x07]));
        assert_eq!(
            reconcile_in(&registry, Some(COMMAND), true).unwrap(),
            Change::Nothing
        );
        // A failed read changes nothing; the state is unavailable.
        let registry = Fake {
            read_fails: true,
            ..Fake::default()
        };
        assert!(reconcile_in(&registry, Some(COMMAND), true).is_err());
        assert!(registry.writes.borrow().is_empty());
        assert_eq!(
            state_in(&registry, Some(COMMAND), true),
            WindowsStartState::Unavailable
        );
    }

    #[test]
    fn test_builds_never_reach_the_real_keys() {
        // The real keys are per user; test builds have no registry with them.
        for key in [RUN_KEY, APPROVED_KEY] {
            assert!(key.starts_with(r"Software\Microsoft\Windows\CurrentVersion\"));
            assert!(key.ends_with(r"\Run"));
        }
        assert!(registry().is_none());
        assert_eq!(state(true), WindowsStartState::Unavailable);
        assert_eq!(state(false), WindowsStartState::Unavailable);
        reconcile(|| true);
        reconcile(|| false);
    }

    /// The fake for two threads: what the Run value holds.
    #[derive(Default)]
    struct Shared(Mutex<Option<String>>);

    impl Registry for Shared {
        fn run_value(&self) -> Result<RunValue, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .clone()
                .map_or(RunValue::Missing, RunValue::Text))
        }

        fn approved_value(&self) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }

        fn write_run_value(&self, command: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(command.to_string());
            Ok(())
        }

        fn remove_run_value(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn a_switch_turned_while_the_start_check_runs_has_the_last_word() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;

        // The logon check reads "wanted"; right then the user turns the
        // switch off (saved first, then the entry removed). Without the lock
        // the check would write the entry back after the removal.
        let lock = Mutex::new(());
        let registry = Shared::default();
        let wanted = AtomicBool::new(true);
        let (read, was_read) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let change = reconcile_under(&lock, &registry, Some(COMMAND), || {
                    let now = wanted.load(Ordering::SeqCst);
                    read.send(()).unwrap();
                    // The switch's change gets every chance to run first.
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    now
                });
                assert_eq!(change, Ok(Change::Write));
            });
            was_read.recv().unwrap();
            wanted.store(false, Ordering::SeqCst);
            change_under(&lock, &registry, Some(COMMAND), false).unwrap();
        });
        assert_eq!(registry.run_value().unwrap(), RunValue::Missing);

        // The other way round: turned on while a check that read "off" runs.
        let wanted = AtomicBool::new(false);
        let (read, was_read) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let change = reconcile_under(&lock, &registry, Some(COMMAND), || {
                    let now = wanted.load(Ordering::SeqCst);
                    read.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    now
                });
                assert_eq!(change, Ok(Change::Nothing));
            });
            was_read.recv().unwrap();
            wanted.store(true, Ordering::SeqCst);
            change_under(&lock, &registry, Some(COMMAND), true).unwrap();
        });
        assert_eq!(
            registry.run_value().unwrap(),
            RunValue::Text(COMMAND.to_string())
        );
    }

    /// The real API, under `HKCU\Software\OpenWraithTests\<unique>` only:
    /// removed at the end, also when the test fails.
    #[cfg(windows)]
    mod real {
        use windows_registry::{CURRENT_USER, Type};

        use super::super::*;
        use super::COMMAND;

        const TEST_ROOT: &str = r"Software\OpenWraithTests";

        /// The test's own key; removed when dropped.
        struct TestKey(String);

        impl TestKey {
            fn new() -> Self {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default();
                TestKey(format!(
                    r"{TEST_ROOT}\autostart-{}-{nanos}",
                    std::process::id()
                ))
            }

            fn registry(&self) -> UserRegistry {
                assert!(self.0.starts_with(r"Software\OpenWraithTests\"));
                UserRegistry {
                    run_key: format!(r"{}\Run", self.0),
                    approved_key: format!(r"{}\StartupApproved\Run", self.0),
                }
            }

            fn exists(&self) -> bool {
                CURRENT_USER.open(&self.0).is_ok()
            }
        }

        impl Drop for TestKey {
            fn drop(&mut self) {
                let _ = CURRENT_USER.remove_tree(&self.0);
                // The common parent too, once no test key is left in it.
                let empty = CURRENT_USER.open(TEST_ROOT).is_ok_and(|root| {
                    root.keys().is_ok_and(|mut k| k.next().is_none())
                        && root.values().is_ok_and(|mut v| v.next().is_none())
                });
                if empty {
                    let _ = CURRENT_USER.remove_tree(TEST_ROOT);
                }
            }
        }

        #[test]
        fn the_user_registry_writes_reads_and_deletes_only_its_value() {
            let key = TestKey::new();
            let registry = key.registry();
            // Nothing there yet (not even the keys).
            assert_eq!(registry.run_value().unwrap(), RunValue::Missing);
            assert_eq!(registry.approved_value().unwrap(), None);
            registry.remove_run_value().unwrap();
            assert_eq!(
                state_in(&registry, Some(COMMAND), true),
                WindowsStartState::EntryMissing
            );

            change_in(&registry, Some(COMMAND), true).unwrap();
            // The test's own handle on its Run key (read and write).
            let run = CURRENT_USER.create(format!(r"{}\Run", key.0)).unwrap();
            assert_eq!(run.get_type(VALUE_NAME).unwrap(), Type::String);
            assert_eq!(run.get_string(VALUE_NAME).unwrap(), COMMAND);
            assert_eq!(
                registry.run_value().unwrap(),
                RunValue::Text(COMMAND.to_string())
            );
            assert_eq!(
                state_in(&registry, Some(COMMAND), true),
                WindowsStartState::On
            );

            // The test plays Task Manager, in its own key.
            let approved = CURRENT_USER
                .create(format!(r"{}\StartupApproved\Run", key.0))
                .unwrap();
            let disabled = [
                0x03, 0, 0, 0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0x01,
            ];
            approved
                .set_bytes(VALUE_NAME, Type::Bytes, &disabled)
                .unwrap();
            assert_eq!(registry.approved_value().unwrap(), Some(disabled.to_vec()));
            assert_eq!(
                state_in(&registry, Some(COMMAND), true),
                WindowsStartState::OffInTaskManager
            );
            approved.set_string(VALUE_NAME, "not binary").unwrap();
            assert_eq!(registry.approved_value().unwrap(), None);

            // Another value of the key stays; a value of another type is
            // not ours.
            run.set_string("Other", "other.exe").unwrap();
            change_in(&registry, Some(COMMAND), false).unwrap();
            assert_eq!(registry.run_value().unwrap(), RunValue::Missing);
            assert_eq!(run.get_string("Other").unwrap(), "other.exe");
            run.set_expand_string(VALUE_NAME, "%SystemRoot%\\x.exe")
                .unwrap();
            assert_eq!(registry.run_value().unwrap(), RunValue::Other);
            assert_eq!(
                reconcile_in(&registry, Some(COMMAND), true).unwrap(),
                Change::Write
            );
            assert_eq!(run.get_type(VALUE_NAME).unwrap(), Type::String);

            drop(run);
            drop(approved);
            assert!(key.exists());
            let name = key.0.clone();
            drop(key);
            assert!(CURRENT_USER.open(&name).is_err(), "{name} was removed");
        }
    }
}
