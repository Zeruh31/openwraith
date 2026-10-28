// A one-time tip: an info callout with "Got it", shown until the user
// dismisses it (remembered in settings.json; Settings > Tips shows them
// again).

import type { ReactNode } from "react";

import { useDict } from "../i18n";
import { useHint } from "../state/hints";
import { Button } from "./Button";
import { Callout } from "./Feedback";

/** Props of {@link Tip}. */
export interface TipProps {
  /** The tip's id (1-64 letters, digits, `_`, `-` or `.`). */
  id: string;
  title: ReactNode;
  children: ReactNode;
}

/** A tip shown until it is dismissed. */
export function Tip({ id, title, children }: TipProps) {
  const dict = useDict();
  const hint = useHint(id);
  if (!hint.show) return null;
  return (
    <Callout
      kind="info"
      title={title}
      className="ow-tip"
      actions={
        <Button size="sm" onClick={hint.dismiss}>
          {dict.common.tip.dismiss}
        </Button>
      }
    >
      {children}
    </Callout>
  );
}
