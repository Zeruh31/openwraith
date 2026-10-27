// An `AppError` in words: the localized title and body, the detail line,
// whether anything was written, and the English message under "Technical
// details".

import { Callout } from "../components/Feedback";
import { Disclosure, TechnicalDetails } from "../components/Layout";
import { useDict } from "../i18n";
import { useFormat } from "../i18n/format";
import type { AppError } from "../ipc/bindings";
import { activeProfileOf, describeDetail } from "./labels";

/** What the error says about writes. */
export type WrittenNote =
  /** Nothing was written (invariant E of every command). */
  | "nothing"
  /** Nothing was written, but profile switches may have happened (restore compare, all-profile backup). */
  | "switches"
  /** No statement (not a device command). */
  | "none";

/** Props of {@link ErrorView}. */
export interface ErrorViewProps {
  error: AppError;
  written?: WrittenNote | undefined;
  /** Use the warning look instead of danger (e.g. "cancelled"). */
  soft?: boolean | undefined;
}

/** An error, explained. */
export function ErrorView({ error, written = "nothing", soft }: ErrorViewProps) {
  const dict = useDict();
  const format = useFormat();
  const kind = dict.errors.kind[error.kind];
  const detail = error.detail === null ? null : describeDetail(error.detail, dict, format);
  // What another program sent is technical: it goes behind a disclosure.
  const evidence = error.detail?.detail === "other_program";
  const profile = activeProfileOf(error);
  let writtenText: string | null = null;
  if (written === "nothing") {
    writtenText = dict.flow.nothingWritten;
  } else if (written === "switches") {
    writtenText =
      profile === undefined || profile === null
        ? dict.flow.nothingWrittenSwitchesUnknown
        : dict.flow.nothingWrittenSwitches({ n: profile });
  }
  const gentle = soft ?? (error.kind === "cancelled" || error.kind === "stale_plan");
  return (
    <Callout kind={gentle ? "warning" : "danger"} title={kind.title} role="alert">
      <p>{kind.body}</p>
      {detail === null ? null : evidence ? (
        <Disclosure summary={dict.shell.banners.otherProgramDetails}>
          <p>{detail}</p>
        </Disclosure>
      ) : (
        <p>{detail}</p>
      )}
      {writtenText === null ? null : <p className="ow-strong">{writtenText}</p>}
      {error.message === "" ? null : <TechnicalDetails>{error.message}</TechnicalDetails>}
    </Callout>
  );
}
