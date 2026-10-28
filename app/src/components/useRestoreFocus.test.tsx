// A save that disables the focused control gives the focus back afterwards.

import { act, cleanup, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { useRestoreFocus } from "./useRestoreFocus";

/** Labels of the test component (not UI text). */
const L = { first: "first", second: "second" } as const;

function Saving({ saving }: { saving: boolean }) {
  useRestoreFocus(saving);
  const [n] = useState(0);
  return (
    <div data-n={n}>
      <button type="button" disabled={saving}>
        {L.first}
      </button>
      <button type="button" disabled={saving}>
        {L.second}
      </button>
    </div>
  );
}

afterEach(cleanup);

describe("useRestoreFocus", () => {
  it("puts the focus back on the control it was on when the save began", () => {
    const { rerender } = render(<Saving saving={false} />);
    const second = screen.getByRole("button", { name: L.second });
    act(() => {
      second.focus();
    });
    // Disabling the focused control drops the focus to <body> in Chrome
    // (jsdom keeps it, so the drop is made by hand).
    act(() => {
      second.blur();
    });
    rerender(<Saving saving />);
    expect(document.activeElement).toBe(document.body);
    rerender(<Saving saving={false} />);
    expect(document.activeElement).toBe(second);
  });
});
