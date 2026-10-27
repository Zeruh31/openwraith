// The frontend's command list equals the Rust single source
// (`src-tauri/src/command_names.rs`), and `api` has one function per command.

import { describe, expect, it } from "vitest";

import rust from "../../src-tauri/src/command_names.rs?raw";
import { api, COMMAND_NAMES, DEV_COMMAND_NAMES, devApi } from "../ipc/commands";

function rustList(name: string): string[] {
  const start = rust.indexOf(`pub const ${name}: &[&str] = &[`);
  expect(start).toBeGreaterThanOrEqual(0);
  const body = rust.slice(start, rust.indexOf("];", start));
  return [...body.matchAll(/"([a-z_]+)"/g)].map((m) => m[1] ?? "");
}

function camel(name: string): string {
  return name.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
}

describe("commands", () => {
  it("match command_names.rs", () => {
    expect([...COMMAND_NAMES]).toEqual(rustList("COMMANDS"));
    expect([...DEV_COMMAND_NAMES]).toEqual(rustList("DEV_COMMANDS"));
  });

  it("each have exactly one api function", () => {
    expect(Object.keys(api).sort()).toEqual([...COMMAND_NAMES].map(camel).sort());
    expect(Object.keys(devApi).sort()).toEqual([...DEV_COMMAND_NAMES].map(camel).sort());
  });
});
