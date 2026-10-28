// The confirmation body of a key-colour edit, from the backend's read-only
// plan (not the tab's draft): the boards before and after, the changes
// grouped by old and new colour, what the effect does (the opt-in switch to
// Custom, or that the colours stay hidden until the effect is Custom), the
// plan's warnings with their evidence, and the writes under technical
// details.

import { Badge } from "../../../components/Badge";
import { Callout } from "../../../components/Feedback";
import { ColorSwatch, Table, TechnicalDetails, type TableColumn } from "../../../components/Layout";
import { tableName } from "../../../flow/labels";
import { useDict } from "../../../i18n";
import type { ColorEditPreview, ColorsState } from "../../../ipc/bindings";
import { modeName } from "../labels";
import { colorText, keysListText, warningText } from "./labels";
import { MiniBoard } from "./MiniBoard";
import { afterColors, beforeColors, groupChanges, keyboardColors, type ChangeGroup } from "./model";

/** Props of {@link ColorsReview}. */
export interface ColorsReviewProps {
  preview: ColorEditPreview;
  /** The colours the draft was made on (the keys the plan doesn't change). */
  state: ColorsState;
}

/** A colour with its swatch, its name and its hex value. */
function ColorCell({ color }: { color: string }) {
  const dict = useDict();
  const text = colorText(dict, color);
  return (
    <span className="li-colors__cell">
      <ColorSwatch color={color} showHex={false} />
      <span>{text}</span>
    </span>
  );
}

/** What a key-colour edit will write. */
export function ColorsReview({ preview, state }: ColorsReviewProps) {
  const dict = useDict();
  const t = dict.lighting.colors.review;
  const current = keyboardColors(state);
  const before = beforeColors(current, preview.changes);
  const after = afterColors(current, preview.changes);
  const changed = new Set(preview.changes.map((change) => change.key.index));
  const groups = groupChanges(preview.changes);
  const effect = state.light_mode === null ? null : modeName(dict, state.light_mode);

  const columns: readonly TableColumn<ChangeGroup>[] = [
    {
      key: "keys",
      header: t.keys,
      rowHeader: true,
      render: (group) => keysListText(dict, group.keys.map((key) => key.name)),
    },
    { key: "now", header: t.now, render: (group) => <ColorCell color={group.before} /> },
    { key: "new", header: t.new, render: (group) => <ColorCell color={group.after} /> },
  ];

  const notes = preview.warnings
    .map((warning, index) => ({ warning, index, text: warningText(dict, warning) }))
    .filter((note) => note.text !== null && note.warning.code !== "not_shown_until_custom_mode");
  const notShown = preview.warnings.some((warning) => warning.code === "not_shown_until_custom_mode");
  const otherMessages = preview.warnings.flatMap((warning) => (warning.code === "other" ? [warning.message] : []));

  const technical = [
    ...preview.writes.map((write) =>
      t.technical.write({ table: tableName(write.table, dict), packets: write.packets, summary: write.summary }),
    ),
    t.technical.total({ n: preview.packets }),
    ...(state.active_table === null ? [] : [t.technical.activeTable({ n: state.active_table })]),
    ...otherMessages,
  ];

  return (
    <div className="li-colors-review ow-stack">
      <p>{t.intro({ n: preview.profile })}</p>
      <div className="li-colors-review__boards">
        <MiniBoard caption={t.before} colors={before} />
        <MiniBoard caption={t.after} colors={after} changed={changed} />
      </div>
      <section className="li-colors-review__section">
        <h3 className="li-colors-review__title">{t.changesTitle}</h3>
        <Table
          columns={columns}
          rows={groups}
          rowKey={(group) => `${group.before}>${group.after}`}
          limit={8}
        />
      </section>
      {preview.switches_to_custom ? (
        <Callout kind="info">
          <p>{effect === null ? t.switchesToCustomUnknown : t.switchesToCustom({ effect })}</p>
        </Callout>
      ) : null}
      {notShown ? (
        <Callout kind="warning" title={dict.lighting.colors.notShown.title}>
          <p>{t.notShownBody}</p>
        </Callout>
      ) : null}
      {notes.length === 0 ? null : (
        <ul className="li-colors-review__notes">
          {notes.map((note) => (
            <li key={`${note.warning.code}-${note.index}`}>
              {note.warning.code === "unverified" ? (
                <>
                  <Badge kind="unverified" notes={[note.warning.note]} size="sm" />{" "}
                </>
              ) : null}
              {note.text}
            </li>
          ))}
        </ul>
      )}
      <TechnicalDetails>{technical.join("\n")}</TechnicalDetails>
    </div>
  );
}
