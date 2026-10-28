// The review of a key-assignment plan (`KeymapEditPreview`), in words: what
// changes (Layer | Key | Now | New; macro and advanced-key slots before →
// after), every write in execution order with its own reasons (re-sends
// after Macro 1, the Fn layer sent as commit trigger, writes that complete a
// binding), the commit note derived from the plan (never a fixed sentence),
// warnings (built-ins replaced, "Not yet verified", values kept as read,
// Undo limits) and the bytes under "Technical details".

import { EvidenceBadge, unverifiedWhat } from "../../../components/Badge";
import { Callout } from "../../../components/Feedback";
import { Table, TechnicalDetails, type TableColumn } from "../../../components/Layout";
import { builtinName, keyRefText, tableName, writeReasonTexts } from "../../../flow/labels";
import { useDict, type Dict } from "../../../i18n";
import type {
  KeyEntryChangeView,
  KeymapEditPreview,
  KeymapWarningView,
  KeymapWriteView,
  KeysState,
} from "../../../ipc/bindings";
import { entryText, keyAt, slotContentText, type LabelContext } from "./labels";

/** The commit note of a plan, from its own writes: why extra writes are needed, or that none are. */
export function commitNoteText(writes: readonly KeymapWriteView[], dict: Dict): string | null {
  const note = dict.keys.review.commitNote;
  if (writes.some((write) => write.commit_trigger === "base_keymap")) return note.baseKeymap;
  if (writes.some((write) => write.commit_trigger === "advanced_keys")) return note.advancedKeys;
  if (writes.length > 0 && writes.every((write) => write.table.table === "macro")) return note.ownOnly;
  return null;
}

/** The profile number a critical built-in switches to (Fn + 1..4), from its key's legend. */
export function profileOfBuiltin(keyName: string): number | null {
  const n = Number(keyName);
  return Number.isInteger(n) && n >= 1 && n <= 4 ? n : null;
}

/**
 * The tone and title of a critical built-in replaced on `profile`: Fn + k on
 * profile k switches nowhere, so replacing it costs nothing (a warning with
 * its own title, the confirmation stays); Fn + 1..4 on another profile and
 * Fn + Space are a danger. The same rule everywhere the replacement is shown.
 */
export function criticalBuiltinCallout(
  keyName: string,
  profile: number,
  dict: Dict,
): { kind: "danger" | "warning"; title: string } {
  return profileOfBuiltin(keyName) === profile
    ? { kind: "warning", title: dict.keys.review.ownProfileTitle }
    : { kind: "danger", title: dict.keys.review.criticalTitle };
}

/** A warning in words (built-in warnings are rendered separately). */
export function warningText(warning: KeymapWarningView, dict: Dict): string {
  const w = dict.keys.review.warnings;
  switch (warning.code) {
    case "fn_builtin_overridden":
      return w.builtin({
        key: keyRefText(warning.key, "fn", dict),
        function: builtinName(warning.key.index, warning.function, dict),
      });
    case "unverified":
      return w.unverified({ what: unverifiedWhat(warning.note, dict) });
    case "malformed_reference_preserved":
      return w.malformed_reference_preserved({ key: keyRefText(warning.key, warning.layer, dict) });
    case "dangling_reference_preserved":
      return w.dangling_reference_preserved({
        key: keyRefText(warning.key, warning.layer, dict),
        slot: tableName(warning.slot, dict),
      });
    case "transient_binding":
      return w.transient_binding({ slot: tableName(warning.slot, dict) });
    case "orphan_slot_kept":
      return w.orphan_slot_kept({ slot: tableName(warning.slot, dict) });
    case "not_undoable":
      return w.not_undoable({ table: tableName(warning.table, dict) });
    case "other":
      return warning.message;
  }
}

/** What a plan changes, counted for the summary: plain key assignments, macro slots and advanced-key slots. */
export function reviewCounts(preview: KeymapEditPreview): { keys: number; macros: number; advanced: number } {
  const isSlot = (origin: { origin: string }) => origin.origin === "macro_slot" || origin.origin === "advanced_slot";
  return {
    // A key bound to (or freed from) a slot is part of that macro or advanced key.
    keys: preview.entry_changes.filter((row) => !isSlot(row.before.origin) && !isSlot(row.after.origin)).length,
    macros: preview.slot_changes.filter((slot) => slot.slot.table === "macro").length,
    advanced: preview.slot_changes.filter((slot) => slot.slot.table === "advanced_key").length,
  };
}

/** What a plan changes, for the review's title. */
export function reviewWhat(preview: KeymapEditPreview): "keys" | "macros" | "advanced" | "mixed" {
  const counts = reviewCounts(preview);
  const bound = (kind: string) => preview.entry_changes.some((row) => row.before.origin.origin === kind || row.after.origin.origin === kind);
  const kinds = [
    counts.keys > 0 ? "keys" : null,
    counts.macros > 0 || bound("macro_slot") ? "macros" : null,
    counts.advanced > 0 || bound("advanced_slot") ? "advanced" : null,
  ].filter((kind): kind is "keys" | "macros" | "advanced" => kind !== null);
  const [only] = kinds;
  return kinds.length === 1 && only !== undefined ? only : kinds.length === 0 ? "keys" : "mixed";
}

/** Props of {@link KeymapReview}. */
export interface KeymapReviewProps {
  preview: KeymapEditPreview;
  /** The keys state the plan was made against (names slots, built-ins). */
  state: KeysState | null;
  libraryNames?: ReadonlyMap<string, string> | undefined;
}

/** The review body of a key-assignment plan. */
export function KeymapReview({ preview, state, libraryNames }: KeymapReviewProps) {
  const dict = useDict();
  const t = dict.keys.review;
  const ctx: LabelContext = { state, libraryNames };
  const builtins = preview.warnings.filter(
    (w): w is Extract<KeymapWarningView, { code: "fn_builtin_overridden" }> => w.code === "fn_builtin_overridden",
  );
  const unverified = preview.warnings.filter(
    (w): w is Extract<KeymapWarningView, { code: "unverified" }> => w.code === "unverified",
  );
  const others = preview.warnings.filter((w) => w.code !== "fn_builtin_overridden" && w.code !== "unverified");
  const commitNote = commitNoteText(preview.writes, dict);

  const columns: TableColumn<KeyEntryChangeView>[] = [
    { key: "layer", header: t.columns.layer, render: (row) => dict.keys.layerShort[row.layer] },
    {
      key: "key",
      header: t.columns.key,
      rowHeader: true,
      render: (row) => keyRefText(row.key, row.layer, dict),
    },
    {
      key: "now",
      header: t.columns.now,
      render: (row) => (
        <span className="keys-entry">
          <span>{entryText(row.before, keyAt(state, row.key.index), ctx, dict)}</span>
          <EvidenceBadge evidence={row.before.evidence} keptWhenNone={row.before.preserved !== null} size="sm" />
        </span>
      ),
    },
    {
      key: "new",
      header: t.columns.new,
      render: (row) => (
        <span className="keys-entry ow-strong">
          <span>{entryText(row.after, keyAt(state, row.key.index), ctx, dict)}</span>
          <EvidenceBadge evidence={row.after.evidence} keptWhenNone={row.after.preserved !== null} size="sm" />
        </span>
      ),
    },
  ];

  const technical = [
    ...preview.writes.map((write) => `${tableName(write.table, dict)}: ${write.summary} (${t.packets({ n: write.packets })})`),
    ...preview.slot_changes.map((slot) => slot.summary),
    t.packets({ n: preview.packets }),
  ].join("\n");

  return (
    <div className="keys-review ow-stack">
      <p>
        {t.summary({ ...reviewCounts(preview), writes: preview.writes.length })}
      </p>
      {builtins.map((warning) => {
        const k = profileOfBuiltin(warning.key.name);
        const critical = warning.critical;
        const tone = critical ? criticalBuiltinCallout(warning.key.name, preview.profile, dict) : null;
        return (
          <Callout key={`b-${warning.key.index}`} kind={tone?.kind ?? "warning"} title={tone?.title}>
            <p>{warningText(warning, dict)}</p>
            {critical ? (
              <p>
                {k === null
                  ? dict.keys.builtinReset
                  : dict.keys.builtinProfile({
                      k,
                      n: preview.profile,
                      likely: !(state?.keys.find((key) => key.key.index === warning.key.index)?.builtin?.label_confirmed ?? false),
                    })}
              </p>
            ) : null}
          </Callout>
        );
      })}
      {preview.entry_changes.length === 0 ? null : (
        <section className="keys-review__section">
          <h3 className="keys-review__title">{t.entriesTitle}</h3>
          <Table
            columns={columns}
            rows={preview.entry_changes}
            rowKey={(row) => `${row.layer}-${row.key.index}`}
            limit={12}
          />
        </section>
      )}
      {preview.slot_changes.length === 0 ? null : (
        <section className="keys-review__section">
          <h3 className="keys-review__title">{t.slotsTitle}</h3>
          <ul className="keys-review__slots">
            {preview.slot_changes.map((slot) => (
              <li key={`${slot.slot.table}-${"slot" in slot.slot ? slot.slot.slot : 0}`}>
                <span className="ow-strong">{tableName(slot.slot, dict)}</span>
                {": "}
                <span>{slotContentText(slot.before, dict)}</span>
                {" → "}
                <span className="ow-strong">{slotContentText(slot.after, dict)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
      <section className="keys-review__section">
        <h3 className="keys-review__title">{t.writesTitle}</h3>
        <ol className="keys-review__writes">
          {preview.writes.map((write, index) => {
            const reasons = writeReasonTexts(write, dict);
            const unchanged = write.resent_unchanged || write.resent !== null;
            return (
              <li key={index} className="keys-review__write">
                <span className="keys-review__write-name">
                  <span className="ow-strong">{tableName(write.table, dict)}</span>
                  {unchanged ? <span className="keys-review__tag">{t.unchanged}</span> : null}
                </span>
                {reasons.length === 0 ? null : <span className="ow-small ow-muted">{reasons.join(" ")}</span>}
              </li>
            );
          })}
        </ol>
        {commitNote === null ? null : <p className="ow-small">{commitNote}</p>}
      </section>
      {unverified.length === 0 && others.length === 0 ? null : (
        <section className="keys-review__section">
          <h3 className="keys-review__title">{t.warningsTitle}</h3>
          <ul className="keys-review__warnings">
            {unverified.map((warning, index) => (
              <li key={`u-${index}`}>
                <EvidenceBadge evidence={{ status: "unverified", notes: [warning.note] }} size="sm" />{" "}
                {warningText(warning, dict)}
              </li>
            ))}
            {others.map((warning, index) => (
              <li key={`o-${index}`}>{warningText(warning, dict)}</li>
            ))}
          </ul>
        </section>
      )}
      {preview.persistence.will_run ? (
        <p className="ow-small">
          {preview.persistence.forced ? `${t.persistence.forced} ` : null}
          {t.persistence.tables({ list: preview.persistence.tables.map((table) => tableName(table, dict)).join(", ") })}
        </p>
      ) : null}
      <TechnicalDetails>{technical}</TechnicalDetails>
    </div>
  );
}
