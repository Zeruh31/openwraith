//! Console input and output of the commands that talk to the keyboard
//! through the guarded write path, behind the [`Ui`] trait so that tests can
//! run the same command logic against the device simulator.
//!
//! [`Terminal`] is the real console: normal output goes to stdout (stripped
//! of control characters), notes, progress and prompts go to stderr, and
//! answers are read from stdin. Ctrl+C sets a cancel flag that the write
//! commands check at every table boundary (inside a keymap group, once a
//! group table was sent, the group's Fn keymap (`18 02`) that makes the
//! keyboard save it is still written first); a second Ctrl+C exits at
//! once. During the live key-depth view ([`Ui::enable_cancel_close_first`])
//! every Ctrl+C only asks to stop, so that the test is always closed; only a
//! third one exits at once.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::{emit, printable};

/// What the write-capable commands need from the console.
pub trait Ui {
    /// Normal output (stdout): plans, reports, summaries.
    fn print(&mut self, text: &str) -> anyhow::Result<()>;

    /// A message for the user (stderr), one line or several; a trailing
    /// newline is added.
    fn note(&mut self, text: &str);

    /// A transient progress line (stderr; shown only on a terminal).
    fn progress(&mut self, text: &str);

    /// Removes the progress line, if one is shown.
    fn clear_progress(&mut self);

    /// Whether the user can be asked a question (stdin is a terminal).
    fn can_prompt(&self) -> bool;

    /// Asks a yes/no question. The default answer is No: anything but `y` or
    /// `yes` (and end of input) is No.
    fn confirm(&mut self, question: &str) -> anyhow::Result<bool>;

    /// Starts honouring Ctrl+C as "cancel at the next table boundary" (in a
    /// keymap group: after the group's Fn keymap, `18 02`, which makes the
    /// keyboard save it). Called by the commands that write.
    fn enable_cancel(&mut self);

    /// Starts honouring Ctrl+C for the live key-depth view: every Ctrl+C
    /// only asks to stop (the test is closed before the command ends); the
    /// third one exits at once, naming `openwraith travel --close`.
    fn enable_cancel_close_first(&mut self);

    /// Whether the user asked to cancel (Ctrl+C).
    fn cancelled(&self) -> bool;
}

/// Set by the Ctrl+C handler.
static CANCEL: AtomicBool = AtomicBool::new(false);

/// The Ctrl+C handler only asks to stop (the live key-depth view).
static CLOSE_FIRST: AtomicBool = AtomicBool::new(false);

/// Ctrl+C presses so far.
static PRESSES: AtomicU32 = AtomicU32::new(0);

/// Width of the progress line.
const PROGRESS_WIDTH: usize = 72;

/// The real console.
#[derive(Debug, Default)]
pub struct Terminal {
    progress_shown: bool,
    cancel_handler: bool,
}

impl Terminal {
    /// A console for one command.
    pub fn new() -> Self {
        Terminal::default()
    }

    /// Installs the Ctrl+C handler once. It reads [`CLOSE_FIRST`] at every
    /// press.
    fn install_handler(&mut self) {
        if self.cancel_handler {
            return;
        }
        self.cancel_handler = true;
        let installed = ctrlc::set_handler(|| {
            let presses = PRESSES.fetch_add(1, Ordering::SeqCst) + 1;
            CANCEL.store(true, Ordering::SeqCst);
            if CLOSE_FIRST.load(Ordering::SeqCst) {
                match presses {
                    1 => eprintln!("\nCtrl+C: closing the key-depth test..."),
                    2 => eprintln!(
                        "\nStill closing the key-depth test (it waits up to 1 s for the \
                         keyboard). Press Ctrl+C once more to exit at once."
                    ),
                    _ => {
                        eprintln!(
                            "\nwarning: exiting without closing the key-depth test: the keyboard \
                             may still send key depths. Run `openwraith travel --close`."
                        );
                        std::process::exit(130);
                    }
                }
                return;
            }
            if presses > 1 {
                eprintln!(
                    "\nwarning: second Ctrl+C: stopping immediately. A table or a profile switch \
                     may have been interrupted; check the keyboard with `openwraith profile` and \
                     `openwraith show`, and restore the safety backup if needed."
                );
                std::process::exit(130);
            }
            eprintln!(
                "\nCtrl+C: stopping after the current table (inside the keymap group: after \
                 the Fn keymap (18 02) that makes the keyboard save it and any macro or \
                 advanced-key slot that an already written key refers to) and returning to the \
                 original profile (press Ctrl+C again to stop immediately)."
            );
        });
        if let Err(e) = installed {
            eprintln!("warning: Ctrl+C cannot be handled ({e}); it will stop OpenWraith at once");
        }
    }
}

impl Ui for Terminal {
    fn print(&mut self, text: &str) -> anyhow::Result<()> {
        self.clear_progress();
        emit(text)
    }

    fn note(&mut self, text: &str) {
        self.clear_progress();
        eprintln!("{}", printable(text.trim_end_matches('\n')));
    }

    fn progress(&mut self, text: &str) {
        if !std::io::stderr().is_terminal() {
            return;
        }
        let line: String = printable(text)
            .chars()
            .filter(|c| *c != '\n')
            .take(PROGRESS_WIDTH)
            .collect();
        eprint!("\r{line:<PROGRESS_WIDTH$}");
        self.progress_shown = true;
    }

    fn clear_progress(&mut self) {
        if self.progress_shown {
            eprint!("\r{:PROGRESS_WIDTH$}\r", "");
            self.progress_shown = false;
        }
    }

    fn can_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn confirm(&mut self, question: &str) -> anyhow::Result<bool> {
        self.clear_progress();
        eprint!("{} [y/N] ", printable(question));
        std::io::stderr().flush().ok();
        let mut answer = String::new();
        let read = std::io::stdin().lock().read_line(&mut answer);
        // Ctrl+C while the question is open (the handler keeps the process
        // alive) counts as No, whatever was typed.
        if self.cancelled() {
            eprintln!();
            return Ok(false);
        }
        match read {
            // End of input: No.
            Ok(0) => {
                eprintln!();
                Ok(false)
            }
            Ok(_) => Ok(matches!(
                answer.trim().to_ascii_lowercase().as_str(),
                "y" | "yes"
            )),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn enable_cancel(&mut self) {
        self.install_handler();
    }

    fn enable_cancel_close_first(&mut self) {
        CLOSE_FIRST.store(true, Ordering::SeqCst);
        self.install_handler();
    }

    fn cancelled(&self) -> bool {
        CANCEL.load(Ordering::SeqCst)
    }
}
