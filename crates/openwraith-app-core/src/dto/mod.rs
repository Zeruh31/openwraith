//! The IPC data types: everything the frontend sends and receives.
//!
//! Conventions: field names are snake_case; enums with data are internally
//! tagged; every `Option` is serialised (`null`); numbers are `u8`, `u16`,
//! `u32` or `i32` only; distances are trigger units of 0.02 mm, times are
//! milliseconds and colours are `"#rrggbb"`. Arguments deny unknown fields.
//! No filesystem path is ever an argument. The TypeScript types are
//! generated from these by the tests (`bindings`).

mod app;
mod auto;
mod background;
mod catalog;
mod common;
mod editors;
mod error;
mod files;
mod intent;
mod keys;
mod plan;
mod preview;
mod progress;
mod recovery;
mod report;
mod share;
mod sim;
mod snapshot;
mod status;
mod travel;

pub use app::{AppInfo, LanguageSetting, PlatformView, SafetyDirView, Settings, ThemeSetting};
pub use auto::{
    AutoActivityView, AutoCauseView, AutoEpisodeState, AutoEventView, AutoHoldView, AutoLineView,
    AutoPauseReason, AutoProfileRuleIntent, AutoProfileRuleView, AutoProfileRulesIntent,
    AutoProfileStateView, AutoWaitReason, AutoWaitView, AutomationStatusView, ProgramToken,
    ProgramView, RuleProgramIntent, UiStateReport,
};
pub use background::{
    AutoBackupSettings, AutoProfileRule, AutoProfileSettings, BackgroundSettingsApplied,
    BackgroundSettingsIntent, BackgroundSettingsView, NotificationSettings,
    ProductProfileNamesView, ProfileNamesIntent, ProfileNamesView, ShellChanges, WindowsStartState,
};
pub use catalog::{
    ActionCatalog, BindingRuleView, CatalogEntry, CatalogGroup, CatalogLayerView, CatalogModifier,
    CatalogTab, ComboRuleView, DksRowRuleView, LayoutKeyActionView, MacroPlayKindView,
    MacroRulesView, ModTapRuleView, PlayModeRuleView,
};
pub use common::{
    BackupKind, CloseChoice, CommitTriggerReasonView, DeviceId, DeviceIdentityView, DialogText,
    FileRef, FileToken, ForeignActivityView, KeyRef, LayerView, LightModeKey, LightModeView,
    LightValue, OpId, PlanToken, ResendReasonView, RevealTarget, TableRef, TriggerModeView,
    TriggerValue,
};
pub use editors::{
    ColorChangeView, ColorEditIntent, ColorEditPreview, ColorsDisabled, ColorsState,
    DeadBandChangeView, DeadBandDisabled, DeadBandEditIntent, DeadBandEditPreview,
    DeadBandHintKind, DeadBandHintView, DeadBandState, DeadBandValue, EditFieldView,
    EditProblemView, EditWarningView, EditWriteView, KeptColorView, KeyColorIntent, KeyColorView,
    KeyDeadBandView, PresetEditIntent, PresetRef, PresetRtView, PresetsView, SocdDisabled,
    SocdEditIntent, SocdEditPreview, SocdKeyView, SocdModeView, SocdPairIntent, SocdPairView,
    SocdStateView, TriggerPresetView, UserPresetIntent, UserPresetView,
};
pub use error::{AppError, AppErrorKind, ErrorDetail, IntentField, RangeField};
pub use files::{BackupFileKind, BackupFileSummary, SafetyBackupEntry, SafetyReason, ToolView};
pub use intent::{LightEditIntent, RestoreIntent, RtIntent, TriggerEditIntent};
pub use keys::{
    ActionValue, AdvancedDefView, AdvancedSlotContentView, AdvancedSlotStateView, BindingView,
    BrokenKindView, BrokenReferenceView, BuiltinAckIntent, DksRowView, DksStepView,
    EntryOriginView, EvidenceView, FnBuiltinView, KeyAssignmentsView, KeyEntryChangeView,
    KeyEntryView, KeyKindView, KeyMotionView, KeymapChangeIntent, KeymapEditIntent,
    KeymapEditPreview, KeymapProblemView, KeymapSlotChangeView, KeymapWarningView, KeymapWriteView,
    KeysDisabled, KeysLimitsView, KeysState, KnobEntryView, LibraryMacroView, LibraryNotice,
    MacroLibraryView, MacroPlayView, MacroProgramView, MacroSlotContentView, MacroSlotStateView,
    MacroStepView, MouseActionView, PersistencePlanView, PreservedEntryView, PreservedKindView,
    SlotContentView, SlotKindView, UnverifiedNoteView,
};
pub use plan::{
    AdvancedSummary, BlockCauseView, ChangeView, DeadBandPair, LightBytesView, MacroSummary,
    PlanActionView, PlanItemView, PlanTotals, PlanWarningView, ProfilePlanView, ProfileTotals,
    RestorePlanView, RestorePreview, SkipReasonView, TriggerBytesView,
};
pub use preview::{
    AllProfilesPreview, KeyEditView, LightEditPreview, ProfileSwitchPreview, TriggerEditPreview,
};
pub use progress::{CancelState, ProgressEvent, ProgressStep, ReadPurpose};
pub use recovery::{
    AutoBackupEntry, AutoBackupReason, AutoBackupsView, FirmwareChangeView, FirmwareView,
    KnownGoodTableView, RecoveryIntent, RecoveryProfileState, RecoveryProfileView,
    RecoveryStatusView, RecoveryView, ResetEvidenceView,
};
pub use report::{
    BackupSaved, ClearedMacroView, DanglingBindingView, EditReportView, FinalCheckView,
    FinalMismatchView, Headline, ItemReportView, ItemResultView, LastReport, MismatchKindView,
    MismatchView, OutcomeView, PersistenceCheckView, PersistenceView, ProfileReportView,
    ProfileSwitchReport, ReportCounts, RestoreReportView, WriteWarningView,
};
pub use share::{
    ExportIntent, ExportSource, ExportedProfile, ImportIntent, ImportPreview, ImportSummaryView,
    ShareContentsView, ShareSelectionView, SharedMacroView,
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
pub use travel::{
    DepthFrame, DepthSample, DepthStopReason, DepthStopRequest, DepthViewReport, TravelCloseReport,
    TravelCloseView, TravelStateView, TravelStatusView,
};
