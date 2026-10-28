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
    /// keymap group: after the writes the group still needs, see
    /// [`FIRST_CTRL_C`]). Called by the commands that write.
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

/// What the first Ctrl+C of a command that writes does (see
/// `openwraith_hid::group`): the writes the keymap group still needs go out
/// first, which after macro slot 0 can be up to nine more macro slots.
pub(crate) const FIRST_CTRL_C: &str = "Ctrl+C: stopping after the current table and returning \
     to the original profile. Inside the keymap group the writes it still needs go out first: \
     the Fn keymap (18 02) after a base keymap already written (the keyboard keeps the base \
     keymap only then), a macro or advanced-key slot that a written key refers to, and, once \
     macro slot 0 was written, every other macro slot that keeps content (writing slot 0 makes \
     the keyboard clear slots 1-9, so they are written again). Press Ctrl+C again to stop \
     immediately.";

/// The warning of the second Ctrl+C, before the process exits.
pub(crate) const SECOND_CTRL_C: &str = "warning: second Ctrl+C: stopping immediately. A table \
     or a profile switch may have been interrupted, and if macro slot 0 was written, the macros \
     not yet written again after it stay empty; check the keyboard with `openwraith profile` and \
     `openwraith show`, and restore the safety backup if needed (the undo command of the plan).";

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
                eprintln!("\n{SECOND_CTRL_C}");
                std::process::exit(130);
            }
            eprintln!("\n{FIRST_CTRL_C}");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The Ctrl+C messages name every write a cancellation still sends,
    /// the macro slots after slot 0 included, and what a second Ctrl+C can
    /// leave empty.
    #[test]
    fn the_ctrl_c_messages_name_the_macros_after_slot_0() {
        let first = FIRST_CTRL_C
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for part in [
            "the Fn keymap (18 02) after a base keymap already written",
            "once macro slot 0 was written, every other macro slot that keeps content",
            "Press Ctrl+C again to stop immediately.",
        ] {
            assert!(first.contains(part), "{part}: {first}");
        }
        let second = SECOND_CTRL_C
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            second.contains(
                "if macro slot 0 was written, the macros not yet written again after it stay empty"
            ),
            "{second}"
        );
    }
}
