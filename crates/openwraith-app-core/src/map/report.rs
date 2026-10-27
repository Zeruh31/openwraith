//! Reports of operations as views.

use openwraith_hid::{
    EditReport, ItemReport, ItemResult, Outcome, Persistence, RestoreReport, SelectOutcome,
    WriteWarning,
};
use openwraith_protocol::{Mismatch, MismatchKind};

use super::{commit_reason, foreign, key_ref, sat8, sat16, sat32, table_ref};
use crate::dto::{
    AppError, EditReportView, FileRef, FinalCheckView, FinalMismatchView, Headline, ItemReportView,
    ItemResultView, MismatchKindView, MismatchView, OutcomeView, PersistenceCheckView,
    PersistenceView, ProfileReportView, ProfileSwitchReport, ReportCounts, RestoreReportView,
    WriteWarningView,
};

/// At most this many differing offsets are listed.
const MAX_OFFSETS: usize = 64;

/// A write warning.
pub(crate) fn write_warning(warning: &WriteWarning) -> WriteWarningView {
    match *warning {
        WriteWarning::AckTimeout { packet, of, send } => WriteWarningView::AckTimeout {
            packet: sat16(packet),
            of: sat16(of),
            send: sat8(send),
        },
        WriteWarning::OptionalAckMissing { packet } => WriteWarningView::OptionalAckMissing {
            packet: sat16(packet),
        },
        WriteWarning::VerifyReread => WriteWarningView::VerifyReread,
        WriteWarning::Retried => WriteWarningView::Retried,
        WriteWarning::SelectAckMissing { profile } => {
            WriteWarningView::SelectAckMissing { profile }
        }
        WriteWarning::ProfilePushMissing { profile } => {
            WriteWarningView::ProfilePushMissing { profile }
        }
        WriteWarning::SelectRetried { profile } => WriteWarningView::SelectRetried { profile },
        WriteWarning::WaitedForLateAck => WriteWarningView::WaitedForLateAck,
        WriteWarning::WaitedForCommit { waited_ms } => WriteWarningView::WaitedForCommit {
            waited_ms: sat32(waited_ms),
        },
        WriteWarning::BaseKeymapNotSaved => WriteWarningView::BaseKeymapNotSaved,
        WriteWarning::UnexpectedAck { packet } => WriteWarningView::UnexpectedAck {
            packet: sat16(packet),
        },
        WriteWarning::SocdToggledOnKeyboard { power } => {
            WriteWarningView::SocdToggledOnKeyboard { power }
        }
    }
}

fn warnings(list: &[WriteWarning]) -> Vec<WriteWarningView> {
    list.iter().map(write_warning).collect()
}

/// A read-back difference.
pub(crate) fn mismatch(m: &Mismatch) -> MismatchView {
    MismatchView {
        table: table_ref(m.table),
        read: m.read.to_string(),
        kind: match m.kind {
            MismatchKind::MissingReply => MismatchKindView::MissingReply,
            MismatchKind::InvalidReply => MismatchKindView::InvalidReply,
            MismatchKind::Differs => MismatchKindView::Differs,
        },
        offsets: m
            .differing_offsets()
            .into_iter()
            .take(MAX_OFFSETS)
            .map(sat16)
            .collect(),
        expected_hex: m.expected.to_hex(),
        got_hex: m.got.to_hex(),
    }
}

fn item_result(result: &ItemResult) -> ItemResultView {
    match result {
        ItemResult::Written {
            sends,
            packets,
            warnings: w,
            persistence,
        } => ItemResultView::Written {
            sends: sat8(*sends),
            packets: sat16(*packets),
            warnings: warnings(w),
            persistence: match persistence {
                Persistence::NotChecked => PersistenceView::NotChecked,
                Persistence::Persisted => PersistenceView::Persisted,
            },
        },
        ItemResult::Lost {
            sends,
            packets,
            warnings: w,
            mismatch: m,
        } => ItemResultView::Lost {
            sends: sat8(*sends),
            packets: sat16(*packets),
            warnings: warnings(w),
            mismatch: mismatch(m),
        },
        ItemResult::MaybeLost {
            sends,
            packets,
            warnings: w,
        } => ItemResultView::MaybeLost {
            sends: sat8(*sends),
            packets: sat16(*packets),
            warnings: warnings(w),
        },
        ItemResult::AlreadyMatched => ItemResultView::AlreadyMatched,
        ItemResult::Failed { error, mismatch: m } => ItemResultView::Failed {
            error: AppError::from(error),
            mismatch: m.as_ref().map(mismatch),
        },
        ItemResult::NotAttempted => ItemResultView::NotAttempted,
    }
}

fn item(report: &ItemReport) -> ItemReportView {
    ItemReportView {
        table: table_ref(report.table),
        key: report.key.map(key_ref),
        commit_trigger: report.commit_trigger.map(commit_reason),
        resent_unchanged: report.resent_unchanged,
        result: item_result(&report.result),
    }
}

fn outcome(outcome: Outcome) -> OutcomeView {
    match outcome {
        Outcome::Completed => OutcomeView::Completed,
        Outcome::NothingToDo => OutcomeView::NothingToDo,
        Outcome::Cancelled => OutcomeView::Cancelled,
        Outcome::Failed => OutcomeView::Failed,
    }
}

/// The verdict, with the CLI's precedence.
pub(crate) fn headline(
    outcome: Outcome,
    lost: usize,
    maybe_lost: usize,
    written: bool,
) -> Headline {
    match outcome {
        Outcome::NothingToDo => Headline::NothingToDo,
        _ if lost > 0 => Headline::Lost,
        _ if maybe_lost > 0 => Headline::MaybeLost,
        Outcome::Failed if written => Headline::Failed,
        Outcome::Failed => Headline::NotWritten,
        Outcome::Cancelled => Headline::Cancelled,
        Outcome::Completed => Headline::Done,
    }
}

/// An edit report.
pub(crate) fn edit_report_view(report: &EditReport, safety_backup: FileRef) -> EditReportView {
    let anything_written = report.anything_written();
    EditReportView {
        headline: headline(report.outcome, 0, report.maybe_lost(), anything_written),
        outcome: outcome(report.outcome),
        error: report.error.as_ref().map(AppError::from),
        profile: report.profile,
        active_profile_now: report.active_profile_now,
        items: report.items.iter().map(item).collect(),
        waited_for_commit_ms: report.waited_for_commit_ms.map(sat32),
        safety_backup,
        foreign_activity: report.foreign_activity.map(foreign),
        anything_written,
    }
}

/// A restore report; `skipped` is the number of tables the plan skipped.
pub(crate) fn restore_report_view(
    report: &RestoreReport,
    safety_backup: FileRef,
    skipped: usize,
) -> RestoreReportView {
    let count =
        |f: &dyn Fn(&ItemReport) -> bool| sat16(report.items().filter(|(_, i)| f(i)).count());
    let counts = ReportCounts {
        written: sat16(report.written()),
        persisted: count(&|i| {
            matches!(
                i.result,
                ItemResult::Written {
                    persistence: Persistence::Persisted,
                    ..
                }
            )
        }),
        lost: sat16(report.lost()),
        maybe_lost: sat16(report.maybe_lost()),
        failed: count(&|i| matches!(i.result, ItemResult::Failed { .. })),
        not_attempted: count(&|i| matches!(i.result, ItemResult::NotAttempted)),
        already_matched: count(&|i| matches!(i.result, ItemResult::AlreadyMatched)),
        resent_unchanged: sat16(report.resent_unchanged()),
        skipped: sat16(skipped),
    };
    let anything_written = report.anything_written();
    RestoreReportView {
        headline: headline(
            report.outcome,
            report.lost(),
            report.maybe_lost(),
            anything_written,
        ),
        outcome: outcome(report.outcome),
        error: report.error.as_ref().map(AppError::from),
        original_profile: report.original_profile,
        active_profile_now: report.active_profile_now,
        profiles: report
            .profiles
            .iter()
            .map(|p| ProfileReportView {
                profile: p.profile,
                items: p.items.iter().map(item).collect(),
            })
            .collect(),
        persistence: report.persistence.as_ref().map(|p| PersistenceCheckView {
            profiles: p.profiles.clone(),
            persisted: sat16(p.persisted),
            lost: sat16(p.lost),
            not_checked: sat16(p.not_checked),
        }),
        final_check: report.final_check.as_ref().map(|f| FinalCheckView {
            profiles: f.profiles.clone(),
            mismatches: f
                .mismatches
                .iter()
                .map(|m| FinalMismatchView {
                    profile: m.profile,
                    mismatch: mismatch(&m.mismatch),
                })
                .collect(),
        }),
        counts,
        safety_backup,
        foreign_activity: report.foreign_activity.map(foreign),
        anything_written,
    }
}

/// A profile switch.
pub(crate) fn profile_switch_report(
    from: u8,
    to: u8,
    outcome: &SelectOutcome,
    active_profile_now: Option<u8>,
) -> ProfileSwitchReport {
    let waited = outcome.warnings.iter().find_map(|w| match w {
        WriteWarning::WaitedForCommit { waited_ms } => Some(sat32(*waited_ms)),
        _ => None,
    });
    ProfileSwitchReport {
        from,
        to,
        active_profile_now,
        waited_for_commit_ms: waited,
        warnings: warnings(&outcome.warnings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headlines_follow_the_cli_precedence() {
        assert_eq!(
            headline(Outcome::NothingToDo, 1, 1, true),
            Headline::NothingToDo
        );
        assert_eq!(headline(Outcome::Failed, 1, 1, true), Headline::Lost);
        assert_eq!(headline(Outcome::Failed, 0, 1, true), Headline::MaybeLost);
        assert_eq!(headline(Outcome::Failed, 0, 0, true), Headline::Failed);
        assert_eq!(headline(Outcome::Failed, 0, 0, false), Headline::NotWritten);
        assert_eq!(
            headline(Outcome::Cancelled, 0, 0, true),
            Headline::Cancelled
        );
        assert_eq!(headline(Outcome::Completed, 0, 0, true), Headline::Done);
    }
}
