//! The IPC data types: everything the frontend sends and receives.
//!
//! Conventions: field names are snake_case; enums with data are internally
//! tagged; every `Option` is serialised (`null`); numbers are `u8`, `u16`,
//! `u32` or `i32` only; distances are trigger units of 0.02 mm, times are
//! milliseconds and colours are `"#rrggbb"`. Arguments deny unknown fields.
//! No filesystem path is ever an argument. The TypeScript types are
//! generated from these by the tests (`bindings`).

mod app;
mod common;
mod error;
mod files;
mod intent;
mod plan;
mod preview;
mod progress;
mod report;
mod sim;
mod snapshot;
mod status;

pub use app::{AppInfo, LanguageSetting, PlatformView, SafetyDirView, Settings, ThemeSetting};
pub use common::{
    BackupKind, CloseChoice, CommitTriggerReasonView, DeviceId, DeviceIdentityView, DialogText,
    FileRef, FileToken, ForeignActivityView, KeyRef, LayerView, LightModeKey, LightModeView,
    LightValue, OpId, PlanToken, ResendReasonView, RevealTarget, TableRef, TriggerModeView,
    TriggerValue,
};
pub use error::{AppError, AppErrorKind, ErrorDetail, IntentField, RangeField};
pub use files::{BackupFileKind, BackupFileSummary, SafetyBackupEntry, SafetyReason, ToolView};
pub use intent::{LightEditIntent, RestoreIntent, RtIntent, TriggerEditIntent};
pub use plan::{
    AdvancedSummary, BlockCauseView, ChangeView, DeadBandPair, LightBytesView, MacroSummary,
    PlanActionView, PlanItemView, PlanTotals, PlanWarningView, ProfilePlanView, ProfileTotals,
    RestorePlanView, RestorePreview, SkipReasonView, TriggerBytesView,
};
pub use preview::{
    AllProfilesPreview, KeyEditView, LightEditPreview, ProfileSwitchPreview, TriggerEditPreview,
};
pub use progress::{CancelState, ProgressEvent, ProgressStep, ReadPurpose};
pub use report::{
    BackupSaved, ClearedMacroView, DanglingBindingView, EditReportView, FinalCheckView,
    FinalMismatchView, Headline, ItemReportView, ItemResultView, LastReport, MismatchKindView,
    MismatchView, OutcomeView, PersistenceCheckView, PersistenceView, ProfileReportView,
    ProfileSwitchReport, ReportCounts, RestoreReportView, WriteWarningView,
};
pub use sim::{SimAction, SimInfo};
pub use snapshot::{
    DeviceSnapshot, KeyTriggerView, LightDisabled, LightState, NotEditable, TriggerDisabled,
    TriggerLimitsView, TriggerState,
};
pub use status::{
    AppEvent, ConnectionState, DeviceEntryView, DeviceStatus, InterruptedOp, Notice, OpInfo,
    OpKind, PlanInvalidation, WriteBlockReason, WriteGate,
};
