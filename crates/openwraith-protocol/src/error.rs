//! Error types of the protocol crate.

use crate::request::ReadRequest;
use crate::units::Distance;
use crate::write::PlacementError;

/// A byte buffer could not be turned into a [`Report`](crate::Report).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// The buffer has a length that is neither a bare 63-byte payload nor a
    /// 64-byte report with a leading report ID.
    #[error("input report has {len} bytes, expected 63 (or 64 with the report ID)")]
    BadLength {
        /// Length of the rejected buffer.
        len: usize,
    },
    /// A 64-byte buffer did not start with report ID 1.
    #[error("input report has report ID {id:#04x}, expected 0x01")]
    WrongReportId {
        /// The report ID that was found.
        id: u8,
    },
    /// A hex string could not be parsed.
    #[error("invalid hex: {0}")]
    BadHex(String),
}

/// The reports collected for a request do not form a complete response.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseError {
    /// Some chunks never arrived.
    #[error("incomplete response to {request}: missing chunk(s) {missing:?} of {expected}")]
    Incomplete {
        /// The request that was being answered.
        request: ReadRequest,
        /// Chunk indices that are missing.
        missing: Vec<u16>,
        /// Number of chunks the response should have.
        expected: u16,
    },
    /// A report does not belong to the response of this request.
    #[error("report {report} does not answer {request}")]
    Unrelated {
        /// The request that was being answered.
        request: ReadRequest,
        /// Hex dump of the header of the offending report.
        report: String,
    },
}

/// A complete response could not be decoded into its typed model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The response belongs to a different request than the decoder expects.
    #[error("cannot decode the response to {got} as {expected}")]
    WrongRequest {
        /// Name of the model the caller asked for.
        expected: &'static str,
        /// The request whose response was supplied.
        got: ReadRequest,
    },
    /// The response carries fewer bytes than the layout needs.
    #[error("{what}: need {need} byte(s), got {got}")]
    TooShort {
        /// What was being decoded.
        what: &'static str,
        /// Minimum number of bytes needed.
        need: usize,
        /// Number of bytes available.
        got: usize,
    },
    /// A chunk of a table declares a different length than the spec says.
    #[error("{what}: chunk {chunk} declares {got} byte(s), expected {expected}")]
    ChunkLength {
        /// What was being decoded.
        what: &'static str,
        /// Chunk index.
        chunk: u16,
        /// Length the spec expects for this chunk.
        expected: usize,
        /// Length declared in byte 4 of the report.
        got: usize,
    },
}

/// A write could not be built: the input is not a valid source for it, or a
/// value is outside what the keyboard accepts (see [`write`](crate::write)).
///
/// Nothing has been sent when this error is returned; it comes from the pure
/// builders of the protocol crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteBuildError {
    /// The reply belongs to a different request than the builder expects
    /// (for example a Custom 2 table where only Custom 1 can be written).
    #[error("cannot build {expected} from the reply to {got}")]
    WrongRequest {
        /// What the caller tried to build.
        expected: &'static str,
        /// The request whose reply was supplied.
        got: ReadRequest,
    },
    /// The reply's chunks do not have the lengths the spec gives them.
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// The reply carries a different number of bytes than the table has.
    #[error("{what}: expected {expected} byte(s), got {got}")]
    TableLength {
        /// What was being built.
        what: &'static str,
        /// Number of bytes the table has.
        expected: usize,
        /// Number of bytes the reply carries.
        got: usize,
    },
    /// A macro slot is neither empty (all `00` or all `FF`) nor in the
    /// canonical flat layout, so its bytes cannot be written back safely.
    #[error("macro slot {slot} is not in the canonical flat layout ({reason})")]
    NonCanonicalMacro {
        /// Slot number 0..=9.
        slot: u8,
        /// Which rule the slot breaks.
        reason: &'static str,
    },
    /// The light mode is not in the keyboard's `0x0A` list (Custom `0x0A`
    /// excepted), or it is the music-rhythm pseudo-mode `0x64`.
    #[error("light mode {mode:#04x} is not supported by this keyboard")]
    UnsupportedLightMode {
        /// The rejected mode ID.
        mode: u8,
    },
    /// A light record field is outside its range.
    #[error("light {field} {value} is out of range (maximum {max})")]
    LightValueOutOfRange {
        /// Field name (`brightness`, `speed`, `direction`, ...).
        field: &'static str,
        /// The rejected value.
        value: u8,
        /// Largest allowed value.
        max: u8,
    },
    /// A trigger write must name at least one key.
    #[error("no key selected")]
    EmptyKeySet,
    /// A key-mask bit names a position that cannot be written: the knob, a
    /// position outside the model's layout, or bits 6-7 of a mask byte.
    #[error("key position r{row}c{col} cannot be written (knob, outside the layout or not a key)")]
    KeyNotSelectable {
        /// Row of the offending bit (6 and 7 are not matrix rows).
        row: u8,
        /// Column (mask byte).
        col: u8,
    },
    /// The trigger mode byte of a read is not `00`, `0C` or `0D`.
    #[error("unknown trigger mode {mode:#04x}")]
    UnknownTriggerMode {
        /// The mode byte as read.
        mode: u8,
    },
    /// The second travel value of a read differs from the actuation point.
    /// The site always writes them equal; another combination was never seen.
    #[error("second travel {second} differs from the actuation point {actuation}")]
    SecondTravelDiffers {
        /// Actuation point.
        actuation: Distance,
        /// Second travel value.
        second: Distance,
    },
    /// Mode `0x0C` (Rapid Trigger, same sensitivity) with different press and
    /// release sensitivities.
    #[error(
        "Rapid Trigger with one sensitivity needs equal press and release values (press {press}, release {release})"
    )]
    PressReleaseDiffer {
        /// Press sensitivity.
        press: Distance,
        /// Release sensitivity.
        release: Distance,
    },
    /// A trigger value is outside the range the keyboard reports.
    #[error("{field} {value} is out of range ({min} to {max})")]
    TriggerValueOutOfRange {
        /// Field name (`actuation`, `press sensitivity`, ...).
        field: &'static str,
        /// The rejected value.
        value: Distance,
        /// Smallest allowed value.
        min: Distance,
        /// Largest allowed value.
        max: Distance,
    },
    /// The `21/04` parameters give no usable travel range (minimum or
    /// maximum travel 0, or minimum above maximum).
    #[error("the keyboard reports no usable trigger travel range")]
    TriggerLimitsUnsupported,
    /// The model is not one of [`MODELS`](crate::MODELS): write limits
    /// and selectable keys are only taken from the models this crate knows.
    #[error("unknown keyboard model {product:?}: nothing can be written to it")]
    UnknownModel {
        /// The product code of the model that was passed.
        product: &'static str,
    },
    /// A dead-band value is larger than the keyboard accepts.
    #[error("dead band {which} of key #{index} is {value} units, more than the maximum {max}")]
    DeadBandOutOfRange {
        /// Key index.
        index: u8,
        /// `top` or `bottom`.
        which: &'static str,
        /// The rejected value in trigger units.
        value: u8,
        /// Largest allowed value in trigger units.
        max: u16,
    },
    /// A value is not one the vendor configurator offers (a HID usage,
    /// multimedia index, ...), so it is never written.
    #[error("{what} {value:#04x} is not offered")]
    NotOffered {
        /// What the value is (`"HID usage"`, `"multimedia index"`, ...).
        what: &'static str,
        /// The rejected value.
        value: u8,
    },
    /// A key combination does not have the form the site builds: one or
    /// two distinct modifiers, then exactly one other key.
    #[error("invalid key combination: {reason}")]
    InvalidCombo {
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A macro has more steps than the vendor site allows.
    #[error("the macro has {steps} steps, at most {max} are allowed")]
    MacroTooLong {
        /// Number of steps.
        steps: usize,
        /// Largest number of steps.
        max: usize,
    },
    /// A macro presses a key twice, releases a key that is not pressed, or
    /// leaves a key pressed at its end.
    #[error("macro step {step}: {reason}")]
    UnbalancedMacro {
        /// Index of the offending step (0-based).
        step: usize,
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// The last step of a macro has a delay after it (the site always
    /// writes 0 there).
    #[error("the last macro step must not have a delay after it")]
    MacroLastDelay,
    /// A macro repeat count of 0.
    #[error("a macro must be played at least once")]
    InvalidMacroCount,
    /// An advanced-key slot read from the keyboard or a backup is not in a
    /// form OpenWraith writes back (unknown type, unexpected bytes).
    #[error("advanced-key slot cannot be written: {reason}")]
    UnwritableAdvancedKey {
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A DKS press point is outside 1..=`max` (0.1 mm units).
    #[error("DKS press point {value} is out of range (1 to {max}, in 0.1 mm)")]
    DksPointOutOfRange {
        /// The rejected value.
        value: u8,
        /// Largest allowed value.
        max: u8,
    },
    /// A DKS row breaks the step grammar of the vendor site.
    #[error("DKS row {row}: {reason}")]
    DksSteps {
        /// Row 0..=3.
        row: usize,
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A Mod-Tap hold threshold outside the site's range.
    #[error("Mod-Tap hold threshold {value} is out of range ({min} to {max}, in 10 ms)")]
    MtThresholdOutOfRange {
        /// The rejected value.
        value: u8,
        /// Smallest allowed value.
        min: u8,
        /// Largest allowed value.
        max: u8,
    },
    /// The keyboard's trigger unit is not 20 um (0.02 mm), the only one the
    /// DKS press points are derived for.
    #[error("trigger unit of {unit_um} um is not supported")]
    UnsupportedUnit {
        /// The unit in micrometres.
        unit_um: u32,
    },
    /// A SOCD pair table read from the keyboard or a backup is not in the
    /// form the vendor site writes.
    #[error("SOCD pairs cannot be written: {reason}")]
    UnwritableSocdPairs {
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// A key is used in two SOCD pairs.
    #[error("HID usage {usage:#04x} is used in two SOCD pairs")]
    SocdDuplicateKey {
        /// The usage.
        usage: u8,
    },
    /// Both keys of a SOCD pair are the same.
    #[error("both keys of a SOCD pair are HID usage {usage:#04x}")]
    SocdSameKey {
        /// The usage.
        usage: u8,
    },
    /// More SOCD pairs than the vendor site allows.
    #[error("{count} SOCD pairs, at most 20 are allowed")]
    TooManySocdPairs {
        /// Number of pairs.
        count: usize,
    },
    /// A colour for a position that is not a key of the model, or for the
    /// knob.
    #[error("key #{index} has no colour slot that can be written")]
    KeyNotColourable {
        /// Key index.
        index: u8,
    },
    /// A keymap entry cannot be placed on this key and layer.
    #[error(transparent)]
    Placement(#[from] PlacementError),
}
