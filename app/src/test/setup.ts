// Test setup (jsdom): cleanup after each test and small polyfills for what
// jsdom lacks (modal dialogs, matchMedia, ResizeObserver).

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});

const dialogProto = HTMLDialogElement.prototype as unknown as {
  showModal?: unknown;
  close?: unknown;
};
if (typeof dialogProto.showModal !== "function") {
  dialogProto.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
}
if (typeof dialogProto.close !== "function") {
  dialogProto.close = function close(this: HTMLDialogElement) {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
}

const win = window as unknown as { matchMedia?: unknown; ResizeObserver?: unknown };
if (typeof win.matchMedia !== "function") {
  win.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  });
}
if (typeof win.ResizeObserver !== "function") {
  win.ResizeObserver = class {
    observe(): void {
      // jsdom has no layout.
    }
    unobserve(): void {
      // jsdom has no layout.
    }
    disconnect(): void {
      // jsdom has no layout.
    }
  };
}
