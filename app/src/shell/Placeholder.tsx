// The stand-in for a tab or section that a later part of this version
// fills in: it says so, and it reads or writes nothing.

import { Card } from "../components/Layout";
import { EmptyState } from "../components/Feedback";
import { useDict } from "../i18n";

/** A tab or section that isn't built yet. */
export function TabPlaceholder() {
  const dict = useDict();
  return (
    <Card>
      <EmptyState icon="info" title={dict.shell.placeholder.title}>
        <p>{dict.shell.placeholder.body}</p>
      </EmptyState>
    </Card>
  );
}
