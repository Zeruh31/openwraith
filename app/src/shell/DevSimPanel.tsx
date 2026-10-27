// Developer simulator controls (simulator builds only): events on the
// simulated keyboard, as if done by hand, and the simulator's counters.
// The header's simulator badge opens and closes the panel, so nothing
// floats over the pages' apply bars.

import { useEffect, useState } from "react";

import { Button } from "../components/Button";
import { KeyValueList } from "../components/Layout";
import { useDict } from "../i18n";
import type { SimAction, SimInfo } from "../ipc/bindings";
import { devApi } from "../ipc/commands";
import { createStore, useStore } from "../state/store";
import { toastError } from "./actions";

const panelOpen = createStore(false);

/** Opens or closes the simulator panel (the header's simulator badge). */
export function toggleDevSimPanel(): void {
  panelOpen.set((open) => !open);
}

/** Whether the simulator panel is open. */
export function useDevSimPanelOpen(): boolean {
  return useStore(panelOpen);
}

/** The simulator panel, below the header at the right. */
export function DevSimPanel() {
  const dict = useDict();
  const t = dict.shell.devSim;
  const open = useStore(panelOpen);
  const setOpen = (next: boolean) => {
    panelOpen.set(next);
  };
  const [info, setInfo] = useState<SimInfo | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    let alive = true;
    const load = () => {
      devApi
        .devSimInfo()
        .then((next) => {
          if (alive) setInfo(next);
        })
        .catch(() => {
          // The counters are best effort.
        });
    };
    load();
    const timer = window.setInterval(load, 2000);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [open]);

  const act = (action: SimAction) => {
    devApi.devSimAction(action).then(setInfo).catch(toastError);
  };

  const actions: { label: string; action: SimAction }[] = [
    { label: t.unplug, action: { action: "unplug" } },
    { label: t.replug, action: { action: "replug" } },
    { label: t.heartbeat, action: { action: "vendor_heartbeat", count: 10, interval_ms: 500 } },
    { label: t.switchReport, action: { action: "keyboard_switch", profile: 2, after_ms: 0, report: true } },
    { label: t.switchSilent, action: { action: "keyboard_switch", profile: 3, after_ms: 0, report: false } },
    { label: t.reset, action: { action: "keyboard_reset" } },
    { label: t.slowSave, action: { action: "slow_save", ms: 5000 } },
    { label: t.dropAck, action: { action: "drop_next_ack" } },
    { label: t.unplugAfter, action: { action: "unplug_after_packets", packets: 3 } },
  ];

  if (!open) {
    return null;
  }
  return (
    <aside id="ow-devsim" className="ow-devsim" aria-label={t.title}>
      <div className="ow-devsim__header">
        <h2>{t.title}</h2>
        <Button
          size="sm"
          variant="ghost"
          icon="close"
          onClick={() => {
            setOpen(false);
          }}
        >
          {dict.common.actions.close}
        </Button>
      </div>
      <p className="ow-muted ow-small">{t.hint}</p>
      <div className="ow-devsim__actions">
        {actions.map((item) => (
          <Button
            key={item.label}
            size="sm"
            onClick={() => {
              act(item.action);
            }}
          >
            {item.label}
          </Button>
        ))}
      </div>
      {info === null ? null : (
        <KeyValueList
          items={[
            { key: "p", label: t.activeProfile, value: info.active_profile },
            { key: "u", label: t.unsavedEdits, value: info.unsaved_edits ? dict.common.yes : dict.common.no },
            { key: "d", label: t.discardedEdits, value: info.discarded_edits },
            { key: "w", label: t.writesSent, value: info.writes_sent },
            {
              key: "v",
              label: t.violations,
              value: <span className={info.violations > 0 ? "ow-danger-text" : undefined}>{info.violations}</span>,
            },
            { key: "x", label: t.unplugged, value: info.unplugged ? dict.common.yes : dict.common.no },
          ]}
        />
      )}
    </aside>
  );
}
