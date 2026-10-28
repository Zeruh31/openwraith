// "Try it" after a write: a text box to press the macro's key into, so the
// user sees what the keyboard now types (the keyboard plays the macro; the
// box only shows what Windows receives). Nothing here talks to the keyboard.

import { useId, useState } from "react";

import { Button } from "../../../components/Button";
import { Card } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import { keyOnLayer } from "../shared";
import type { TryKey } from "./model";

/** Props of {@link TryIt}. */
export interface TryItProps {
  keys: readonly TryKey[];
  onDone: () => void;
}

/** The "Try it" field. */
export function TryIt({ keys, onDone }: TryItProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.keysMacros.tryIt;
  const id = useId();
  const [text, setText] = useState("");
  const names = format.list(keys.map((key) => keyOnLayer(key.name, key.layer, dict)));
  const fn = keys.some((key) => key.layer === "fn");
  return (
    <Card
      title={t.title}
      level={2}
      className="keys-macros__try"
      actions={
        <Button size="sm" onClick={onDone}>
          {t.done}
        </Button>
      }
    >
      <p>{t.body({ keys: names })}</p>
      {fn ? <p className="ow-small ow-muted">{t.fnHint}</p> : null}
      <div className="keys-macros__field">
        <label htmlFor={`${id}-field`} className="ow-field__label">
          {t.field}
        </label>
        <textarea
          id={`${id}-field`}
          className="ow-input keys-macros__try-field"
          rows={3}
          value={text}
          spellCheck={false}
          onChange={(event) => {
            setText(event.currentTarget.value);
          }}
        />
      </div>
      <div>
        <Button
          size="sm"
          variant="ghost"
          disabled={text === ""}
          onClick={() => {
            setText("");
          }}
        >
          {t.clear}
        </Button>
      </div>
    </Card>
  );
}
