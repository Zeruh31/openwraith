// What a profile file holds: the parts with readable settings, the parts it
// lacks, the advanced keys in use and every macro as the keys it presses
// (macros travel with the file, so whoever gets it can read them).

import { useId } from "react";

import { Callout } from "../../../components/Feedback";
import { Icon } from "../../../components/Icon";
import { Disclosure } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { ShareContentsView, SharedMacroView } from "../../../ipc/bindings";
import { chosenParts, macroStepText, partsText, playText, SHARE_PARTS } from "./model";

/** Macros are shown open while all of them together have at most this many steps. */
const OPEN_STEPS = 12;

function MacroSteps({ macro, open }: { macro: SharedMacroView; open: boolean }) {
  const dict = useDict();
  const format = useFormat();
  const c = dict.backupRestore.share.contents;
  const program = macro.program;
  if (program === null) {
    return (
      <Callout kind="warning">
        <p>{c.macroUnknown({ n: macro.slot + 1 })}</p>
      </Callout>
    );
  }
  return (
    <Disclosure
      summary={c.macro({ n: macro.slot + 1, steps: program.steps.length, play: playText(program.play, dict) })}
      defaultOpen={open}
    >
      <ol className="br-share__steps">
        {program.steps.map((step, index) => (
          <li key={index}>{macroStepText(step, dict, format)}</li>
        ))}
      </ol>
    </Disclosure>
  );
}

/** Props of {@link ShareContents}. */
export interface ShareContentsProps {
  contents: ShareContentsView;
  /** The section's heading ("What the file holds"). */
  title: string;
}

/** What a profile file holds. */
export function ShareContents({ contents, title }: ShareContentsProps) {
  const dict = useDict();
  const headingId = useId();
  const macrosId = useId();
  const c = dict.backupRestore.share.contents;
  const present = chosenParts(contents.present);
  const missing = SHARE_PARTS.filter((part) => !contents.present[part]);
  const steps = contents.macros.reduce((sum, macro) => sum + (macro.program?.steps.length ?? 0), 0);
  return (
    <section className="br-share__contents" aria-labelledby={headingId}>
      <h4 id={headingId} className="br-section__title">
        {title}
      </h4>
      {present.length === 0 ? (
        <p className="ow-muted">{c.none}</p>
      ) : (
        <ul className="br-share__parts">
          {present.map((part) => (
            <li key={part}>
              <Icon name="check" size={16} className="br-share__check" />
              <span>{dict.backupRestore.share.parts[part]}</span>
            </li>
          ))}
        </ul>
      )}
      {present.length > 0 && missing.length > 0 ? (
        <p className="ow-muted ow-small">{c.missing({ list: partsText(missing, dict) })}</p>
      ) : null}
      {contents.advanced_slots > 0 ? <p>{c.advanced({ n: contents.advanced_slots })}</p> : null}
      {contents.macros.length === 0 ? null : (
        <div className="br-section" role="group" aria-labelledby={macrosId}>
          <p id={macrosId} className="ow-strong">
            {c.macrosTitle}
          </p>
          <ul className="br-share__macros">
            {contents.macros.map((macro) => (
              <li key={macro.slot}>
                <MacroSteps macro={macro} open={steps <= OPEN_STEPS} />
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
