//! The requests the command handlers send to the worker.

use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use crate::dto::{
    AllProfilesPreview, AppError, AutoBackupEntry, BackupSaved, ColorEditIntent, ColorEditPreview,
    DeadBandEditIntent, DeadBandEditPreview, DepthViewReport, DeviceId, DeviceSnapshot,
    EditReportView, ExportIntent, ExportedProfile, FileToken, ImportIntent, ImportPreview,
    KeymapEditIntent, KeymapEditPreview, LightEditIntent, LightEditPreview, PlanInvalidation,
    PlanToken, PresetEditIntent, ProfileSwitchPreview, ProfileSwitchReport, RecoveryIntent,
    RestoreIntent, RestorePreview, RestoreReportView, SocdEditIntent, SocdEditPreview,
    TravelCloseReport, TriggerEditIntent, TriggerEditPreview,
};
use crate::sink::{DepthSink, ProgressSink};

/// Where the worker sends its answer.
pub(crate) type Reply<T> = SyncSender<Result<T, AppError>>;

/// A progress sink.
pub(crate) type Progress = Arc<dyn ProgressSink>;

/// One request.
#[allow(missing_docs)]
pub(crate) enum Request {
    // ----- operations (admission-gated) -----
    Refresh {
        progress: Progress,
        reply: Reply<DeviceSnapshot>,
    },
    SelectDevice {
        id: DeviceId,
        reply: Reply<()>,
    },
    Reconnect {
        reply: Reply<()>,
    },
    Disconnect {
        reply: Reply<()>,
    },
    RecheckOtherProgram {
        reply: Reply<()>,
    },
    PrepareProfileSwitch {
        profile: u8,
        reply: Reply<ProfileSwitchPreview>,
    },
    ApplyProfileSwitch {
        token: PlanToken,
        progress: Progress,
        reply: Reply<ProfileSwitchReport>,
    },
    PrepareTriggerEdit {
        intent: TriggerEditIntent,
        reply: Reply<TriggerEditPreview>,
    },
    ApplyTriggerEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    PrepareLightEdit {
        intent: LightEditIntent,
        reply: Reply<LightEditPreview>,
    },
    ApplyLightEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    BackupActiveProfile {
        destination: FileToken,
        progress: Progress,
        reply: Reply<BackupSaved>,
    },
    PrepareAllProfileBackup {
        destination: FileToken,
        reply: Reply<AllProfilesPreview>,
    },
    ApplyAllProfileBackup {
        token: PlanToken,
        progress: Progress,
        reply: Reply<BackupSaved>,
    },
    PrepareRestore {
        intent: RestoreIntent,
        progress: Progress,
        reply: Reply<RestorePreview>,
    },
    PrepareKeymapEdit {
        intent: KeymapEditIntent,
        reply: Reply<KeymapEditPreview>,
    },
    ApplyKeymapEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    ApplyRestore {
        token: PlanToken,
        progress: Progress,
        reply: Reply<RestoreReportView>,
    },
    PrepareColorEdit {
        intent: ColorEditIntent,
        reply: Reply<ColorEditPreview>,
    },
    ApplyColorEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    PrepareDeadBandEdit {
        intent: DeadBandEditIntent,
        reply: Reply<DeadBandEditPreview>,
    },
    ApplyDeadBandEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    PrepareSocdEdit {
        intent: SocdEditIntent,
        reply: Reply<SocdEditPreview>,
    },
    ApplySocdEdit {
        token: PlanToken,
        progress: Progress,
        reply: Reply<EditReportView>,
    },
    PreparePresetEdit {
        intent: PresetEditIntent,
        reply: Reply<TriggerEditPreview>,
    },
    StartDepthView {
        sink: Arc<dyn DepthSink>,
        reply: Reply<DepthViewReport>,
    },
    CloseStrayTravelTest {
        reply: Reply<TravelCloseReport>,
    },
    ExportProfile {
        intent: ExportIntent,
        progress: Progress,
        reply: Reply<ExportedProfile>,
    },
    PrepareImport {
        intent: ImportIntent,
        progress: Progress,
        reply: Reply<ImportPreview>,
    },
    ApplyImport {
        token: PlanToken,
        progress: Progress,
        reply: Reply<RestoreReportView>,
    },
    PrepareRecovery {
        intent: RecoveryIntent,
        progress: Progress,
        reply: Reply<RestorePreview>,
    },
    /// "Back up now" (the tray): an automatic backup of the active profile.
    BackUpNow {
        reply: Reply<Option<AutoBackupEntry>>,
    },
    // ----- queued, no admission -----
    DiscardPlan {
        token: PlanToken,
    },
    InvalidatePlan {
        reason: PlanInvalidation,
    },
    WindowFocused,
    Shutdown {
        reply: SyncSender<()>,
    },
    /// Panics inside the worker (tests of the fault handling).
    #[cfg(test)]
    Panic,
}

/// A request and whether it passed admission (the worker clears the busy
/// flag after an admitted request).
pub(crate) struct Envelope {
    pub(crate) request: Request,
    pub(crate) admitted: bool,
}
