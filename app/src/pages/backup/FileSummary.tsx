// The chosen backup: kind, name, when it was made, keyboard, firmware,
// profiles, the program that made it, and whether it fits the keyboard.

import { IconButton } from "../../components/Button";
import { Callout } from "../../components/Feedback";
import { Icon } from "../../components/Icon";
import { KeyValueList, type KeyValueItem } from "../../components/Layout";
import { useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { BackupFileSummary } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { toastError } from "../../shell/actions";
import { profilesLabel } from "./texts";

/** The chosen backup's facts. */
export function FileSummary({ summary }: { summary: BackupFileSummary }) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.backupRestore.restore;
  const unknown = dict.common.unknown;
  const model = summary.model ?? summary.product ?? unknown;
  const items: KeyValueItem[] = [
    { key: "made", label: t.facts.made, value: <span className="ow-num">{format.dateTime(summary.created_at)}</span> },
    {
      key: "keyboard",
      label: t.facts.keyboard,
      value: (
        <span className="ow-row">
          <span>{model}</span>
          {summary.product === null || summary.product === model ? null : (
            <span className="ow-mono ow-muted">{summary.product}</span>
          )}
        </span>
      ),
    },
    { key: "firmware", label: t.facts.firmware, value: <span className="ow-num">{summary.firmware ?? unknown}</span> },
    { key: "profiles", label: t.facts.profiles, value: profilesLabel(summary.profiles, dict, format) },
    {
      key: "tool",
      label: t.facts.tool,
      value: <span className="ow-mono">{`${summary.tool.name} ${summary.tool.version}`}</span>,
    },
  ];
  if (summary.safety_reason !== null) {
    items.push({ key: "reason", label: t.facts.reason, value: dict.backupRestore.safetyReasons[summary.safety_reason] });
  }
  return (
    <div className="br-file">
      <div className="br-file__head">
        <Icon name={summary.kind === "safety" ? "lock" : "backup"} size={20} className="br-file__icon" />
        <div className="br-file__names">
          <p className="ow-muted ow-small">{t.kinds[summary.kind]}</p>
          <p className="br-file__name ow-mono ow-break ow-selectable">{summary.file.file_name}</p>
          <p className="ow-muted ow-small ow-mono ow-break ow-selectable">{summary.file.display_path}</p>
        </div>
        <IconButton
          icon="folder"
          size="sm"
          label={dict.common.actions.showInFolder}
          onClick={() => {
            api.reveal({ target: "file", file: summary.file.token }).catch(toastError);
          }}
        />
      </div>
      <KeyValueList items={items} />
      {summary.made_by_simulator ? <Callout kind="warning">{t.simulator}</Callout> : null}
      {summary.same_model === false ? (
        <Callout kind="danger" title={t.otherKeyboardTitle} role="alert">
          <p>{t.otherKeyboardBody({ model })}</p>
        </Callout>
      ) : null}
      {summary.same_model === null ? <p className="ow-muted ow-small">{t.notChecked}</p> : null}
      {summary.undecoded > 0 ? <p className="ow-muted ow-small">{t.undecoded({ n: summary.undecoded })}</p> : null}
    </div>
  );
}
