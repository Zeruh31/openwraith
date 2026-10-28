//! The platform feed of the per-game automatic profiles (design B7): which
//! program is in front, and when a watched game exits. Also the running
//! programs for Settings > Games.
//!
//! * A thread polls the foreground window once a second
//!   (`active-win-pos-rs`: the window's process, opened with
//!   `PROCESS_QUERY_LIMITED_INFORMATION` for its path) and reports it to the
//!   core ([`WorkerHandle::foreground_changed`]; repeats are no-ops there).
//!   A window that cannot be queried (no foreground window, a protected
//!   process whose limited query is denied: `active-win-pos-rs` then fails
//!   as a whole, so such a program is never known, not even by its file
//!   name) counts as "nothing known" only after two failed samples in a
//!   row, so that a moment between two windows does not restart a game's
//!   wait.
//! * The same thread checks the processes the core waits for
//!   ([`WorkerHandle::auto_watched_pids`]) with `sysinfo` and reports the
//!   ones that are gone, or whose pid now belongs to another program
//!   ([`WorkerHandle::process_exited`]). The game in front is never checked
//!   (the sample shows it runs; its exit shows as another window in front,
//!   and it is checked at once after it left the front); the others every
//!   [`BACKGROUND_EVERY`] polls. It asks `sysinfo` only about those
//!   processes, with nothing to refresh but their existence, and keeps no
//!   handle between checks. `sysinfo` 0.39 opens each checked process with
//!   `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ` first (and the limited
//!   right only when that is denied): the "limited query rights only" of
//!   design B7 hold for the foreground sample, not for this check, which is
//!   why the game in front (often protected by an anti-cheat driver) is
//!   left out of it.
//! * The running-programs list ([`Programs`]) is made on demand only.
//! * Nothing here panics the thread: a panic inside a platform call counts
//!   as a failed sample. The thread stops at exit ([`Feed::stop`]).
//!
//! Nothing is logged or kept about the windows (no titles); the core
//! receives the process id, the file name and the path.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use openwraith_app_core::shell::{ForegroundProgram, RunningProgram};
use openwraith_app_core::{ProgramSource, WorkerHandle};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// How often the foreground window is checked.
pub const POLL: Duration = Duration::from_secs(1);

/// The watched processes that are not in front are checked every this many
/// polls (and one that just left the front at once).
pub const BACKGROUND_EVERY: u32 = 3;

/// Failed foreground samples in a row before the core hears "nothing
/// known".
pub const FOREGROUND_GRACE: u32 = 2;

/// How long [`Feed::stop`] waits for the thread.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// The program of a foreground window: its process id and path (the file
/// name is the rule's `exe`). An empty path is reported without a name
/// (`elevated`; nothing is known about it then). `active-win-pos-rs` 0.11
/// never gives one: it fails as a whole when the path cannot be read.
pub fn program_of(process_id: u64, path: &Path) -> Option<ForegroundProgram> {
    let pid = u32::try_from(process_id).ok().filter(|p| *p != 0)?;
    if path.as_os_str().is_empty() {
        return Some(ForegroundProgram {
            pid,
            exe: None,
            path: None,
            elevated: true,
        });
    }
    Some(ForegroundProgram {
        pid,
        exe: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        path: Some(path.to_string_lossy().into_owned()),
        elevated: false,
    })
}

/// A running program for the list: the file name from the process list,
/// the path when it could be read.
pub fn running_program(pid: u32, name: &OsStr, path: Option<&Path>) -> Option<RunningProgram> {
    let exe = name.to_string_lossy().trim().to_string();
    if pid == 0 || exe.is_empty() {
        return None;
    }
    Some(RunningProgram {
        pid,
        exe,
        path: path
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned()),
    })
}

/// Turns raw foreground samples into reports: a sample is reported as it
/// is; a failed one only after [`FOREGROUND_GRACE`] failures in a row.
#[derive(Debug, Default)]
pub struct ForegroundFilter {
    failures: u32,
}

impl ForegroundFilter {
    /// The report for `sample` (`None` = report nothing this time).
    pub fn next(&mut self, sample: Option<ForegroundProgram>) -> Option<Option<ForegroundProgram>> {
        match sample {
            Some(program) => {
                self.failures = 0;
                Some(Some(program))
            }
            None => {
                self.failures = self.failures.saturating_add(1);
                (self.failures >= FOREGROUND_GRACE).then_some(None)
            }
        }
    }
}

/// Who a process is: its file name and start time (seconds since the
/// epoch; 0 when it could not be read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The file name.
    pub name: String,
    /// The start time.
    pub start: u64,
}

impl Identity {
    fn same_process(&self, other: &Identity) -> bool {
        self.name.eq_ignore_ascii_case(&other.name)
            && (self.start == 0 || other.start == 0 || self.start == other.start)
    }
}

/// Finds the watched processes that exited (gone, or their pid reused by
/// another program).
#[derive(Debug, Default)]
pub struct ExitWatch {
    seen: HashMap<u32, Identity>,
}

impl ExitWatch {
    /// The checked pids that are gone, given what runs now; `keep` (the
    /// process in front, not checked) keeps what was seen of it.
    pub fn exited(
        &mut self,
        watched: &[u32],
        alive: &HashMap<u32, Identity>,
        keep: Option<u32>,
    ) -> Vec<u32> {
        let mut gone = Vec::new();
        for &pid in watched {
            let now = alive.get(&pid);
            let before = self.seen.get(&pid);
            match (now, before) {
                (None, _) => gone.push(pid),
                (Some(now), Some(before)) if !before.same_process(now) => gone.push(pid),
                (Some(now), None) => {
                    self.seen.insert(pid, now.clone());
                }
                (Some(_), Some(_)) => {}
            }
        }
        for pid in &gone {
            self.seen.remove(pid);
        }
        self.seen
            .retain(|pid, _| watched.contains(pid) || Some(*pid) == keep);
        gone
    }
}

/// What the feed reads from Windows (replaced by a fake in the tests).
pub trait Platform {
    /// The program in front; `None` when it cannot be known.
    fn foreground(&mut self) -> Option<ForegroundProgram>;
    /// Which of `pids` run now.
    fn alive(&mut self, pids: &[u32]) -> HashMap<u32, Identity>;
}

/// What the feed tells the core (the worker; a recorder in the tests).
pub trait Core {
    /// The program in front changed.
    fn foreground_changed(&self, program: Option<ForegroundProgram>);
    /// The processes whose exit the core waits for.
    fn watched(&self) -> Vec<u32>;
    /// A watched process is gone.
    fn exited(&self, pid: u32);
}

impl Core for WorkerHandle {
    fn foreground_changed(&self, program: Option<ForegroundProgram>) {
        WorkerHandle::foreground_changed(self, program);
    }

    fn watched(&self) -> Vec<u32> {
        self.auto_watched_pids()
    }

    fn exited(&self, pid: u32) {
        self.process_exited(pid);
    }
}

/// The feed's state between ticks.
#[derive(Debug, Default)]
pub struct FeedState {
    filter: ForegroundFilter,
    exits: ExitWatch,
    /// The process in front at the last poll.
    last_front: Option<u32>,
    /// Polls until the next check of the watched processes (0: now).
    until_check: u32,
}

/// One poll: the foreground window, then (when due) the watched processes
/// that are not in front.
pub fn tick(core: &impl Core, platform: &mut impl Platform, state: &mut FeedState) {
    let sample = catch_unwind(AssertUnwindSafe(|| platform.foreground())).unwrap_or(None);
    let front = sample.as_ref().map(|p| p.pid);
    if let Some(report) = state.filter.next(sample) {
        core.foreground_changed(report);
    }
    let watched = core.watched();
    let left_front = state
        .last_front
        .is_some_and(|pid| Some(pid) != front && watched.contains(&pid));
    state.last_front = front;
    if watched.is_empty() {
        state.exits = ExitWatch::default();
        state.until_check = 0;
        return;
    }
    // The game in front runs (the sample shows it): it is not opened.
    let check: Vec<u32> = watched
        .iter()
        .copied()
        .filter(|pid| Some(*pid) != front)
        .collect();
    if check.is_empty() {
        return;
    }
    if state.until_check > 0 && !left_front {
        state.until_check -= 1;
        return;
    }
    state.until_check = BACKGROUND_EVERY.saturating_sub(1);
    // A failed check reports nothing (better late than a false exit).
    let Ok(alive) = catch_unwind(AssertUnwindSafe(|| platform.alive(&check))) else {
        return;
    };
    for pid in state.exits.exited(&check, &alive, front) {
        core.exited(pid);
    }
}

/// The real platform: `active-win-pos-rs` and `sysinfo`.
struct Windows;

impl Platform for Windows {
    fn foreground(&mut self) -> Option<ForegroundProgram> {
        let window = active_win_pos_rs::get_active_window().ok()?;
        program_of(window.process_id, &window.process_path)
    }

    fn alive(&mut self, pids: &[u32]) -> HashMap<u32, Identity> {
        // A fresh list each time: no process handle is kept between checks.
        let mut system = System::new();
        let list: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&list),
            true,
            ProcessRefreshKind::nothing(),
        );
        pids.iter()
            .filter_map(|pid| {
                system.process(Pid::from_u32(*pid)).map(|process| {
                    (
                        *pid,
                        Identity {
                            name: process.name().to_string_lossy().into_owned(),
                            start: process.start_time(),
                        },
                    )
                })
            })
            .collect()
    }
}

/// The running programs, for Settings > Games (made on demand).
pub struct Programs;

impl ProgramSource for Programs {
    fn running_programs(&self) -> Vec<RunningProgram> {
        let listed = catch_unwind(|| {
            let mut system = System::new();
            system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
            );
            system
                .processes()
                .iter()
                .filter_map(|(pid, process)| {
                    running_program(pid.as_u32(), process.name(), process.exe())
                })
                .collect::<Vec<_>>()
        });
        listed.unwrap_or_default()
    }
}

/// The feed thread (managed by Tauri; stopped at exit).
pub struct Feed {
    stop: Mutex<Option<Sender<()>>>,
    done: Mutex<Option<Receiver<()>>>,
}

impl Feed {
    /// Stops the thread and waits for it (at most 2 s).
    pub fn stop(&self) {
        drop(
            self.stop
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        if let Some(done) = self
            .done
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = done.recv_timeout(STOP_WAIT);
        }
    }
}

/// Starts the feed thread on Windows.
pub fn start(worker: Arc<WorkerHandle>) -> Result<Feed, String> {
    spawn(worker, Windows, POLL)
}

/// Starts a feed thread that ticks every `every` until stopped.
fn spawn<C, P>(core: Arc<C>, mut platform: P, every: Duration) -> Result<Feed, String>
where
    C: Core + Send + Sync + 'static,
    P: Platform + Send + 'static,
{
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("openwraith-foreground".to_string())
        .spawn(move || {
            let mut state = FeedState::default();
            // Stops when the sender is dropped (or anything is sent).
            while let Err(RecvTimeoutError::Timeout) = stop_rx.recv_timeout(every) {
                tick(core.as_ref(), &mut platform, &mut state);
            }
            let _ = done_tx.send(());
        })
        .map_err(|e| format!("cannot start the foreground thread: {e}"))?;
    Ok(Feed {
        stop: Mutex::new(Some(stop_tx)),
        done: Mutex::new(Some(done_rx)),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    fn game(pid: u32) -> ForegroundProgram {
        ForegroundProgram {
            pid,
            exe: Some("game.exe".to_string()),
            path: Some(r"C:\Games\game.exe".to_string()),
            elevated: false,
        }
    }

    #[derive(Default)]
    struct FakeCore {
        reports: RefCell<Vec<Option<ForegroundProgram>>>,
        watched: RefCell<Vec<u32>>,
        exits: RefCell<Vec<u32>>,
    }

    impl Core for FakeCore {
        fn foreground_changed(&self, program: Option<ForegroundProgram>) {
            self.reports.borrow_mut().push(program);
        }
        fn watched(&self) -> Vec<u32> {
            self.watched.borrow().clone()
        }
        fn exited(&self, pid: u32) {
            self.exits.borrow_mut().push(pid);
            // The core stops watching an exited process.
            self.watched.borrow_mut().retain(|p| *p != pid);
        }
    }

    #[derive(Default)]
    struct FakePlatform {
        samples: VecDeque<Option<ForegroundProgram>>,
        alive: HashMap<u32, Identity>,
        alive_calls: usize,
        panic_on_alive: bool,
    }

    impl Platform for FakePlatform {
        fn foreground(&mut self) -> Option<ForegroundProgram> {
            match self.samples.pop_front() {
                Some(sample) => sample,
                None => panic!("the window could not be read"),
            }
        }
        fn alive(&mut self, pids: &[u32]) -> HashMap<u32, Identity> {
            self.alive_calls += 1;
            assert!(!self.panic_on_alive, "the process list failed");
            pids.iter()
                .filter_map(|p| self.alive.get(p).map(|i| (*p, i.clone())))
                .collect()
        }
    }

    fn identity(name: &str, start: u64) -> Identity {
        Identity {
            name: name.to_string(),
            start,
        }
    }

    #[test]
    fn a_window_that_cannot_be_read_counts_only_after_two_tries() {
        let core = FakeCore::default();
        let mut platform = FakePlatform {
            samples: VecDeque::from([Some(game(10)), None, Some(game(10)), None, None, None]),
            ..FakePlatform::default()
        };
        let mut state = FeedState::default();
        for _ in 0..6 {
            tick(&core, &mut platform, &mut state);
        }
        assert_eq!(
            *core.reports.borrow(),
            [Some(game(10)), Some(game(10)), None, None]
        );
        // A panic inside the platform call is a failed sample, not a dead
        // thread.
        tick(&core, &mut platform, &mut state);
        assert_eq!(core.reports.borrow().last(), Some(&None));
        assert_eq!(platform.alive_calls, 0, "nothing watched: no process list");
    }

    #[test]
    fn watched_games_are_reported_once_when_they_exit() {
        let core = FakeCore::default();
        core.watched.borrow_mut().extend([10, 20]);
        let mut samples = vec![Some(game(10)); 2];
        samples.extend(vec![Some(game(30)); 8]);
        let mut platform = FakePlatform {
            samples: VecDeque::from(samples),
            ..FakePlatform::default()
        };
        platform.alive.insert(10, identity("game.exe", 100));
        platform.alive.insert(20, identity("other.exe", 0));
        let mut state = FeedState::default();
        tick(&core, &mut platform, &mut state);
        tick(&core, &mut platform, &mut state);
        assert!(core.exits.borrow().is_empty());
        // 10 exits: another window comes to the front, 10 is checked at
        // once.
        platform.alive.remove(&10);
        tick(&core, &mut platform, &mut state);
        assert_eq!(*core.exits.borrow(), [10]);
        // The pid of 20 now belongs to another program (found at the next
        // background check).
        platform.alive.insert(20, identity("notepad.exe", 0));
        for _ in 0..BACKGROUND_EVERY {
            tick(&core, &mut platform, &mut state);
        }
        assert_eq!(*core.exits.borrow(), [10, 20]);
        for _ in 0..4 {
            tick(&core, &mut platform, &mut state);
        }
        assert_eq!(*core.exits.borrow(), [10, 20], "each exit once");
    }

    #[test]
    fn the_game_in_front_is_never_opened_and_the_others_seldom() {
        struct Recording(FakePlatform, Vec<Vec<u32>>);
        impl Platform for Recording {
            fn foreground(&mut self) -> Option<ForegroundProgram> {
                self.0.foreground()
            }
            fn alive(&mut self, pids: &[u32]) -> HashMap<u32, Identity> {
                self.1.push(pids.to_vec());
                self.0.alive(pids)
            }
        }
        let core = FakeCore::default();
        core.watched.borrow_mut().extend([10, 20]);
        let mut platform = Recording(
            FakePlatform {
                samples: VecDeque::from(vec![Some(game(10)); 9]),
                ..FakePlatform::default()
            },
            Vec::new(),
        );
        platform.0.alive.insert(10, identity("game.exe", 100));
        platform.0.alive.insert(20, identity("other.exe", 0));
        let mut state = FeedState::default();
        for _ in 0..9 {
            tick(&core, &mut platform, &mut state);
        }
        assert_eq!(platform.1, [vec![20], vec![20], vec![20]]);
    }

    #[test]
    fn a_failed_process_check_reports_no_exit() {
        let core = FakeCore::default();
        core.watched.borrow_mut().push(10);
        let mut platform = FakePlatform {
            samples: VecDeque::from(vec![Some(game(99)); 2]),
            panic_on_alive: true,
            ..FakePlatform::default()
        };
        let mut state = FeedState::default();
        tick(&core, &mut platform, &mut state);
        assert!(core.exits.borrow().is_empty());
        assert_eq!(platform.alive_calls, 1);
    }

    #[test]
    fn exit_watch_tells_a_reused_pid_from_the_same_process() {
        let mut watch = ExitWatch::default();
        let mut alive = HashMap::new();
        alive.insert(7, identity("Game.exe", 50));
        assert!(watch.exited(&[7], &alive, None).is_empty());
        // Same name in another case, start time unknown now: the same one.
        alive.insert(7, identity("game.EXE", 0));
        assert!(watch.exited(&[7], &alive, None).is_empty());
        // In front (not checked): what was seen of it stays.
        assert!(watch.exited(&[], &alive, Some(7)).is_empty());
        assert!(watch.seen.contains_key(&7));
        // Same name, another start time: a new process.
        alive.insert(7, identity("Game.exe", 51));
        assert_eq!(watch.exited(&[7], &alive, None), [7]);
        // Not watched any more: forgotten.
        assert!(watch.exited(&[], &alive, None).is_empty());
        assert!(watch.seen.is_empty());
    }

    #[test]
    fn the_foreground_sample_carries_the_file_name_and_path() {
        let program = program_of(42, Path::new(r"C:\Games\CS2\cs2.exe"));
        assert_eq!(
            program,
            Some(ForegroundProgram {
                pid: 42,
                exe: Some("cs2.exe".to_string()),
                path: Some(r"C:\Games\CS2\cs2.exe".to_string()),
                elevated: false,
            })
        );
        assert_eq!(
            program_of(42, Path::new("")),
            Some(ForegroundProgram {
                pid: 42,
                exe: None,
                path: None,
                elevated: true,
            })
        );
        assert_eq!(program_of(0, Path::new(r"C:\x.exe")), None);
        assert_eq!(program_of(u64::MAX, Path::new(r"C:\x.exe")), None);
    }

    #[test]
    fn the_running_list_needs_a_name() {
        assert_eq!(
            running_program(
                5,
                OsStr::new("steam.exe"),
                Some(Path::new(r"C:\Steam\steam.exe"))
            ),
            Some(RunningProgram {
                pid: 5,
                exe: "steam.exe".to_string(),
                path: Some(r"C:\Steam\steam.exe".to_string()),
            })
        );
        assert_eq!(
            running_program(5, OsStr::new("x.exe"), Some(Path::new(""))).map(|p| p.path),
            Some(None)
        );
        assert_eq!(
            running_program(0, OsStr::new("System Idle Process"), None),
            None
        );
        assert_eq!(running_program(9, OsStr::new(" "), None), None);
    }

    /// A thread-safe core that counts reports.
    #[derive(Default)]
    struct CountingCore(std::sync::atomic::AtomicUsize);

    impl Core for CountingCore {
        fn foreground_changed(&self, _program: Option<ForegroundProgram>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn watched(&self) -> Vec<u32> {
            Vec::new()
        }
        fn exited(&self, _pid: u32) {}
    }

    /// Always the same game in front.
    struct Steady;

    impl Platform for Steady {
        fn foreground(&mut self) -> Option<ForegroundProgram> {
            Some(game(1))
        }
        fn alive(&mut self, _pids: &[u32]) -> HashMap<u32, Identity> {
            HashMap::new()
        }
    }

    #[test]
    fn the_thread_polls_until_it_is_stopped() {
        let core = Arc::new(CountingCore::default());
        let feed = spawn(Arc::clone(&core), Steady, Duration::from_millis(5));
        let feed = feed.unwrap_or_else(|e| panic!("{e}"));
        let started = std::time::Instant::now();
        while core.0.load(std::sync::atomic::Ordering::SeqCst) < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the feed never ticked"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let stopping = std::time::Instant::now();
        feed.stop();
        assert!(
            stopping.elapsed() < STOP_WAIT,
            "the thread did not stop at once"
        );
        let after = core.0.load(std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(core.0.load(std::sync::atomic::Ordering::SeqCst), after);
        // Stopping again is harmless.
        feed.stop();
    }
}
