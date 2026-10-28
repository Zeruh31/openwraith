// The keyboard's own SOCD switch (Fn + Right Shift) is announced on every
// page: the keyboard pushes the new power state and the shell toasts it.

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../i18n";
import { NoticeToaster } from "../shell/NoticeToaster";
import { emitNotice } from "../state/status";
import { clearToasts, getToasts } from "../state/toasts";

const en = DICTIONARIES.en;

beforeEach(() => {
  setLang("en");
  clearToasts();
});

afterEach(() => {
  cleanup();
  clearToasts();
});

describe("notices", () => {
  it("toast an SOCD switch made on the keyboard", () => {
    render(<NoticeToaster />);
    act(() => {
      emitNotice({ notice: "socd_toggled_on_keyboard", power: false });
    });
    expect(getToasts().map((t) => t.title)).toEqual([en.shell.notices.socdToggled({ on: false })]);
    expect(en.shell.notices.socdToggled({ on: true })).toBe("SOCD turned on on the keyboard (Fn + Right Shift).");
    expect(DICTIONARIES.tr.shell.notices.socdToggled({ on: false })).toBe("SOCD klavyeden kapatıldı (Fn + Sağ Shift).");
  });
});
