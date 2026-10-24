//! Lossless backups of the keyboard's settings (spec sections 6.9 and 6.10).
//!
//! [`read_all`] sends every read request of
//! [`ReadRequest::full_read_plan`] and stores each complete reply **raw** (all
//! 63 bytes of every reply report, as hex) next to its decoded form. The raw
//! reports are the source of truth: when a backup is loaded (or deserialized
//! in any other way) the decoded views, the `device` header and
//! `active_profile` are rebuilt from them, so a backup stays readable even if
//! a later version decodes a field differently, and an edited header cannot
//! contradict the replies.
//!
//! # Scope
//!
//! A plain backup ([`read_all`], schema 1) holds the **active onboard
//! profile** only; reading it never writes anything. The W75 has four
//! onboard profiles; triggers, the main light, the base keymap and the macros
//! are per profile (captures c04 and c10). The backup also contains the
//! custom colour tables, the polling rate, the switch types and the other
//! tables whose profile scope is unverified (spec section 4.5.4, open
//! question Q3). [`read_all`] reads the active profile at the start and again
//! at the end and fails with [`BackupError::ProfileChanged`] if it changed in
//! between (a switch away and back during the backup is only noticed if the
//! keyboard reports it).
//!
//! An all-profile backup ([`read_profiles`](crate::read_profiles), schema 2)
//! also holds a complete snapshot of every other profile. Reading them
//! switches profiles (`21/12`, the only write it sends) and returns to the
//! original profile.
//!
//! # File format
//!
//! Pretty-printed JSON (`*.owbackup.json`):
//!
//! ```json
//! {
//!   "format": "openwraith-backup",
//!   "schema_version": 1,
//!   "tool": { "name": "openwraith", "version": "0.1.0" },
//!   "created_at": "2026-10-24T07:19:00Z",
//!   "scope": "active_profile_and_global_settings",
//!   "note": "...",
//!   "device": { "model": "Wraith W75", "product": "W75HEARGB", "firmware": "3.22.07", ... },
//!   "active_profile": 1,
//!   "entries": [
//!     { "raw": { "request": "device_info", "reports": ["0d000000325736...", "..."] },
//!       "decoded": { "device_info": { ... } } },
//!     ...
//!   ]
//! }
//! ```
//!
//! Schema 2 is schema 1 plus `"scope": "profiles"` and a `profiles` array
//! with one complete snapshot per other profile:
//! `"profiles": [{ "profile": 2, "entries": [...] }, ...]`. The top-level
//! `entries` stay the snapshot of the profile that was active when the
//! backup started, so everything that reads a schema-1 backup works on it.
//! Single-profile backups are still written as schema 1, byte for byte.
//!
//! When a backup is read, only `format`, `schema_version`, `tool`,
//! `created_at`, `scope`, `note`, `device.usb`, `entries[].raw` and
//! `profiles[].entries[].raw` are taken from the file. Everything else
//! (including each snapshot's `profile`) is derived from the raw reports.

use std::collections::HashSet;
use std::io::Write as _;
use std::ops::ControlFlow;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use openwraith_protocol::{
    ActiveCustomTable, ActiveProfile, AdvancedKeys, ColorTable, DeadBandTable, Decoded,
    DeviceEvent, DeviceInfo, FnLockedKeys, FromResponse, KeyTrigger, KeymapTable, LightEffect,
    LightModeList, MacroSlot, Model, PollRate, ProfileId, RawResponse, ReadRequest, SocdPairs,
    SocdSettings, Status, SwitchTypeList, SwitchTypeTable, TriggerGlobal, model_for_product,
};
use serde::{Deserialize, Serialize};

use crate::error::{BackupError, SessionError};
use crate::session::Session;
use crate::summary::identify;
use crate::transport::{Transport, UsbInfo};

/// Value of the `format` field of every backup file.
pub const BACKUP_FORMAT: &str = "openwraith-backup";
/// Schema version of single-profile backups (written by [`read_all`]).
pub const BACKUP_SCHEMA_VERSION: u32 = 1;
/// Schema version of all-profile backups (written by
/// [`read_profiles`](crate::read_profiles) when it read more than one
/// profile).
pub const BACKUP_SCHEMA_VERSION_PROFILES: u32 = 2;
/// Highest schema version this crate reads.
pub const BACKUP_SCHEMA_MAX: u32 = 2;
/// File name extension of backup files (without the leading dot).
pub const BACKUP_EXTENSION: &str = "owbackup.json";
/// Value of the `scope` field of a single-profile backup.
pub const BACKUP_SCOPE: &str = "active_profile_and_global_settings";
/// Value of the `scope` field of an all-profile backup (schema 2).
pub const BACKUP_SCOPE_PROFILES: &str = "profiles";
/// Human-readable explanation stored in every single-profile backup.
pub const BACKUP_NOTE: &str = "Read-only snapshot of the ACTIVE onboard profile. Other onboard \
profiles are not included. Tables whose profile scope is unverified (custom colour tables, \
polling rate, switch types) are included as read. The raw reports are authoritative; the decoded \
views, the device header and the active profile are regenerated from them when the file is \
loaded.";

/// The `note` of an all-profile backup whose primary snapshot is `profile`.
fn profiles_note(profile: u8) -> String {
    format!(
        "Snapshot of several onboard profiles. `entries` hold profile {profile}, the profile that \
         was active when the backup started; `profiles` hold the others. Reading them switched the \
         keyboard's active profile (the lighting changed briefly) and switched back to profile \
         {profile} at the end; nothing else was written. Every snapshot is a complete read, \
         including the tables whose profile scope is unverified. The raw reports are \
         authoritative; the decoded views, the device header and each snapshot's profile number \
         are regenerated from them when the file is loaded."
    )
}

/// Name and version of the program that made a backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Program name, e.g. `"openwraith"`.
    pub name: String,
    /// Program version.
    pub version: String,
}

impl ToolInfo {
    /// This crate's name and version.
    pub fn this_crate() -> Self {
        ToolInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Which keyboard a backup was read from.
///
/// In a loaded backup every field except `usb` is derived from the raw
/// device-info reply (`0D`); without one they are empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// Model name, e.g. `"Wraith W75"`.
    pub model: Option<String>,
    /// Product code from the device-info string, e.g. `"W75HEARGB"`.
    pub product: Option<String>,
    /// Firmware version, e.g. `"3.22.07"`.
    pub firmware: Option<String>,
    /// Firmware field as sent, e.g. `"V3.22.07"`.
    pub firmware_raw: Option<String>,
    /// Firmware build date, e.g. `"Aug  6 2026,13:46:54"`.
    pub build_date: Option<String>,
    /// The whole device-info string.
    pub info: String,
    /// USB identity, if the transport knows it.
    #[serde(default)]
    pub usb: Option<UsbInfo>,
}

impl DeviceIdentity {
    /// The identity described by a decoded device-info reply.
    pub(crate) fn new(info: &DeviceInfo, model: Option<&Model>, usb: Option<UsbInfo>) -> Self {
        DeviceIdentity {
            model: model.map(|m| m.name.to_string()),
            product: info.product.clone(),
            firmware: info.firmware.clone(),
            firmware_raw: info.firmware_raw.clone(),
            build_date: info.build_date.clone(),
            info: info.info.clone(),
            usb,
        }
    }

    /// An identity with nothing known but (possibly) the USB identity.
    fn unknown(usb: Option<UsbInfo>) -> Self {
        DeviceIdentity {
            model: None,
            product: None,
            firmware: None,
            firmware_raw: None,
            build_date: None,
            info: String::new(),
            usb,
        }
    }
}

/// One complete reply, raw and decoded.
///
/// Only `raw` is read when an entry is deserialized; the decoded view is
/// always rebuilt from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "BackupEntryRepr")]
pub struct BackupEntry {
    /// The request and every reply report (63 bytes each), losslessly.
    pub raw: RawResponse,
    /// The decoded view, rebuilt from `raw`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded: Option<Decoded>,
    /// Why `raw` could not be decoded, if it could not. The raw reports are
    /// kept either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_error: Option<String>,
}

impl BackupEntry {
    /// Wraps a reply and decodes it.
    pub fn new(raw: RawResponse) -> Self {
        let (decoded, decode_error) = match raw.decode() {
            Ok(decoded) => (Some(decoded), None),
            Err(e) => (None, Some(e.to_string())),
        };
        BackupEntry {
            raw,
            decoded,
            decode_error,
        }
    }

    /// The request this entry answers.
    pub fn request(&self) -> ReadRequest {
        self.raw.request()
    }
}

/// What deserialization reads of a [`BackupEntry`]: the raw reply only.
#[derive(Deserialize)]
struct BackupEntryRepr {
    raw: RawResponse,
}

impl From<BackupEntryRepr> for BackupEntry {
    fn from(repr: BackupEntryRepr) -> Self {
        BackupEntry::new(repr.raw)
    }
}

/// A complete snapshot of one more onboard profile in an all-profile backup
/// (schema 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileSnapshot {
    /// The profile (1-4), derived from the snapshot's raw `21/11` reply when
    /// the backup is loaded.
    pub profile: u8,
    /// Every reply of the snapshot, in the order the requests were sent (the
    /// same plan as a single-profile backup).
    pub entries: Vec<BackupEntry>,
}

/// A backup of the keyboard's settings. See the [module docs](self).
///
/// Deserializing a `Backup` (with [`from_json`](Self::from_json) or with
/// any serde deserializer) checks `format` and `schema_version`, rejects
/// files that store a request twice, validates every profile snapshot, and
/// rebuilds the decoded views, the `device` header, `active_profile` and
/// each snapshot's profile from the raw reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BackupRepr")]
pub struct Backup {
    /// Always [`BACKUP_FORMAT`].
    pub format: String,
    /// Schema version: [`BACKUP_SCHEMA_VERSION`] (1) for a single-profile
    /// backup, [`BACKUP_SCHEMA_VERSION_PROFILES`] (2) for an all-profile one.
    pub schema_version: u32,
    /// Program that made the backup.
    pub tool: ToolInfo,
    /// Creation time, RFC 3339 in UTC (e.g. `2026-10-24T07:19:00Z`).
    pub created_at: String,
    /// What the backup covers: [`BACKUP_SCOPE`] or [`BACKUP_SCOPE_PROFILES`].
    pub scope: String,
    /// Human-readable explanation of the scope.
    pub note: String,
    /// The keyboard (derived from the raw device-info reply).
    pub device: DeviceIdentity,
    /// Active onboard profile (1-4) the top-level entries belong to (derived
    /// from the raw `21/11` reply).
    pub active_profile: Option<u8>,
    /// Every reply of the primary snapshot (the profile that was active when
    /// the backup started), in the order the requests were sent.
    pub entries: Vec<BackupEntry>,
    /// Snapshots of the other onboard profiles, in ascending profile order
    /// (schema 2 only; empty in a single-profile backup).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<ProfileSnapshot>,
}

/// What deserialization reads of a [`Backup`]. Derived fields (`device`
/// except `usb`, `active_profile`, decoded views, snapshot profiles) are not
/// read.
#[derive(Deserialize)]
struct BackupRepr {
    format: String,
    schema_version: u32,
    tool: ToolInfo,
    created_at: String,
    scope: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    device: DeviceRepr,
    entries: Vec<BackupEntry>,
    #[serde(default)]
    profiles: Option<Vec<ProfileSnapshotRepr>>,
}

/// What deserialization reads of a [`ProfileSnapshot`]: the raw replies.
#[derive(Deserialize)]
struct ProfileSnapshotRepr {
    entries: Vec<BackupEntry>,
}

/// The part of the `device` header that is not derived from the replies.
#[derive(Default, Deserialize)]
struct DeviceRepr {
    #[serde(default)]
    usb: Option<UsbInfo>,
}

impl TryFrom<BackupRepr> for Backup {
    type Error = BackupError;

    fn try_from(repr: BackupRepr) -> Result<Self, BackupError> {
        check_header(Some(repr.format.as_str()), Some(repr.schema_version))?;
        check_duplicates(&repr.entries)?;
        let mut backup = Backup {
            format: repr.format,
            schema_version: repr.schema_version,
            tool: repr.tool,
            created_at: repr.created_at,
            scope: repr.scope,
            note: repr.note,
            device: DeviceIdentity::unknown(None),
            active_profile: None,
            entries: repr.entries,
            profiles: Vec::new(),
        };
        // The raw replies are authoritative: the header is derived from them.
        backup.device = match backup.device_info() {
            Some(info) => DeviceIdentity::new(
                info,
                info.product.as_deref().and_then(model_for_product),
                repr.device.usb,
            ),
            None => DeviceIdentity::unknown(repr.device.usb),
        };
        backup.active_profile = backup.active_profile_record().map(|p| p.profile);
        if let Some(snapshots) = repr.profiles {
            if backup.schema_version < BACKUP_SCHEMA_VERSION_PROFILES {
                return Err(BackupError::InvalidSnapshot {
                    profile: None,
                    reason: format!(
                        "a schema {} backup cannot contain other profiles",
                        backup.schema_version
                    ),
                });
            }
            backup.profiles = validate_snapshots(&backup, snapshots)?;
        }
        Ok(backup)
    }
}

/// Rejects a list of entries that stores a request twice.
fn check_duplicates(entries: &[BackupEntry]) -> Result<(), BackupError> {
    let mut seen = HashSet::new();
    match entries.iter().find(|e| !seen.insert(e.request())) {
        Some(entry) => Err(BackupError::DuplicateEntry {
            request: entry.request(),
        }),
        None => Ok(()),
    }
}

/// The profile of a list of entries, from its raw `21/11` reply.
fn entries_profile(entries: &[BackupEntry]) -> Option<u8> {
    entries
        .iter()
        .find(|e| e.request() == ReadRequest::ActiveProfile)
        .and_then(|e| ActiveProfile::from_response(&e.raw).ok())
        .map(|p| p.profile)
}

/// The product code of a list of entries, from its raw `0D` reply.
fn entries_product(entries: &[BackupEntry]) -> Option<String> {
    entries
        .iter()
        .find(|e| e.request() == ReadRequest::DeviceInfo)
        .and_then(|e| DeviceInfo::from_response(&e.raw).ok())
        .and_then(|info| info.product)
}

/// Validates the snapshots of a schema-2 backup and derives their profiles.
fn validate_snapshots(
    backup: &Backup,
    snapshots: Vec<ProfileSnapshotRepr>,
) -> Result<Vec<ProfileSnapshot>, BackupError> {
    let invalid =
        |profile: Option<u8>, reason: String| BackupError::InvalidSnapshot { profile, reason };
    let Some(primary) = backup
        .active_profile
        .filter(|p| ProfileId::new(*p).is_some())
    else {
        return Err(invalid(
            None,
            "an all-profile backup needs an active-profile reply (1-4) in its primary snapshot"
                .to_string(),
        ));
    };
    let Some(product) = backup.device.product.clone() else {
        return Err(invalid(
            None,
            "an all-profile backup needs a device-info reply in its primary snapshot".to_string(),
        ));
    };
    let mut seen = vec![primary];
    let mut out = Vec::with_capacity(snapshots.len());
    for snapshot in snapshots {
        check_duplicates(&snapshot.entries)?;
        let Some(profile) = entries_profile(&snapshot.entries) else {
            return Err(invalid(
                None,
                "a profile snapshot has no active-profile reply".to_string(),
            ));
        };
        if ProfileId::new(profile).is_none() {
            return Err(invalid(
                Some(profile),
                format!("a profile snapshot reports profile {profile}, outside 1-4"),
            ));
        }
        if seen.contains(&profile) {
            return Err(invalid(
                Some(profile),
                format!("profile {profile} is stored more than once"),
            ));
        }
        seen.push(profile);
        match entries_product(&snapshot.entries) {
            Some(p) if p == product => {}
            other => {
                return Err(invalid(
                    Some(profile),
                    format!(
                        "the snapshot of profile {profile} is from another keyboard ({})",
                        other.as_deref().unwrap_or("no device-info reply")
                    ),
                ));
            }
        }
        out.push(ProfileSnapshot {
            profile,
            entries: snapshot.entries,
        });
    }
    Ok(out)
}

/// Checks the `format` marker and the schema version of a backup.
fn check_header(format: Option<&str>, schema_version: Option<u32>) -> Result<(), BackupError> {
    if format != Some(BACKUP_FORMAT) {
        return Err(BackupError::NotABackup {
            found: format.map(str::to_string),
            expected: BACKUP_FORMAT,
        });
    }
    match schema_version {
        Some(found) if found > BACKUP_SCHEMA_MAX => Err(BackupError::UnsupportedSchema {
            found,
            supported: BACKUP_SCHEMA_MAX,
        }),
        _ => Ok(()),
    }
}

/// Read-only view of one profile snapshot of a [`Backup`]: the primary
/// snapshot (top-level `entries`) or one of the `profiles`.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotView<'a> {
    profile: Option<u8>,
    primary: bool,
    entries: &'a [BackupEntry],
}

/// Generates a view accessor returning the first decoded value of one variant.
macro_rules! view_first {
    ($(#[$doc:meta])* $name:ident => $variant:ident($ty:ty)) => {
        $(#[$doc])*
        pub fn $name(&self) -> Option<&'a $ty> {
            self.decoded().find_map(|d| match d {
                Decoded::$variant(value) => Some(value),
                _ => None,
            })
        }
    };
}

/// Generates a view accessor returning every decoded value of one variant.
macro_rules! view_all {
    ($(#[$doc:meta])* $name:ident => $variant:ident($ty:ty)) => {
        $(#[$doc])*
        pub fn $name(&self) -> Vec<&'a $ty> {
            self.decoded()
                .filter_map(|d| match d {
                    Decoded::$variant(value) => Some(value),
                    _ => None,
                })
                .collect()
        }
    };
}

impl<'a> SnapshotView<'a> {
    /// The profile of the snapshot (from its raw `21/11` reply), if known.
    pub fn profile(&self) -> Option<u8> {
        self.profile
    }

    /// Whether this is the primary snapshot (the top-level `entries`).
    pub fn is_primary(&self) -> bool {
        self.primary
    }

    /// Every entry of the snapshot.
    pub fn entries(&self) -> &'a [BackupEntry] {
        self.entries
    }

    /// The model, from the snapshot's device-info reply.
    pub fn model(&self) -> Option<&'static Model> {
        self.device_info()
            .and_then(|info| info.product.as_deref())
            .and_then(model_for_product)
    }

    /// The raw reply to `request`, if the snapshot has one.
    pub fn response(&self, request: ReadRequest) -> Option<&'a RawResponse> {
        self.entries
            .iter()
            .map(|e| &e.raw)
            .find(|raw| raw.request() == request)
    }

    /// Decodes the reply to `request` as `M` (from the raw reports).
    pub fn get<M: FromResponse>(
        &self,
        request: ReadRequest,
    ) -> Option<Result<M, openwraith_protocol::DecodeError>> {
        self.response(request).map(M::from_response)
    }

    /// Every decoded reply, in request order.
    pub fn decoded(&self) -> impl Iterator<Item = &'a Decoded> + 'a {
        self.entries.iter().filter_map(|e| e.decoded.as_ref())
    }

    /// Entries whose raw reports could not be decoded.
    pub fn undecoded(&self) -> impl Iterator<Item = &'a BackupEntry> + 'a {
        self.entries.iter().filter(|e| e.decoded.is_none())
    }

    view_first!(
        /// Device info (`0D`).
        device_info => DeviceInfo(DeviceInfo)
    );
    view_first!(
        /// Status and capability flags (`01`).
        status => Status(Status)
    );
    view_first!(
        /// Fn-layer locked keys (`0E`).
        fn_locked_keys => FnLockedKeys(FnLockedKeys)
    );
    view_first!(
        /// Active onboard profile (`21/11`).
        active_profile_record => ActiveProfile(ActiveProfile)
    );
    view_first!(
        /// Main-light mode list (`0A`).
        main_light_modes => MainLightModes(LightModeList)
    );
    view_first!(
        /// Side-light mode list (`02`).
        side_light_modes => SideLightModes(LightModeList)
    );
    view_first!(
        /// Main light effect (`07 01`).
        main_light => MainLight(LightEffect)
    );
    view_first!(
        /// Side light effect (`08 01`).
        side_light => SideLight(LightEffect)
    );
    view_first!(
        /// Active custom colour table (`09 20`).
        active_custom_table => ActiveCustomTable(ActiveCustomTable)
    );
    view_all!(
        /// Custom colour tables (`09 80` / `09 81`).
        custom_colors => CustomColors(ColorTable)
    );
    view_all!(
        /// Keymap layers (`18 80` / `18 82`).
        keymaps => Keymap(KeymapTable)
    );
    view_all!(
        /// Macro slots (`19 80`..`19 89`).
        macros => Macro(MacroSlot)
    );
    view_first!(
        /// Advanced keys (`22 80`).
        advanced_keys => AdvancedKeys(AdvancedKeys)
    );
    view_first!(
        /// SOCD power and hotkey (`24 02`).
        socd_settings => SocdSettings(SocdSettings)
    );
    view_first!(
        /// SOCD pairs (`24 01`).
        socd_pairs => SocdPairs(SocdPairs)
    );
    view_first!(
        /// Supported switch types (`25 00`).
        switch_type_list => SwitchTypeList(SwitchTypeList)
    );
    view_first!(
        /// Per-key switch types (`25 02`).
        switch_types => SwitchTypes(SwitchTypeTable)
    );
    view_first!(
        /// Dead band table (`26 00`).
        dead_band => DeadBand(DeadBandTable)
    );
    view_first!(
        /// Global trigger parameters (`21/04`).
        trigger_global => TriggerGlobal(TriggerGlobal)
    );
    view_all!(
        /// Per-key trigger settings (`21/05`), in request (key index) order.
        key_triggers => KeyTrigger(KeyTrigger)
    );
    view_first!(
        /// Polling rate (`21/09`).
        poll_rate => PollRate(PollRate)
    );

    /// Trigger settings of the key with this index.
    pub fn key_trigger(&self, index: u8) -> Option<&'a KeyTrigger> {
        self.key_triggers().into_iter().find(|t| t.index() == index)
    }
}

/// Generates an accessor returning the first decoded value of one variant.
macro_rules! first_decoded {
    ($(#[$doc:meta])* $name:ident => $variant:ident($ty:ty)) => {
        $(#[$doc])*
        pub fn $name(&self) -> Option<&$ty> {
            self.decoded().find_map(|d| match d {
                Decoded::$variant(value) => Some(value),
                _ => None,
            })
        }
    };
}

/// Generates an accessor returning every decoded value of one variant.
macro_rules! all_decoded {
    ($(#[$doc:meta])* $name:ident => $variant:ident($ty:ty)) => {
        $(#[$doc])*
        pub fn $name(&self) -> Vec<&$ty> {
            self.decoded()
                .filter_map(|d| match d {
                    Decoded::$variant(value) => Some(value),
                    _ => None,
                })
                .collect()
        }
    };
}

/// The typed accessors of [`Backup`] read the **primary** snapshot (the
/// top-level `entries`); use [`Backup::snapshot`] for another profile.
impl Backup {
    /// The model the backup was read from, if this version knows it.
    pub fn model(&self) -> Option<&'static Model> {
        self.device.product.as_deref().and_then(model_for_product)
    }

    /// Whether this is an all-profile backup (schema 2).
    pub fn is_all_profiles(&self) -> bool {
        self.schema_version >= BACKUP_SCHEMA_VERSION_PROFILES
    }

    /// The primary snapshot (the profile that was active when the backup
    /// started).
    pub fn primary(&self) -> SnapshotView<'_> {
        SnapshotView {
            profile: self.active_profile,
            primary: true,
            entries: &self.entries,
        }
    }

    /// The snapshot of `profile`, if the backup has one.
    pub fn snapshot(&self, profile: ProfileId) -> Option<SnapshotView<'_>> {
        self.snapshots()
            .into_iter()
            .find(|s| s.profile == Some(profile.get()))
    }

    /// Every snapshot: the primary one first, then the other profiles in
    /// stored (ascending) order.
    pub fn snapshots(&self) -> Vec<SnapshotView<'_>> {
        let mut out = vec![self.primary()];
        out.extend(self.profiles.iter().map(|s| SnapshotView {
            profile: Some(s.profile),
            primary: false,
            entries: &s.entries,
        }));
        out
    }

    /// The profiles the backup holds (1-4), ascending.
    pub fn profile_numbers(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self.snapshots().iter().filter_map(|s| s.profile).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The raw reply to `request` in the primary snapshot, if any.
    pub fn response(&self, request: ReadRequest) -> Option<&RawResponse> {
        self.entries
            .iter()
            .map(|e| &e.raw)
            .find(|raw| raw.request() == request)
    }

    /// Decodes the reply to `request` as `M` (from the raw reports).
    pub fn get<M: FromResponse>(
        &self,
        request: ReadRequest,
    ) -> Option<Result<M, openwraith_protocol::DecodeError>> {
        self.response(request).map(M::from_response)
    }

    /// Every decoded reply of the primary snapshot, in request order.
    pub fn decoded(&self) -> impl Iterator<Item = &Decoded> {
        self.entries.iter().filter_map(|e| e.decoded.as_ref())
    }

    /// Entries of the primary snapshot whose raw reports could not be
    /// decoded ([`SnapshotView::undecoded`] for the others).
    pub fn undecoded(&self) -> impl Iterator<Item = &BackupEntry> {
        self.entries.iter().filter(|e| e.decoded.is_none())
    }

    first_decoded!(
        /// Device info (`0D`).
        device_info => DeviceInfo(DeviceInfo)
    );
    first_decoded!(
        /// Status and capability flags (`01`).
        status => Status(Status)
    );
    first_decoded!(
        /// Fn-layer locked keys (`0E`).
        fn_locked_keys => FnLockedKeys(FnLockedKeys)
    );
    first_decoded!(
        /// Active onboard profile (`21/11`).
        active_profile_record => ActiveProfile(ActiveProfile)
    );
    first_decoded!(
        /// Main-light mode list (`0A`).
        main_light_modes => MainLightModes(LightModeList)
    );
    first_decoded!(
        /// Side-light mode list (`02`).
        side_light_modes => SideLightModes(LightModeList)
    );
    first_decoded!(
        /// Main light effect (`07 01`).
        main_light => MainLight(LightEffect)
    );
    first_decoded!(
        /// Side light effect (`08 01`).
        side_light => SideLight(LightEffect)
    );
    first_decoded!(
        /// Active custom colour table (`09 20`).
        active_custom_table => ActiveCustomTable(ActiveCustomTable)
    );
    all_decoded!(
        /// Custom colour tables (`09 80` / `09 81`).
        custom_colors => CustomColors(ColorTable)
    );
    all_decoded!(
        /// Keymap layers (`18 80` / `18 82`).
        keymaps => Keymap(KeymapTable)
    );
    all_decoded!(
        /// Macro slots (`19 80`..`19 89`).
        macros => Macro(MacroSlot)
    );
    first_decoded!(
        /// Advanced keys (`22 80`).
        advanced_keys => AdvancedKeys(AdvancedKeys)
    );
    first_decoded!(
        /// SOCD power and hotkey (`24 02`).
        socd_settings => SocdSettings(SocdSettings)
    );
    first_decoded!(
        /// SOCD pairs (`24 01`).
        socd_pairs => SocdPairs(SocdPairs)
    );
    first_decoded!(
        /// Supported switch types (`25 00`).
        switch_type_list => SwitchTypeList(SwitchTypeList)
    );
    first_decoded!(
        /// Per-key switch types (`25 02`).
        switch_types => SwitchTypes(SwitchTypeTable)
    );
    first_decoded!(
        /// Dead band table (`26 00`).
        dead_band => DeadBand(DeadBandTable)
    );
    first_decoded!(
        /// Global trigger parameters (`21/04`).
        trigger_global => TriggerGlobal(TriggerGlobal)
    );
    all_decoded!(
        /// Per-key trigger settings (`21/05`), in request (key index) order.
        key_triggers => KeyTrigger(KeyTrigger)
    );
    first_decoded!(
        /// Polling rate (`21/09`).
        poll_rate => PollRate(PollRate)
    );

    /// Trigger settings of the key with this index (primary snapshot).
    pub fn key_trigger(&self, index: u8) -> Option<&KeyTrigger> {
        self.key_triggers().into_iter().find(|t| t.index() == index)
    }

    /// Pretty-printed JSON, ending with a newline.
    pub fn to_json(&self) -> Result<String, BackupError> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Parses a backup and rebuilds every derived field from the raw reports.
    ///
    /// Checks the `format` marker and refuses schema versions newer than
    /// [`BACKUP_SCHEMA_MAX`] before looking at the rest of the file, so
    /// these problems are reported as [`BackupError::NotABackup`] and
    /// [`BackupError::UnsupportedSchema`]. Every raw reply is re-validated
    /// (all chunks present, every report matching its request), a file that
    /// stores a request twice is refused ([`BackupError::DuplicateEntry`]),
    /// and every profile snapshot is checked ([`BackupError::InvalidSnapshot`]:
    /// profile 1-4 from its `21/11` reply, unique, not the primary's, same
    /// product; a schema-1 file must not have snapshots).
    pub fn from_json(text: &str) -> Result<Backup, BackupError> {
        #[derive(Deserialize)]
        struct Header {
            format: Option<String>,
            schema_version: Option<u32>,
        }
        // A newer schema may not even parse structurally: check it first.
        let header: Header = serde_json::from_str(text)?;
        check_header(header.format.as_deref(), header.schema_version)?;
        let repr: BackupRepr = serde_json::from_str(text)?;
        Backup::try_from(repr)
    }

    /// Writes the backup to `path`.
    ///
    /// The JSON goes to a new temporary file with a unique name in the same
    /// directory (`.openwraith-*.partial`), is flushed to disk and is then
    /// moved to `path` in one step, so `path` never holds a half-written
    /// backup. Unless `overwrite` is true an existing file is never replaced,
    /// not even one created while this call runs: the final move itself
    /// refuses to replace a file. The temporary file is removed on every
    /// error.
    pub fn save(&self, path: &Path, overwrite: bool) -> Result<(), BackupError> {
        let already_exists = || BackupError::AlreadyExists {
            path: path.to_path_buf(),
        };
        // Fast path; the move at the end checks again.
        if !overwrite && path.exists() {
            return Err(already_exists());
        }
        let json = self.to_json()?;
        let write_error = |source| BackupError::Io {
            action: "write",
            path: path.to_path_buf(),
            source,
        };
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        // Deleted when dropped, i.e. on every early return below.
        let mut temp = tempfile::Builder::new()
            .prefix(".openwraith-")
            .suffix(".partial")
            .tempfile_in(dir)
            .map_err(write_error)?;
        temp.write_all(json.as_bytes()).map_err(write_error)?;
        temp.as_file().sync_all().map_err(write_error)?;
        let persisted = if overwrite {
            temp.persist(path)
        } else {
            temp.persist_noclobber(path)
        };
        match persisted {
            Ok(_) => Ok(()),
            // Dropping the error also deletes the temporary file.
            Err(e) if !overwrite && e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(already_exists())
            }
            Err(e) => Err(write_error(e.error)),
        }
    }

    /// Reads and parses a backup file (see [`from_json`](Self::from_json)).
    pub fn load(path: &Path) -> Result<Backup, BackupError> {
        let text = std::fs::read_to_string(path).map_err(|source| BackupError::Io {
            action: "read",
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json(&text)
    }

    /// Suggested file name: `openwraith-<product>-<YYYYMMDD-HHMMSS>.owbackup.json`
    /// (time in UTC, from [`created_at`](Self::created_at)).
    pub fn default_file_name(&self) -> String {
        format!(
            "openwraith-{}-{}.{BACKUP_EXTENSION}",
            self.file_product(),
            self.file_stamp()
        )
    }

    /// The product code for file names (`keyboard` if unknown).
    pub(crate) fn file_product(&self) -> String {
        let product: String = self
            .device
            .product
            .as_deref()
            .unwrap_or("keyboard")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        if product.is_empty() {
            "keyboard".to_string()
        } else {
            product
        }
    }

    /// `YYYYMMDD-HHMMSS` of the creation time, or `undated`.
    pub(crate) fn file_stamp(&self) -> String {
        file_stamp(&self.created_at).unwrap_or_else(|| "undated".to_string())
    }

    /// A backup of one snapshot (schema 1) or of several (schema 2).
    pub(crate) fn from_snapshots(
        primary: Snapshot,
        others: Vec<Snapshot>,
        tool: ToolInfo,
        created_at: String,
        usb: Option<UsbInfo>,
    ) -> Backup {
        let device = DeviceIdentity::new(&primary.info, Some(primary.model), usb);
        let (schema_version, scope, note) = match (others.is_empty(), primary.profile) {
            (false, Some(profile)) => (
                BACKUP_SCHEMA_VERSION_PROFILES,
                BACKUP_SCOPE_PROFILES.to_string(),
                profiles_note(profile),
            ),
            _ => (
                BACKUP_SCHEMA_VERSION,
                BACKUP_SCOPE.to_string(),
                BACKUP_NOTE.to_string(),
            ),
        };
        let mut profiles: Vec<ProfileSnapshot> = others
            .into_iter()
            .filter_map(|s| {
                s.profile.map(|profile| ProfileSnapshot {
                    profile,
                    entries: s.entries,
                })
            })
            .collect();
        profiles.sort_by_key(|s| s.profile);
        if schema_version == BACKUP_SCHEMA_VERSION {
            profiles.clear();
        }
        Backup {
            format: BACKUP_FORMAT.to_string(),
            schema_version,
            tool,
            created_at,
            scope,
            note,
            device,
            active_profile: primary.profile,
            entries: primary.entries,
            profiles,
        }
    }
}

/// Options of [`read_all_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOptions {
    /// Recorded as the program that made the backup.
    pub tool: ToolInfo,
    /// Creation time to record; `None` = now.
    pub created_at: Option<SystemTime>,
}

impl Default for BackupOptions {
    fn default() -> Self {
        BackupOptions {
            tool: ToolInfo::this_crate(),
            created_at: None,
        }
    }
}

/// Progress of [`read_all_with`], reported after every request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// Requests completed so far.
    pub done: usize,
    /// Requests in total.
    pub total: usize,
    /// The request that just completed.
    pub request: ReadRequest,
}

/// Reads every setting of the active onboard profile, plus the tables whose
/// scope is unverified (read-only). See [`read_all_with`].
pub fn read_all<T: Transport>(session: &mut Session<T>) -> Result<Backup, BackupError> {
    read_all_with(session, &BackupOptions::default(), |_| {})
}

/// Reads every setting of the active onboard profile, plus the tables whose
/// scope is unverified (see the [module docs](self)).
///
/// 1. Reads the device info and checks that the product is a supported model
///    (spec section 2.1: the model is identified by the `0x0D` string).
/// 2. Reads the status to learn which optional features exist.
/// 3. Sends every request of [`ReadRequest::full_read_plan`] and stores the
///    replies. A reply that cannot be decoded is kept raw.
/// 4. Reads the active profile again and fails if it changed.
///
/// Fails on the first request that cannot be completed (after the session's
/// retry), and immediately if the keyboard reports a reset.
pub fn read_all_with<T: Transport>(
    session: &mut Session<T>,
    options: &BackupOptions,
    mut progress: impl FnMut(Progress),
) -> Result<Backup, BackupError> {
    read_all_cancellable(session, options, |p| {
        progress(p);
        ControlFlow::Continue(())
    })
}

/// [`read_all_with`] with a progress callback that can cancel the backup:
/// returning [`ControlFlow::Break`] stops after the current request with
/// [`BackupError::Cancelled`].
pub fn read_all_cancellable<T: Transport>(
    session: &mut Session<T>,
    options: &BackupOptions,
    mut progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<Backup, BackupError> {
    let created_at = rfc3339_utc(options.created_at.unwrap_or_else(SystemTime::now));
    let snapshot = read_snapshot(session, None, &mut progress)?;
    let usb = session.transport().usb_info();
    Ok(Backup::from_snapshots(
        snapshot,
        Vec::new(),
        options.tool.clone(),
        created_at,
        usb,
    ))
}

/// One complete read of the active profile.
pub(crate) struct Snapshot {
    /// Decoded device info.
    pub(crate) info: DeviceInfo,
    /// The model.
    pub(crate) model: &'static Model,
    /// Every reply, in plan order.
    pub(crate) entries: Vec<BackupEntry>,
    /// The profile from the plan's `21/11` reply.
    pub(crate) profile: Option<u8>,
}

/// Reads the [`ReadRequest::full_read_plan`] of the active profile (see
/// [`read_all_with`]). With `expected_profile`, the plan's `21/11` reply and
/// the final check must report that profile.
pub(crate) fn read_snapshot<T: Transport>(
    session: &mut Session<T>,
    expected_profile: Option<u8>,
    progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
) -> Result<Snapshot, BackupError> {
    let events_before = session.events().len();

    let (info_raw, info, model) = identify(session)?;
    let status_raw = session.request(ReadRequest::Status)?;
    let status = Status::from_response(&status_raw).map_err(|source| SessionError::Decode {
        request: ReadRequest::Status,
        source,
    })?;

    let plan = ReadRequest::full_read_plan(model, Some(&status));
    // The plan, plus the final active-profile check.
    let total = plan.len() + 1;
    let mut prefetched = vec![info_raw, status_raw];
    let mut entries = Vec::with_capacity(plan.len());
    for (i, request) in plan.iter().copied().enumerate() {
        let raw = match prefetched.iter().position(|r| r.request() == request) {
            Some(pos) => prefetched.swap_remove(pos),
            None => session.request(request)?,
        };
        let entry = BackupEntry::new(raw);
        if let (Some(expected), Some(Decoded::ActiveProfile(p))) =
            (expected_profile, &entry.decoded)
        {
            if p.profile != expected {
                return Err(BackupError::ProfileChanged {
                    before: expected,
                    after: p.profile,
                });
            }
        }
        entries.push(entry);
        let step = Progress {
            done: i + 1,
            total,
            request,
        };
        if progress(step).is_break() {
            return Err(BackupError::Cancelled);
        }
    }

    let active_profile = entries.iter().find_map(|e| match &e.decoded {
        Some(Decoded::ActiveProfile(p)) => Some(p.profile),
        _ => None,
    });
    let check: ActiveProfile = session.read(ReadRequest::ActiveProfile)?;
    let step = Progress {
        done: total,
        total,
        request: ReadRequest::ActiveProfile,
    };
    if progress(step).is_break() {
        return Err(BackupError::Cancelled);
    }
    if let Some(before) = expected_profile.or(active_profile) {
        let reported = session.events()[events_before..]
            .iter()
            .filter_map(|event| match event {
                DeviceEvent::ProfileReport { profile, .. } => Some(*profile),
                _ => None,
            });
        if let Some(after) = std::iter::once(check.profile)
            .chain(reported)
            .find(|p| *p != before)
        {
            return Err(BackupError::ProfileChanged { before, after });
        }
    }

    Ok(Snapshot {
        info,
        model,
        entries,
        profile: active_profile,
    })
}

/// Formats `time` as RFC 3339 in UTC with second precision, e.g.
/// `2026-10-24T07:19:00Z`. Times before 1970 are clamped to the epoch.
pub fn rfc3339_utc(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX / 2);
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Gregorian date of a day count since 1970-01-01 (Howard Hinnant's
/// `civil_from_days` algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    // Both values are small and positive by construction.
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// `YYYYMMDD-HHMMSS` from an RFC 3339 timestamp.
fn file_stamp(rfc3339: &str) -> Option<String> {
    let digits: String = rfc3339
        .chars()
        .take(19)
        .filter(char::is_ascii_digit)
        .collect();
    (digits.len() == 14).then(|| format!("{}-{}", &digits[..8], &digits[8..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> String {
        rfc3339_utc(UNIX_EPOCH + Duration::from_secs(secs))
    }

    #[test]
    fn rfc3339_formatting() {
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_709_251_199), "2024-02-29T23:59:59Z");
        // OPEN time of capture c01 (1792809086131 ms).
        assert_eq!(at(1_792_809_086), "2026-10-24T02:31:26Z");
        assert_eq!(at(4_102_444_800), "2100-01-01T00:00:00Z");
        assert_eq!(
            rfc3339_utc(UNIX_EPOCH - Duration::from_secs(10)),
            "1970-01-01T00:00:00Z"
        );
    }

    #[test]
    fn file_stamps() {
        assert_eq!(
            file_stamp("2026-10-24T07:19:00Z").as_deref(),
            Some("20261024-071900")
        );
        assert_eq!(file_stamp("yesterday"), None);
    }

    #[test]
    fn header_checks() {
        assert!(check_header(Some(BACKUP_FORMAT), Some(1)).is_ok());
        assert!(check_header(Some(BACKUP_FORMAT), Some(2)).is_ok());
        assert!(check_header(Some(BACKUP_FORMAT), None).is_ok());
        assert!(matches!(
            check_header(Some("something-else"), Some(1)),
            Err(BackupError::NotABackup { found: Some(f), .. }) if f == "something-else"
        ));
        assert!(matches!(
            check_header(Some(BACKUP_FORMAT), Some(3)),
            Err(BackupError::UnsupportedSchema {
                found: 3,
                supported: 2
            })
        ));
    }
}
