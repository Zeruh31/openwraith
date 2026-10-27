// Entry point. In the browser preview (`vite --mode mock`) the Tauri IPC is
// replaced by recorded fixtures first; that branch is removed from
// production builds (`npm run build:check` verifies it).

import "./theme/tokens.css";
import "./theme/base.css";
import "./components/components.css";
import "./flow/flow.css";
import "./shell/shell.css";

import { StrictMode, type ReactNode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";

async function start(): Promise<void> {
  let devtools: ReactNode = null;
  if (import.meta.env.MODE === "mock") {
    const preview = await import("./preview/installMocks");
    devtools = preview.installMocks();
  }
  const root = document.getElementById("root");
  if (root === null) {
    return;
  }
  createRoot(root).render(
    <StrictMode>
      <App devtools={devtools} />
    </StrictMode>,
  );
}

void start();
