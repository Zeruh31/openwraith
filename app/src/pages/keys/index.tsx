// The Keys page (`#/keys?tab=assign|macros|advanced`): what each key does on
// the active profile. The page loads the keymap group of the last full read
// (`get_keys_state`, no keyboard traffic; refetched when the snapshot
// changes), shows the shared states (not connected, reading, stale,
// disabled) and hands the loaded state to the tab. Every change goes
// through the shared KeymapEditFlow (prepare → review → Write → result
// with Undo); nothing is written from a gesture.

import type { ComponentType } from "react";

import { Button } from "../../components/Button";
import { Callout } from "../../components/Feedback";
import { Card, PageHeader } from "../../components/Layout";
import { Tip } from "../../components/Tip";
import { useDict } from "../../i18n";
import { useTab, type TabId } from "../../routes";
import { readAgain } from "../../shell/actions";
import { NotReadEmptyState } from "../../shell/NotReadEmptyState";
import { TabbedPage } from "../../shell/PageTabs";
import { ReadAt } from "../../shell/ReadAt";
import { useOperationLock } from "../../state/operation";
import { useStatus } from "../../state/status";
import AdvancedTab from "./advanced";
import AssignTab from "./assign";
import MacrosTab from "./macros";
import { keysBlockReason, useKeysFlowOpen, useKeysState, type KeysTabProps } from "./shared";
import { tableName } from "../../flow/labels";
import "./keys.css";

const TABS: Record<TabId<"keys">, ComponentType<KeysTabProps>> = {
  assign: AssignTab,
  macros: MacrosTab,
  advanced: AdvancedTab,
};

/** The Keys page. */
export default function KeysPage() {
  const dict = useDict();
  const status = useStatus();
  const lock = useOperationLock();
  const view = useKeysState();
  const [tab] = useTab("keys");
  const t = dict.keys;
  const flowOpen = useKeysFlowOpen();
  // While a write flow is open, the tab stays with the last state read (read-only: the
  // block reason says why), so the flow's progress or report never disappears.
  const state = view.state ?? (flowOpen ? view.last : null);
  const Tab = TABS[tab];
  const blockReason = keysBlockReason(view, status, dict);

  let body;
  if (state === null) {
    body = (
      <Card>
        <NotReadEmptyState
          icon="keys"
          title={t.empty.title}
          body={t.empty.body}
          failed={t.empty.failed}
          loading={view.loading}
          error={view.error !== null}
        />
      </Card>
    );
  } else {
    const disabled = state.disabled;
    body = (
      <>
        {disabled === null ? null : (
          <Callout kind="warning" role="status">
            {disabled.reason === "unreadable_table"
              ? t.disabled.unreadable_table({ table: tableName(disabled.table, dict) })
              : t.disabled.unsupported}
          </Callout>
        )}
        <Tip id="keys.intro" title={t.tip.title}>
          {t.tip.body}
        </Tip>
        {view.stale ? (
          <Callout
            kind="info"
            actions={
              <Button icon="refresh" size="sm" disabled={lock.busy} onClick={readAgain}>
                {dict.common.actions.readAgain}
              </Button>
            }
          >
            {t.stale}
          </Callout>
        ) : null}
        <Tab state={state} view={view} blockReason={blockReason} />
      </>
    );
  }

  return (
    <TabbedPage page="keys">
      <div className="keys-page">
        <PageHeader
          title={t.title}
          description={t.description}
          actions={state === null ? undefined : <ReadAt time={state.read_at} prefix={t.board.title({ n: state.profile })} />}
        />
        {body}
      </div>
    </TabbedPage>
  );
}
