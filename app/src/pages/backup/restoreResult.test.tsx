// A restore that wrote Macro 1 and stopped before writing the other macros
// again (writing Macro 1 makes the keyboard clear Macros 2–10) names the
// macros it left empty and the keys bound to them; a macro written again
// says why.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type { RestoreReportView } from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { RestoreResult } from "./RestoreResult";

/** The recorded report of the "LOST" scenario, with a stop after Macro 1. */
function stoppedAfterMacro1(): RestoreReportView {
  const fixture = FIXTURES["restore_lost"];
  const call = fixture?.calls.find((c) => c.command === "apply_restore" && c.ok !== null);
  if (call?.command !== "apply_restore" || call.ok === null) throw new Error("no recorded restore report");
  const report = call.ok;
  return {
    ...report,
    profiles: [
      {
        profile: 3,
        items: [
          {
            table: { table: "macro", slot: 2 },
            key: null,
            commit_trigger: null,
            resent_unchanged: false,
            resent: "macro_slot0_clears",
            result: { result: "not_attempted" },
          },
        ],
      },
    ],
    cleared_macros: [{ profile: 3, slot: 2 }],
    dangling: [
      {
        profile: 3,
        layer: "fn",
        key: { index: 45, row: 2, col: 1, name: "Q" },
        slot: { table: "macro", slot: 2 },
      },
    ],
  };
}

beforeEach(() => {
  statusStore.set({ ...INITIAL_STATUS, connection: { state: "ready" } });
});

afterEach(() => {
  cleanup();
  setLang("en");
});

describe("a restore that left macros empty", () => {
  it("names the emptied macros and the keys bound to them (EN)", () => {
    setLang("en");
    const r = DICTIONARIES.en.backupRestore.result;
    render(<RestoreResult report={stoppedAfterMacro1()} onReadAgain={() => undefined} />);
    expect(screen.getByText(r.clearedTitle)).toBeTruthy();
    expect(screen.getByText("Profile 3: Macro 3")).toBeTruthy();
    expect(screen.getByText(r.danglingTitle)).toBeTruthy();
    expect(screen.getByText("Profile 3: Fn + Q → Macro 3")).toBeTruthy();
    expect(screen.getByText(`· ${DICTIONARIES.en.flow.resent.macro_slot0_clears}`, { exact: false })).toBeTruthy();
  });

  it("speaks Turkish too", () => {
    setLang("tr");
    const r = DICTIONARIES.tr.backupRestore.result;
    render(<RestoreResult report={stoppedAfterMacro1()} onReadAgain={() => undefined} />);
    expect(screen.getByText(r.clearedTitle)).toBeTruthy();
    expect(screen.getByText("Profil 3: Makro 3")).toBeTruthy();
    expect(screen.getByText("Profil 3: Fn + Q → Makro 3")).toBeTruthy();
  });
});
