import { describe, expect, it } from "vitest";

import { makeFormat } from "../../i18n/format";
import { buildDateText, parseBuildDate } from "./buildDate";

describe("firmware build date", () => {
  it("parses the keyboard's __DATE__,__TIME__ form, double spaces included", () => {
    const date = parseBuildDate("Aug  6 2026,13:46:54");
    expect(date?.getFullYear()).toBe(2026);
    expect(date?.getMonth()).toBe(7);
    expect(date?.getDate()).toBe(6);
    expect(date?.getHours()).toBe(13);
    expect(parseBuildDate("Aug 6 2026,13:46:54")?.getMinutes()).toBe(46);
  });

  it("is shown in the UI's language", () => {
    expect(buildDateText("Aug 6 2026,13:46:54", makeFormat("en"))).toBe("6 Aug 2026, 13:46");
    expect(buildDateText("Aug 6 2026,13:46:54", makeFormat("tr"))).not.toContain("Aug");
  });

  it("tidies a form it doesn't know", () => {
    expect(parseBuildDate("Foo 6 2026,13:46:54")).toBeNull();
    expect(parseBuildDate("Feb 31 2026,10:00:00")).toBeNull();
    expect(buildDateText("build  42,r7", makeFormat("en"))).toBe("build 42, r7");
  });
});
