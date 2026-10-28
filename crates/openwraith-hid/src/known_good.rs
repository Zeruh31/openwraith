//! Reset detection against known-good backups, and the composition of
//! per-profile backups into one all-profile backup (design part B, B6).
//!
//! A desktop application keeps the last good read of every profile (its
//! automatic backups, the "known-good store"). Comparing a fresh read with
//! it tells whether the keyboard looks reset. Everything here is **pure**:
//! nothing is read from or written to a keyboard, and nothing is ever
//! restored automatically. A restore goes through
//! [`prepare_restore`](crate::prepare_restore) and
//! [`execute_restore`](crate::execute_restore) like any other.
//!
//! # Signals
//!
//! * **R1** the keyboard's reset notification (`14 .. 01 01`, Fn + Space
//!   held 3-5 s; capture c07). The caller knows it from its session
//!   ([`Session::keyboard_reset_seen`](crate::Session::keyboard_reset_seen))
//!   and passes it to [`reset_verdict`].
//! * **R2** the firmware of the profile's `0D` reply differs from the
//!   known-good one (version or build date): a firmware update very likely
//!   resets the settings.
//! * **R3** [`assess`] compares every table of a profile with its known-good
//!   copy: [`TableVerdict::Unchanged`], [`TableVerdict::ChangedToFactory`]
//!   (the table now has its factory form and the known-good copy did not),
//!   [`TableVerdict::ChangedOther`] or [`TableVerdict::Missing`].
//!
//! A profile looks reset when R2 holds, when its triggers changed to the
//! factory form, or when at least two tables of its keymap group (base
//! keymap, Fn keymap, macros, advanced keys) did. A change of the light, of
//! the dead band or of SOCD alone never does. On firmware V3.22.07 every
//! table survives a reconnect and a USB power loss (captures c05, c06,
//! hardware session H22), so a reconnect is never a signal.
//!
//! # Factory forms
//!
//! A table is classified [`TableVerdict::ChangedToFactory`] only for a form
//! backed by evidence in this repository (captures and `docs/PROTOCOL.md`,
//! section 6.12); [`KnownGoodTable::factory_source`] names it.
//!
//! | Table | Factory form | Source |
//! |---|---|---|
//! | Triggers (`21/05`, every key but the knob) | record `00 55 55 01 01 00 00 00 00` (1.70 mm, Rapid Trigger off, 0.02 / 0.02 mm) | c07: every key after the Fn + Space reset; c11 (Reset Trigger); the `factory` preset |
//! | Main light (`07 01`) | mode, brightness, speed `02 04 03`; the colour and the other bytes are free | c07: `02 04 03 ff 00 00` after the reset, colour kept; c08 profiles 2-4 |
//! | Custom colours 1 (`09 80`) | `ff0000` at slots 46, 68, 69, 70, 103, 124, 125, 126, `000000` elsewhere | c01, before any change (spec 4.7.6) |
//! | Base and Fn keymap (`18 80` / `18 82`) | all `00` | c01-c08 (spec 4.8; c07 typed with all-zero tables) |
//! | Macros (`19 80`-`19 89`) | every slot all `FF` | c01-c09 (spec 4.9) |
//! | Advanced keys (`22 80`) | all `00` | c01 (spec 4.14); c16 profile 2 |
//! | Dead band (`26 00`) | all `00` | c01 (spec 4.12) |
//! | SOCD pairs (`24 01`) | all `00` | c16 (spec 4.15) |
//! | SOCD power (`24 02` byte 0) | `00` (off) | every `24 02` read of c01-c16 (spec 4.15) |
//!
//! Tables OpenWraith never writes (side light, custom colours 2, switch
//! types, polling rate) are not compared: they cannot be restored.
//!
//! # SOCD power
//!
//! The SOCD power is one setting of the whole keyboard; the pairs belong to
//! each profile (H16, H21). [`assess`] compares the power separately
//! ([`ProfileAssessment::socd_power`]): it is never one of a profile's
//! tables and never evidence against a profile. [`reset_verdict`] reports it
//! once, against the newest known-good backup.
//!
//! # Composing
//!
//! [`compose`] (also [`Backup::compose`]) builds a schema-2 backup from
//! per-profile backups of one keyboard (same product, same firmware,
//! distinct profiles), for "Restore all profiles". Its primary snapshot is
//! the **newest** part (by `created_at`; the lowest profile on a tie): a
//! restore writes the SOCD power of the primary snapshot, once, so the
//! power restored is the most recently read value of that keyboard-wide
//! setting. The restore warns when another part holds another power
//! ([`PlanWarning::SocdPowerDiffersInBackup`](crate::PlanWarning::SocdPowerDiffersInBackup)).

use std::cmp::Reverse;
use std::fmt;

use openwraith_protocol::{
    CustomTable, Layer, MacroSlotId, Model, ProfileId, RawResponse, ReadRequest, model_for_product,
};
use serde::Serialize;

use crate::backup::{
    BACKUP_FORMAT, BACKUP_SCHEMA_VERSION, BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE,
    BACKUP_SCOPE_EXPORT, BACKUP_SCOPE_PROFILES, Backup, ProfileSnapshot, SnapshotView, ToolInfo,
};
use crate::error::{BackupError, ErrorKind};

/// The factory trigger record of every key (payload bytes 1-9 of a `21/05`
/// reply): 1.70 mm, Rapid Trigger off, sensitivities 0.02 mm, high bytes 0.
pub const FACTORY_TRIGGER_RECORD: [u8; 9] = [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0];

/// The factory main light's mode, brightness and speed (payload bytes 0-2
/// of a `07 01` reply): Wave, 4, 3.
pub const FACTORY_LIGHT_PREFIX: [u8; 3] = [0x02, 0x04, 0x03];

/// The custom colour 1 slots lit red (`ff0000`) in the factory table (c01).
pub const FACTORY_CUSTOM1_RED_SLOTS: [usize; 8] = [46, 68, 69, 70, 103, 124, 125, 126];

/// Slots of a custom colour table (3 bytes each).
const COLOR_SLOTS: usize = 132;

/// A table [`assess`] compares, per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownGoodTable {
    /// Actuation and Rapid Trigger of every key but the knob (`21/05`).
    Triggers,
    /// The main light (`07 01`).
    MainLight,
    /// Custom colour table 1 (`09 80`).
    CustomColors1,
    /// The base keymap (`18 80`).
    BaseKeymap,
    /// The Fn keymap (`18 82`).
    FnKeymap,
    /// The ten macro slots (`19 80`-`19 89`), as one table.
    Macros,
    /// The forty advanced-key slots (`22 80`), as one table.
    AdvancedKeys,
    /// The dead band (`26 00`).
    DeadBand,
    /// The profile's SOCD pairs (`24 01`; the power is not part of a
    /// profile).
    SocdPairs,
}

impl KnownGoodTable {
    /// Every table, in the order [`ProfileAssessment::tables`] lists them.
    pub const ALL: [KnownGoodTable; 9] = [
        KnownGoodTable::Triggers,
        KnownGoodTable::MainLight,
        KnownGoodTable::CustomColors1,
        KnownGoodTable::BaseKeymap,
        KnownGoodTable::FnKeymap,
        KnownGoodTable::Macros,
        KnownGoodTable::AdvancedKeys,
        KnownGoodTable::DeadBand,
        KnownGoodTable::SocdPairs,
    ];

    /// Whether the table belongs to the keymap group (keymaps, macros,
    /// advanced keys), which the keyboard saves together.
    pub fn is_keymap_group(self) -> bool {
        matches!(
            self,
            KnownGoodTable::BaseKeymap
                | KnownGoodTable::FnKeymap
                | KnownGoodTable::Macros
                | KnownGoodTable::AdvancedKeys
        )
    }

    /// Where the table's factory form comes from (captures and
    /// `docs/PROTOCOL.md`).
    pub fn factory_source(self) -> &'static str {
        match self {
            KnownGoodTable::Triggers => {
                "every key read 00 55 55 01 01 after the Fn + Space reset (capture c07) and after \
                 Reset Trigger (c11); PROTOCOL.md 6.1 row d, 4.11; the factory preset"
            }
            KnownGoodTable::MainLight => {
                "the main light read 02 04 03 (Wave, brightness 4, speed 3) after the Fn + Space \
                 reset, colour kept (c07), and in profiles 2-4 of c08; PROTOCOL.md 6.1 row d"
            }
            KnownGoodTable::CustomColors1 => {
                "custom colour table 1 before any change: ff0000 at slots 46, 68, 69, 70, 103, \
                 124, 125 and 126 (c01); PROTOCOL.md 4.7.6"
            }
            KnownGoodTable::BaseKeymap | KnownGoodTable::FnKeymap => {
                "all 00 in both keymap tables in every capture up to c08; PROTOCOL.md 4.8"
            }
            KnownGoodTable::Macros => {
                "every macro slot all FF in every capture up to c09; PROTOCOL.md 4.9"
            }
            KnownGoodTable::AdvancedKeys => {
                "all 40 advanced-key slots all 00 (c01, c16 profile 2); PROTOCOL.md 4.14"
            }
            KnownGoodTable::DeadBand => "dead band all 00 (c01); PROTOCOL.md 4.12",
            KnownGoodTable::SocdPairs => "SOCD pairs all 00 (c16); PROTOCOL.md 4.15",
        }
    }

    /// Whether the table in `view` has its factory form; `None` when the
    /// snapshot does not hold it (or it cannot be compared).
    pub fn is_factory(self, view: SnapshotView<'_>) -> Option<bool> {
        let model = view.model()?;
        state_of(self, view, model).map(|state| is_factory_state(self, &state))
    }

    /// The table in `view` in a canonical form: two snapshots hold the same
    /// table, as [`assess`] compares them ([`TableVerdict::Unchanged`];
    /// macro slots: empty equals empty), exactly when their forms are
    /// equal. `None` when the snapshot does not hold the table (or it cannot
    /// be compared).
    ///
    /// A caller can keep a hash of it to tell later whether a table still
    /// holds what it held then (an application remembering what its own
    /// write left in a table). Pure: nothing is read or written.
    pub fn canonical(self, view: SnapshotView<'_>) -> Option<Vec<u8>> {
        let model = view.model()?;
        state_of(self, view, model).map(|state| canonical_state(self, &state))
    }

    /// The table's name in plain words.
    pub fn label(self) -> &'static str {
        match self {
            KnownGoodTable::Triggers => "triggers",
            KnownGoodTable::MainLight => "main light",
            KnownGoodTable::CustomColors1 => "key colours",
            KnownGoodTable::BaseKeymap => "base keymap",
            KnownGoodTable::FnKeymap => "Fn keymap",
            KnownGoodTable::Macros => "macros",
            KnownGoodTable::AdvancedKeys => "advanced keys",
            KnownGoodTable::DeadBand => "dead band",
            KnownGoodTable::SocdPairs => "SOCD pairs",
        }
    }
}

impl fmt::Display for KnownGoodTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// How a table compares with its known-good copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableVerdict {
    /// The same as in the known-good backup (macro slots: empty equals
    /// empty, as for a restore).
    Unchanged,
    /// Changed to the table's factory form, which the known-good copy did
    /// not have (see the [module documentation](self)).
    ChangedToFactory,
    /// Changed to something else (a change made by the user, with
    /// OpenWraith or another program).
    ChangedOther,
    /// The known-good backup or the current read does not hold the table
    /// (or holds it unusable).
    Missing,
}

/// One table of a [`ProfileAssessment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TableAssessment {
    /// The table.
    pub table: KnownGoodTable,
    /// How it compares.
    pub verdict: TableVerdict,
}

/// A firmware as the `0D` reply names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirmwareId {
    /// The version, e.g. `3.22.07`.
    pub version: Option<String>,
    /// The build date, e.g. `Aug  6 2026,13:46:54`.
    pub build_date: Option<String>,
}

impl FirmwareId {
    fn of(view: SnapshotView<'_>) -> Option<FirmwareId> {
        let info = view.device_info()?;
        Some(FirmwareId {
            version: info.firmware.clone(),
            build_date: info.build_date.clone(),
        })
    }
}

impl fmt::Display for FirmwareId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.version.as_deref().unwrap_or("unknown"))?;
        if let Some(date) = &self.build_date {
            write!(f, " ({date})")?;
        }
        Ok(())
    }
}

/// Why a profile looks reset (R2 and R3; R1 is
/// [`ResetVerdict::reset_seen`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum ResetEvidence {
    /// R2: the keyboard runs another firmware than when the known-good
    /// backup was read (version or build date).
    FirmwareChanged {
        /// The known-good backup's firmware.
        known: FirmwareId,
        /// The keyboard's firmware now.
        current: FirmwareId,
    },
    /// R3: every key's trigger record is the factory one now, and it was
    /// not in the known-good backup.
    TriggersToFactory,
    /// R3: at least two tables of the keymap group changed to their factory
    /// form.
    KeymapGroupToFactory {
        /// Those tables.
        tables: Vec<KnownGoodTable>,
    },
}

/// One profile compared with its known-good backup ([`assess`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileAssessment {
    /// The profile (1-4).
    pub profile: u8,
    /// When the known-good backup was read (its `created_at`).
    pub known_created_at: String,
    /// When the current read was made (its `created_at`).
    pub current_created_at: String,
    /// Every table of [`KnownGoodTable::ALL`], in that order.
    pub tables: Vec<TableAssessment>,
    /// The SOCD power: one setting of the **whole keyboard**, compared with
    /// this known-good backup for information only. It is never one of the
    /// profile's tables and never evidence; [`reset_verdict`] reports it
    /// once, against the newest known-good backup.
    pub socd_power: TableVerdict,
    /// R2 and R3 evidence of this profile (empty: it does not look reset).
    pub evidence: Vec<ResetEvidence>,
}

impl ProfileAssessment {
    /// Whether the profile looks reset (R2 or R3).
    pub fn reset_like(&self) -> bool {
        !self.evidence.is_empty()
    }

    /// The verdict of `table`.
    pub fn verdict(&self, table: KnownGoodTable) -> TableVerdict {
        self.tables
            .iter()
            .find(|t| t.table == table)
            .map_or(TableVerdict::Missing, |t| t.verdict)
    }

    /// The tables with `verdict`, in [`KnownGoodTable::ALL`] order.
    pub fn tables_with(&self, verdict: TableVerdict) -> Vec<KnownGoodTable> {
        self.tables
            .iter()
            .filter(|t| t.verdict == verdict)
            .map(|t| t.table)
            .collect()
    }

    /// Whether every table (and the SOCD power) is unchanged.
    pub fn is_unchanged(&self) -> bool {
        self.socd_power == TableVerdict::Unchanged
            && self
                .tables
                .iter()
                .all(|t| t.verdict == TableVerdict::Unchanged)
    }
}

/// The evidence of one profile in a [`ResetVerdict`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileEvidence {
    /// The profile (1-4).
    pub profile: u8,
    /// Its evidence (never empty).
    pub evidence: Vec<ResetEvidence>,
}

/// The overall verdict over the profiles compared ([`reset_verdict`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResetVerdict {
    /// Whether the keyboard looks reset: R1, or a profile with evidence.
    pub reset_like: bool,
    /// R1: the keyboard sent its reset notification.
    pub reset_seen: bool,
    /// The profiles with evidence of their own (R2, R3), ascending.
    pub profiles: Vec<ProfileEvidence>,
    /// The SOCD power (one setting of the whole keyboard) compared with the
    /// newest known-good backup among the assessments ([`TableVerdict::Missing`]
    /// without any). Never evidence.
    pub socd_power: TableVerdict,
}

/// Which backup a [`KnownGoodError`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The known-good backup.
    Known,
    /// The current read.
    Current,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::Known => "known-good",
            Side::Current => "current",
        })
    }
}

/// [`assess`] or [`compose`] cannot use the backups. Nothing is read or
/// written either way.
#[derive(Debug, thiserror::Error)]
pub enum KnownGoodError {
    /// A backup does not hold the profile.
    #[error("the {side} backup does not hold profile {profile}")]
    ProfileNotInBackup {
        /// Which backup.
        side: Side,
        /// The profile.
        profile: u8,
    },
    /// The backups do not come from one keyboard (another product, or
    /// another USB serial number).
    #[error("the backups are from different keyboards ({first} and {other})")]
    OtherKeyboard {
        /// The first backup's product (and serial).
        first: String,
        /// The other backup's.
        other: String,
    },
    /// The backup does not identify a keyboard this version supports.
    #[error(
        "the backup does not identify a supported keyboard (product {})",
        product.as_deref().unwrap_or("unknown")
    )]
    UnsupportedKeyboard {
        /// The product code, if any.
        product: Option<String>,
    },
    /// [`compose`] got no backup.
    #[error("there is nothing to compose: no backup was given")]
    NoParts,
    /// A part is an exported profile, which is only ever imported.
    #[error("part {index} is an exported profile; it can only be imported into a chosen profile")]
    ExportedProfile {
        /// The part's position (0-based).
        index: usize,
    },
    /// A part holds more than one profile.
    #[error("part {index} holds several profiles; compose takes one-profile backups")]
    NotSingleProfile {
        /// The part's position (0-based).
        index: usize,
    },
    /// A part has no usable active-profile reply (`21/11`, 1-4).
    #[error("part {index} does not say which profile it holds (no 21/11 reply)")]
    NoProfile {
        /// The part's position (0-based).
        index: usize,
    },
    /// The parts were made with different firmware.
    #[error(
        "the parts were made with different firmware ({} and {})",
        first.as_deref().unwrap_or("unknown"),
        other.as_deref().unwrap_or("unknown")
    )]
    MixedFirmware {
        /// The first part's firmware.
        first: Option<String>,
        /// The other part's firmware.
        other: Option<String>,
    },
    /// Two parts hold the same profile.
    #[error("profile {profile} is in more than one part")]
    DuplicateProfile {
        /// The profile.
        profile: u8,
    },
    /// The composed backup did not load back (a bug or a damaged part).
    #[error(transparent)]
    Invalid(#[from] BackupError),
}

impl KnownGoodError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            KnownGoodError::OtherKeyboard { .. } | KnownGoodError::UnsupportedKeyboard { .. } => {
                ErrorKind::ModelMismatch
            }
            KnownGoodError::MixedFirmware { .. } => ErrorKind::FirmwareMismatch,
            KnownGoodError::ProfileNotInBackup { .. }
            | KnownGoodError::NoParts
            | KnownGoodError::ExportedProfile { .. }
            | KnownGoodError::NotSingleProfile { .. }
            | KnownGoodError::NoProfile { .. }
            | KnownGoodError::DuplicateProfile { .. } => ErrorKind::InvalidBackup,
            KnownGoodError::Invalid(e) => e.kind(),
        }
    }
}

// ----- assess -----

/// Compares `profile` of the `current` read with the same profile of the
/// `known` (known-good) backup, table by table, and collects the R2 and R3
/// evidence of the profile (see the [module documentation](self)).
///
/// Both may be single-profile or all-profile backups; only their snapshots
/// of `profile` are compared, so a known-good backup is never compared with
/// another profile. Pure: nothing is read or written.
///
/// Fails when a backup does not hold the profile, or when they come from
/// different keyboards (product, or USB serial number when both record
/// one).
pub fn assess(
    known: &Backup,
    current: &Backup,
    profile: ProfileId,
) -> Result<ProfileAssessment, KnownGoodError> {
    let missing = |side| KnownGoodError::ProfileNotInBackup {
        side,
        profile: profile.get(),
    };
    let k = known
        .snapshot(profile)
        .ok_or_else(|| missing(Side::Known))?;
    let c = current
        .snapshot(profile)
        .ok_or_else(|| missing(Side::Current))?;
    let product = |view: SnapshotView<'_>| view.device_info().and_then(|i| i.product.clone());
    let (k_product, c_product) = (product(k), product(c));
    let Some(model) = c_product.as_deref().and_then(model_for_product) else {
        return Err(KnownGoodError::UnsupportedKeyboard { product: c_product });
    };
    if k_product != c_product || serial_differs(known, current) {
        return Err(KnownGoodError::OtherKeyboard {
            first: identity(k_product.as_deref(), serial_of(known)),
            other: identity(c_product.as_deref(), serial_of(current)),
        });
    }

    let tables: Vec<TableAssessment> = KnownGoodTable::ALL
        .into_iter()
        .map(|table| TableAssessment {
            table,
            verdict: verdict(
                table,
                state_of(table, k, model).as_ref(),
                state_of(table, c, model).as_ref(),
            ),
        })
        .collect();
    let socd_power = power_verdict(socd_power_of(k), socd_power_of(c));

    let mut evidence = Vec::new();
    if let (Some(known_fw), Some(current_fw)) = (FirmwareId::of(k), FirmwareId::of(c))
        && known_fw != current_fw
    {
        evidence.push(ResetEvidence::FirmwareChanged {
            known: known_fw,
            current: current_fw,
        });
    }
    let to_factory = |t: &&TableAssessment| t.verdict == TableVerdict::ChangedToFactory;
    if tables
        .iter()
        .filter(to_factory)
        .any(|t| t.table == KnownGoodTable::Triggers)
    {
        evidence.push(ResetEvidence::TriggersToFactory);
    }
    let group: Vec<KnownGoodTable> = tables
        .iter()
        .filter(to_factory)
        .map(|t| t.table)
        .filter(|t| t.is_keymap_group())
        .collect();
    if group.len() >= 2 {
        evidence.push(ResetEvidence::KeymapGroupToFactory { tables: group });
    }

    Ok(ProfileAssessment {
        profile: profile.get(),
        known_created_at: known.created_at.clone(),
        current_created_at: current.created_at.clone(),
        tables,
        socd_power,
        evidence,
    })
}

/// The overall verdict: the keyboard looks reset when it sent its reset
/// notification (`reset_seen`, R1) or a profile has evidence of its own
/// (R2, R3). The SOCD power is reported once, from the assessment with the
/// newest known-good backup, and is never evidence.
pub fn reset_verdict(assessments: &[ProfileAssessment], reset_seen: bool) -> ResetVerdict {
    let mut profiles: Vec<ProfileEvidence> = assessments
        .iter()
        .filter(|a| a.reset_like())
        .map(|a| ProfileEvidence {
            profile: a.profile,
            evidence: a.evidence.clone(),
        })
        .collect();
    profiles.sort_by_key(|p| p.profile);
    profiles.dedup_by_key(|p| p.profile);
    let socd_power = assessments
        .iter()
        .max_by_key(|a| (a.known_created_at.as_str(), Reverse(a.profile)))
        .map_or(TableVerdict::Missing, |a| a.socd_power);
    ResetVerdict {
        reset_like: reset_seen || !profiles.is_empty(),
        reset_seen,
        profiles,
        socd_power,
    }
}

/// The comparable state of a table: one part per key (triggers), per slot
/// (macros) or the whole table.
type State = Vec<Vec<u8>>;

/// The declared contents of a reply: the table bytes of a multi-chunk
/// reply, else the payload (as a restore compares them).
fn content(raw: &RawResponse) -> Vec<u8> {
    raw.table_bytes("known-good table")
        .unwrap_or_else(|_| raw.payload())
}

/// The state of `table` in `view`, `None` when a reply is missing or
/// unusable.
fn state_of(table: KnownGoodTable, view: SnapshotView<'_>, model: &Model) -> Option<State> {
    let whole = |request| view.response(request).map(|raw| vec![content(raw)]);
    match table {
        KnownGoodTable::Triggers => model
            .positions()
            .filter(|pos| Some(pos.index()) != model.knob_index)
            .map(|pos| {
                view.response(ReadRequest::KeyTrigger(pos))
                    .and_then(|raw| raw.payload().get(1..10).map(<[u8]>::to_vec))
            })
            .collect(),
        KnownGoodTable::MainLight => view
            .response(ReadRequest::MainLight)
            .map(|raw| vec![raw.payload()]),
        KnownGoodTable::CustomColors1 => whole(ReadRequest::CustomColors(CustomTable::Custom1)),
        KnownGoodTable::BaseKeymap => whole(ReadRequest::Keymap(Layer::Base)),
        KnownGoodTable::FnKeymap => whole(ReadRequest::Keymap(Layer::Fn)),
        KnownGoodTable::Macros => MacroSlotId::all()
            .map(|slot| view.response(ReadRequest::Macro(slot)).map(content))
            .collect(),
        KnownGoodTable::AdvancedKeys => whole(ReadRequest::AdvancedKeys),
        KnownGoodTable::DeadBand => whole(ReadRequest::DeadBand),
        KnownGoodTable::SocdPairs => whole(ReadRequest::SocdPairs),
    }
}

/// Whether a macro slot is empty (all `00` or all `FF`, as the vendor site
/// and a restore treat it).
fn empty_slot(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xFF)
}

/// Whether two states of `table` are the same (macro slots: empty equals
/// empty).
fn same_state(table: KnownGoodTable, a: &State, b: &State) -> bool {
    match table {
        KnownGoodTable::Macros => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(x, y)| x == y || (empty_slot(x) && empty_slot(y)))
        }
        _ => a == b,
    }
}

/// The canonical form of `state` (see [`KnownGoodTable::canonical`]): each
/// part as a tag byte, then (tag 0) its length as 8 bytes little-endian and
/// its bytes; an empty macro slot is the tag 1 alone, so that two empty
/// slots are equal whatever their filler (`00` or `FF`), as in
/// [`same_state`].
fn canonical_state(table: KnownGoodTable, state: &State) -> Vec<u8> {
    let mut out = Vec::with_capacity(state.iter().map(|p| p.len() + 9).sum());
    for part in state {
        if table == KnownGoodTable::Macros && empty_slot(part) {
            out.push(1);
            continue;
        }
        out.push(0);
        out.extend_from_slice(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// Whether `state` is the factory form of `table` (see the module docs).
fn is_factory_state(table: KnownGoodTable, state: &State) -> bool {
    let all = |byte: u8| {
        !state.is_empty()
            && state
                .iter()
                .all(|part| !part.is_empty() && part.iter().all(|b| *b == byte))
    };
    match table {
        KnownGoodTable::Triggers => {
            !state.is_empty() && state.iter().all(|r| r[..] == FACTORY_TRIGGER_RECORD)
        }
        KnownGoodTable::MainLight => state
            .first()
            .is_some_and(|light| light.get(..3) == Some(&FACTORY_LIGHT_PREFIX[..])),
        KnownGoodTable::CustomColors1 => state
            .first()
            .is_some_and(|table| table[..] == factory_custom1()[..]),
        KnownGoodTable::Macros => all(0xFF),
        KnownGoodTable::BaseKeymap
        | KnownGoodTable::FnKeymap
        | KnownGoodTable::AdvancedKeys
        | KnownGoodTable::DeadBand
        | KnownGoodTable::SocdPairs => all(0x00),
    }
}

/// The factory custom colour table 1 (c01): red at
/// [`FACTORY_CUSTOM1_RED_SLOTS`], black elsewhere.
pub fn factory_custom1() -> Vec<u8> {
    let mut table = vec![0u8; COLOR_SLOTS * 3];
    for slot in FACTORY_CUSTOM1_RED_SLOTS {
        if let Some(rgb) = table.get_mut(slot * 3..slot * 3 + 3) {
            rgb.copy_from_slice(&[0xff, 0x00, 0x00]);
        }
    }
    table
}

fn verdict(table: KnownGoodTable, known: Option<&State>, current: Option<&State>) -> TableVerdict {
    match (known, current) {
        (Some(k), Some(c)) if same_state(table, k, c) => TableVerdict::Unchanged,
        (Some(k), Some(c)) if is_factory_state(table, c) && !is_factory_state(table, k) => {
            TableVerdict::ChangedToFactory
        }
        (Some(_), Some(_)) => TableVerdict::ChangedOther,
        _ => TableVerdict::Missing,
    }
}

/// The SOCD power byte (`24 02` payload byte 0).
fn socd_power_of(view: SnapshotView<'_>) -> Option<u8> {
    view.response(ReadRequest::SocdSettings)
        .and_then(|raw| raw.payload().first().copied())
}

/// The SOCD power's verdict; its factory form is `00` (off).
fn power_verdict(known: Option<u8>, current: Option<u8>) -> TableVerdict {
    match (known, current) {
        (Some(k), Some(c)) if k == c => TableVerdict::Unchanged,
        (Some(_), Some(0)) => TableVerdict::ChangedToFactory,
        (Some(_), Some(_)) => TableVerdict::ChangedOther,
        _ => TableVerdict::Missing,
    }
}

/// The USB serial number a backup records, if any.
fn serial_of(backup: &Backup) -> Option<&str> {
    backup.device.usb.as_ref().and_then(|u| u.serial.as_deref())
}

/// Whether both backups record a USB serial number and they differ.
fn serial_differs(a: &Backup, b: &Backup) -> bool {
    matches!((serial_of(a), serial_of(b)), (Some(x), Some(y)) if x != y)
}

/// "W75HEARGB" or "W75HEARGB, serial 1234" for messages.
fn identity(product: Option<&str>, serial: Option<&str>) -> String {
    let product = product.unwrap_or("an unknown keyboard");
    match serial {
        Some(serial) => format!("{product}, serial {serial}"),
        None => product.to_string(),
    }
}

// ----- compose -----

/// Builds one backup of several profiles from one-profile backups of the
/// same keyboard (the known-good store's automatic backups), for "Restore
/// all profiles" through [`prepare_restore`](crate::prepare_restore).
///
/// Every part must be a single-profile backup (schema 1, not an exported
/// profile) with an active-profile reply of its own; all parts must name the
/// same supported product and the same firmware, record no two different
/// USB serial numbers, and hold distinct profiles.
///
/// The **newest** part (by `created_at`, the lowest profile on a tie) is
/// the primary snapshot; the others follow in ascending profile order. The
/// SOCD power is one setting of the whole keyboard and a restore writes the
/// primary snapshot's power once (H27), so the power restored is the most
/// recently read value. Two or more parts give a schema-2 backup (scope
/// [`BACKUP_SCOPE_PROFILES`]); one part gives a schema-1 backup with the
/// same entries. The result's `created_at` is the primary's; the raw
/// replies are the parts' own, unchanged. The composed backup is serialised
/// and loaded again before it is returned, so it is exactly what a saved
/// and reloaded file would be.
pub fn compose(parts: &[Backup]) -> Result<Backup, KnownGoodError> {
    if parts.is_empty() {
        return Err(KnownGoodError::NoParts);
    }
    let mut profiles: Vec<u8> = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        if part.scope == BACKUP_SCOPE_EXPORT {
            return Err(KnownGoodError::ExportedProfile { index });
        }
        if part.is_all_profiles() || !part.profiles.is_empty() {
            return Err(KnownGoodError::NotSingleProfile { index });
        }
        let Some(profile) = part.active_profile.filter(|p| ProfileId::new(*p).is_some()) else {
            return Err(KnownGoodError::NoProfile { index });
        };
        if profiles.contains(&profile) {
            return Err(KnownGoodError::DuplicateProfile { profile });
        }
        profiles.push(profile);
        if part.model().is_none() {
            return Err(KnownGoodError::UnsupportedKeyboard {
                product: part.device.product.clone(),
            });
        }
    }
    let first = &parts[0];
    for part in &parts[1..] {
        if part.device.product != first.device.product || serial_differs(first, part) {
            return Err(KnownGoodError::OtherKeyboard {
                first: identity(first.device.product.as_deref(), serial_of(first)),
                other: identity(part.device.product.as_deref(), serial_of(part)),
            });
        }
        if part.device.firmware != first.device.firmware {
            return Err(KnownGoodError::MixedFirmware {
                first: first.device.firmware.clone(),
                other: part.device.firmware.clone(),
            });
        }
    }

    let Some(primary) = parts
        .iter()
        .max_by_key(|p| (p.created_at.as_str(), Reverse(p.active_profile)))
    else {
        return Err(KnownGoodError::NoParts);
    };
    let primary_profile = primary.active_profile.unwrap_or_default();
    let mut others: Vec<ProfileSnapshot> = parts
        .iter()
        .filter(|p| p.active_profile != primary.active_profile)
        .map(|p| ProfileSnapshot {
            profile: p.active_profile.unwrap_or_default(),
            entries: p.entries.clone(),
        })
        .collect();
    others.sort_by_key(|s| s.profile);
    let usb = primary
        .device
        .usb
        .clone()
        .or_else(|| parts.iter().find_map(|p| p.device.usb.clone()));
    let mut device = primary.device.clone();
    device.usb = usb;
    let (schema_version, scope) = if others.is_empty() {
        (BACKUP_SCHEMA_VERSION, BACKUP_SCOPE)
    } else {
        (BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE_PROFILES)
    };
    let mut read_at: Vec<(u8, &str)> = parts
        .iter()
        .map(|p| (p.active_profile.unwrap_or_default(), p.created_at.as_str()))
        .collect();
    read_at.sort_unstable();
    let list = read_at
        .iter()
        .map(|(profile, at)| format!("profile {profile} read {at}"))
        .collect::<Vec<_>>()
        .join(", ");
    let note = format!(
        "Composed by OpenWraith from one-profile backups of one keyboard ({list}). `entries` \
         hold profile {primary_profile}, the most recently read part: its SOCD power (one \
         setting of the whole keyboard) is the one a restore writes. Nothing was read from the \
         keyboard to make this file. The raw reports are authoritative; the decoded views, the \
         device header and each snapshot's profile number are regenerated from them when the \
         file is loaded."
    );
    let composed = Backup {
        format: BACKUP_FORMAT.to_string(),
        schema_version,
        tool: ToolInfo::this_crate(),
        created_at: primary.created_at.clone(),
        scope: scope.to_string(),
        note,
        device,
        active_profile: primary.active_profile,
        entries: primary.entries.clone(),
        profiles: others,
    };
    Ok(Backup::from_json(&composed.to_json()?)?)
}

impl Backup {
    /// [`compose`]: one backup of several profiles from one-profile backups
    /// of the same keyboard.
    pub fn compose(parts: &[Backup]) -> Result<Backup, KnownGoodError> {
        compose(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_custom1_has_eight_red_slots() {
        let table = factory_custom1();
        assert_eq!(table.len(), 396);
        let red = table
            .chunks(3)
            .enumerate()
            .filter(|(_, rgb)| *rgb == [0xff, 0, 0])
            .map(|(slot, _)| slot)
            .collect::<Vec<_>>();
        assert_eq!(red, FACTORY_CUSTOM1_RED_SLOTS);
        assert_eq!(
            table.iter().filter(|b| **b != 0).count(),
            FACTORY_CUSTOM1_RED_SLOTS.len()
        );
    }

    #[test]
    fn verdicts_follow_the_rule() {
        let t = KnownGoodTable::DeadBand;
        let zero: State = vec![vec![0; 4]];
        let set: State = vec![vec![0, 5, 0, 0]];
        let other: State = vec![vec![0, 7, 0, 0]];
        assert_eq!(verdict(t, Some(&set), Some(&set)), TableVerdict::Unchanged);
        assert_eq!(
            verdict(t, Some(&set), Some(&zero)),
            TableVerdict::ChangedToFactory
        );
        assert_eq!(
            verdict(t, Some(&zero), Some(&set)),
            TableVerdict::ChangedOther
        );
        assert_eq!(
            verdict(t, Some(&set), Some(&other)),
            TableVerdict::ChangedOther
        );
        assert_eq!(verdict(t, None, Some(&set)), TableVerdict::Missing);
        assert_eq!(verdict(t, Some(&set), None), TableVerdict::Missing);

        // Macro slots: empty equals empty; only all-FF is the factory form.
        let m = KnownGoodTable::Macros;
        let ff: State = vec![vec![0xFF; 8]; 10];
        let zeros: State = vec![vec![0x00; 8]; 10];
        let mut content = zeros.clone();
        content[3] = vec![0, 0, 0, 0x10, 0, 1, 0, 0];
        assert_eq!(verdict(m, Some(&zeros), Some(&ff)), TableVerdict::Unchanged);
        assert_eq!(
            verdict(m, Some(&content), Some(&ff)),
            TableVerdict::ChangedToFactory
        );
        assert_eq!(
            verdict(m, Some(&content), Some(&zeros)),
            TableVerdict::ChangedOther
        );

        // The light's form leaves the colour free: a colour change within
        // the factory mode is another change.
        let l = KnownGoodTable::MainLight;
        let wave_white: State = vec![vec![2, 4, 3, 0xff, 0xff, 0xff]];
        let wave_red: State = vec![vec![2, 4, 3, 0xff, 0, 0]];
        let custom: State = vec![vec![0x0a, 4, 3, 0xff, 0, 0]];
        assert_eq!(
            verdict(l, Some(&custom), Some(&wave_red)),
            TableVerdict::ChangedToFactory
        );
        assert_eq!(
            verdict(l, Some(&wave_white), Some(&wave_red)),
            TableVerdict::ChangedOther
        );
    }

    #[test]
    fn canonical_forms_are_equal_exactly_when_the_verdict_is_unchanged() {
        let states: Vec<(KnownGoodTable, State)> = vec![
            (KnownGoodTable::DeadBand, vec![vec![0; 4]]),
            (KnownGoodTable::DeadBand, vec![vec![0, 5, 0, 0]]),
            (KnownGoodTable::DeadBand, vec![vec![0, 5, 0]]),
            (KnownGoodTable::DeadBand, vec![vec![0, 5], vec![0, 0]]),
            (KnownGoodTable::Macros, vec![vec![0xFF; 8]; 10]),
            (KnownGoodTable::Macros, vec![vec![0x00; 8]; 10]),
            (KnownGoodTable::Macros, {
                let mut s = vec![vec![0x00; 8]; 10];
                s[3] = vec![0, 0, 0, 0x10, 0, 1, 0, 0];
                s
            }),
            (KnownGoodTable::Macros, vec![vec![0x00; 8]; 9]),
        ];
        for (ta, a) in &states {
            for (tb, b) in &states {
                if ta != tb {
                    continue;
                }
                let unchanged = verdict(*ta, Some(a), Some(b)) == TableVerdict::Unchanged;
                assert_eq!(
                    canonical_state(*ta, a) == canonical_state(*tb, b),
                    unchanged,
                    "{ta}: {a:?} / {b:?}"
                );
            }
        }
    }

    #[test]
    fn the_socd_power_is_compared_on_its_own() {
        assert_eq!(power_verdict(Some(1), Some(1)), TableVerdict::Unchanged);
        assert_eq!(
            power_verdict(Some(1), Some(0)),
            TableVerdict::ChangedToFactory
        );
        assert_eq!(power_verdict(Some(0), Some(1)), TableVerdict::ChangedOther);
        assert_eq!(power_verdict(None, Some(1)), TableVerdict::Missing);
    }

    #[test]
    fn every_table_names_its_evidence() {
        for table in KnownGoodTable::ALL {
            assert!(!table.factory_source().is_empty(), "{table}");
        }
        let group: Vec<_> = KnownGoodTable::ALL
            .into_iter()
            .filter(|t| t.is_keymap_group())
            .collect();
        assert_eq!(
            group,
            [
                KnownGoodTable::BaseKeymap,
                KnownGoodTable::FnKeymap,
                KnownGoodTable::Macros,
                KnownGoodTable::AdvancedKeys
            ]
        );
    }

    #[test]
    fn errors_have_kinds() {
        assert_eq!(KnownGoodError::NoParts.kind(), ErrorKind::InvalidBackup);
        assert_eq!(
            KnownGoodError::MixedFirmware {
                first: None,
                other: None
            }
            .kind(),
            ErrorKind::FirmwareMismatch
        );
        assert_eq!(
            KnownGoodError::OtherKeyboard {
                first: "a".into(),
                other: "b".into()
            }
            .kind(),
            ErrorKind::ModelMismatch
        );
    }
}
