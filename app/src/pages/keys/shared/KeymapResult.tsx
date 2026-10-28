// The result of a key-assignment write: the shared edit report (headline
// with the persisted wording, a note when Stop came too late to skip
// anything, emptied macros and keys left on them, the persistence check,
// every write with its reasons including re-sent macros, and the footer
// with the safety backup and Undo).

import { EditReportView } from "../../../flow/Report";
import type { EditReportView as EditReport } from "../../../ipc/bindings";

/** The result body of a key-assignment write. */
export function KeymapResult({ report }: { report: EditReport }) {
  return <EditReportView report={report} />;
}
