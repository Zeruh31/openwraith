// Fails when the production bundle contains browser-preview code (the mocked
// IPC, the fixtures, the scenario picker) or the simulator panel's commands,
// and when the third-party notices the installers ship are missing. Run
// after `vite build` and `scripts/third-party-notices.mjs`.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dist = fileURLToPath(new URL("../dist/", import.meta.url));
/** Strings that only the preview code (or the simulator panel, dev server only) contains. */
const forbidden = ["mockIPC", "mockWindows", "installMocks", "initial_status", "ow-preview-panel", "dev_sim_"];

/** Every file below `dir`. */
function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

let failed = false;
let count = 0;
for (const path of files(dist)) {
  if (/(mock|fixture|preview|scenario)/i.test(path.slice(dist.length))) {
    console.error(`dist contains a preview file: ${path}`);
    failed = true;
  }
  if (!/\.(js|css|html)$/.test(path)) continue;
  count += 1;
  const text = readFileSync(path, "utf8");
  for (const word of forbidden) {
    if (text.includes(word)) {
      console.error(`dist contains preview code: "${word}" in ${path}`);
      failed = true;
    }
  }
}
if (count === 0) {
  console.error("dist is empty: run `vite build` first");
  failed = true;
}

// The notices shipped next to LICENSE.txt (`bundle.resources`).
const notices = fileURLToPath(new URL("../src-tauri/legal/THIRD-PARTY-NOTICES.txt", import.meta.url));
const noticesText = existsSync(notices) ? readFileSync(notices, "utf8") : "";
if (noticesText.trim() === "" || noticesText.startsWith("OPENWRAITH-NOTICES-PLACEHOLDER")) {
  console.error(`${notices} is missing or a placeholder: run \`npm run build:notices\``);
  failed = true;
} else {
  for (const name of ["react", "@tauri-apps/api", "tauri ", "serde "]) {
    if (!noticesText.includes(name)) {
      console.error(`THIRD-PARTY-NOTICES.txt does not mention ${name.trim()}`);
      failed = true;
    }
  }
}
if (failed) process.exit(1);
console.log(`dist is free of preview code (${count} files checked); third-party notices present`);
