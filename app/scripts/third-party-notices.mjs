// Writes `src-tauri/legal/THIRD-PARTY-NOTICES.txt`, which the installers ship
// next to LICENSE.txt (`bundle.resources`):
//
// - every Rust crate linked into the app (the `openwraith-app` package's
//   normal dependencies for the host target, from `cargo metadata`), with the
//   license files the crate ships (LICENSE*, COPYING*, NOTICE*), or, for a
//   crate that ships none, its SPDX expression, authors and a standard text;
// - where the source of the MPL-2.0 crates is (MPL-2.0 section 3.2);
// - the frontend packages bundled into the app (`dist/.vite/license.md`,
//   written by `vite build` with `build.license`).
//
// Identical license texts are printed once, with the components that use
// them. Run after `vite build` (part of `npm run build:web`). Fails when
// anything is missing, so a release is never built without its notices.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const app = fileURLToPath(new URL("../", import.meta.url));
const out = join(app, "src-tauri", "legal", "THIRD-PARTY-NOTICES.txt");
const frontendLicenses = join(app, "dist", ".vite", "license.md");
const cargo = process.env.CARGO ?? "cargo";

/** Fails the build with a message. */
function fail(message) {
  console.error(`third-party notices: ${message}`);
  process.exit(1);
}

/** The host target triple (the installers are built for it). */
function hostTriple() {
  const info = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const host = /^host: (.+)$/m.exec(info);
  if (!host) fail("cannot read the host target from `rustc -vV`");
  return host[1].trim();
}

/** `cargo metadata` of the workspace, resolved for `target`. */
function metadata(target) {
  const text = execFileSync(
    cargo,
    [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--filter-platform",
      target,
      "--manifest-path",
      join(app, "src-tauri", "Cargo.toml"),
    ],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  return JSON.parse(text);
}

/** The packages linked into `openwraith-app` (normal dependencies only). */
function linkedPackages(meta) {
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const root = meta.packages.find((p) => p.name === "openwraith-app" && p.source === null);
  if (!root) fail("the openwraith-app package is not in the workspace");
  const seen = new Set();
  const stack = [root.id];
  while (stack.length > 0) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  // Workspace members are OpenWraith itself (LICENSE.txt).
  return [...seen]
    .map((id) => byId.get(id))
    .filter((p) => p !== undefined && p.source !== null)
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

/** The license files a crate ships, in its manifest directory. */
function licenseFiles(pkg) {
  const dir = dirname(pkg.manifest_path);
  const names = readdirSync(dir).filter(
    (name) => /^(licen[cs]e|copying|notice|unlicense)/i.test(name) && statSync(join(dir, name)).isFile(),
  );
  if (pkg.license_file) {
    const declared = pkg.license_file.replace(/^\.\//, "");
    if (!names.includes(declared) && existsSync(join(dir, declared))) names.push(declared);
  }
  return names.sort().map((name) => ({ name, text: readFileSync(join(dir, name), "utf8") }));
}

/** Normalised text (line endings, trailing spaces) for de-duplication. */
function normalise(text) {
  return text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n")
    .trim();
}

/** A standard text for a crate that ships no license file. */
function fallbackText(pkg) {
  const authors = pkg.authors.length > 0 ? pkg.authors.join(", ") : `the ${pkg.name} authors`;
  const license = pkg.license ?? "unknown";
  if (/^MIT$/.test(license)) {
    return [
      `Copyright (c) ${authors}`,
      "",
      "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and",
      'associated documentation files (the "Software"), to deal in the Software without restriction,',
      "including without limitation the rights to use, copy, modify, merge, publish, distribute,",
      "sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is",
      "furnished to do so, subject to the following conditions:",
      "",
      "The above copyright notice and this permission notice shall be included in all copies or",
      "substantial portions of the Software.",
      "",
      'THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT',
      "NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND",
      "NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES",
      "OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN",
      "CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.",
    ].join("\n");
  }
  if (/^BSD-3-Clause$/.test(license)) {
    return [
      `Copyright (c) ${authors}`,
      "",
      "Redistribution and use in source and binary forms, with or without modification, are permitted",
      "provided that the following conditions are met:",
      "",
      "1. Redistributions of source code must retain the above copyright notice, this list of conditions",
      "   and the following disclaimer.",
      "2. Redistributions in binary form must reproduce the above copyright notice, this list of",
      "   conditions and the following disclaimer in the documentation and/or other materials provided",
      "   with the distribution.",
      "3. Neither the name of the copyright holder nor the names of its contributors may be used to",
      "   endorse or promote products derived from this software without specific prior written",
      "   permission.",
      "",
      'THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR',
      "IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND",
      "FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR",
      "CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL",
      "DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,",
      "DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER",
      "IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT",
      "OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.",
    ].join("\n");
  }
  return [
    `${pkg.name} ${pkg.version} is licensed under ${license}; the crate ships no license file.`,
    `Authors: ${authors}.`,
    /MPL-2\.0/.test(license)
      ? "The full text of the Mozilla Public License 2.0 is reproduced in this file (see the other " +
        "MPL-2.0 components) and at https://mozilla.org/MPL/2.0/."
      : `The license text is at https://spdx.org/licenses/${encodeURIComponent(license)}.html.`,
  ].join("\n");
}

const version = JSON.parse(readFileSync(join(app, "package.json"), "utf8")).version;
if (!existsSync(frontendLicenses) || statSync(frontendLicenses).size === 0) {
  fail(`${frontendLicenses} is missing: run \`vite build\` (with build.license) first`);
}
const target = hostTriple();
const crates = linkedPackages(metadata(target));
if (crates.length === 0) fail("no linked crates found");

/** Unique license texts -> the components that use them. */
const texts = new Map();
const addText = (text, user) => {
  const key = normalise(text);
  const entry = texts.get(key) ?? { text: key, users: [] };
  entry.users.push(user);
  texts.set(key, entry);
};

const listing = [];
const mpl = [];
for (const pkg of crates) {
  const user = `${pkg.name} ${pkg.version}`;
  const source = pkg.repository ?? `https://crates.io/crates/${pkg.name}/${pkg.version}`;
  listing.push(`- ${user}: ${pkg.license ?? "license not stated"} (${source})`);
  if (/MPL-2\.0/.test(pkg.license ?? "")) {
    mpl.push(
      `- ${user}: https://crates.io/crates/${pkg.name}/${pkg.version}` +
        (pkg.repository ? ` (repository: ${pkg.repository})` : ""),
    );
  }
  const files = licenseFiles(pkg);
  if (files.length === 0) {
    addText(fallbackText(pkg), `${user} (no license file in the crate)`);
  } else {
    for (const file of files) addText(file.text, `${user} (${file.name})`);
  }
}

const frontend = readFileSync(frontendLicenses, "utf8").replace(/\r\n?/g, "\n").trim();
const rule = (title) => `${title}\n${"=".repeat(title.length)}`;
const parts = [
  rule(`OpenWraith ${version}: third-party notices`),
  "",
  "OpenWraith is free software under the GNU General Public License, version 3 or later",
  "(LICENSE.txt). It includes the third-party components listed below, each under its own",
  "license. Their copyright notices and license texts follow.",
  "",
  rule("Source code of the MPL-2.0 components"),
  "",
  "These components are covered by the Mozilla Public License 2.0. Their source code is",
  "available at:",
  "",
  ...(mpl.length > 0 ? mpl : ["- (none)"]),
  "",
  rule(`Rust crates linked into the app (${crates.length}, target ${target})`),
  "",
  ...listing,
  "",
  rule("Frontend packages bundled into the app"),
  "",
  frontend,
  "",
  rule(`License texts and copyright notices of the Rust crates (${texts.size})`),
];
let n = 0;
for (const entry of texts.values()) {
  n += 1;
  parts.push("", `--- Text ${n}, used by: ${entry.users.join(", ")}`, "", entry.text);
}
parts.push("");

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, parts.join("\n"), "utf8");
console.log(
  `third-party notices: ${crates.length} crates, ${texts.size} distinct texts, ` +
    `${mpl.length} MPL-2.0 crates -> ${out}`,
);
