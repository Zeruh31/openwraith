//! Makes a backup from a USB capture file instead of a keyboard.
//!
//! No HID device is opened: the capture is replayed by `MockTransport`.
//! Useful to try the backup format and the CLI's `show --from` without
//! hardware.
//!
//! ```text
//! cargo run -p openwraith-hid --features mock --example mock_backup -- \
//!     captures/capture_06_powercycle_W_persisted.json sample.owbackup.json
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use openwraith_hid::{BackupOptions, MockTransport, Session, ToolInfo, read_all_with};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [capture, output] = args.as_slice() else {
        eprintln!("usage: mock_backup <capture.json> <output.owbackup.json>");
        return ExitCode::from(2);
    };
    match run(PathBuf::from(capture), PathBuf::from(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(capture: PathBuf, output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mock = MockTransport::from_capture_file(&capture)?;
    let mut session = Session::new(mock);
    let options = BackupOptions {
        tool: ToolInfo {
            name: "openwraith-hid/examples/mock_backup".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        created_at: None,
    };
    let backup = read_all_with(&mut session, &options, |_| {})?;
    backup.save(&output, true)?;
    println!(
        "wrote {} ({} replies replayed from {})",
        output.display(),
        backup.entries.len(),
        capture.display()
    );
    Ok(())
}
