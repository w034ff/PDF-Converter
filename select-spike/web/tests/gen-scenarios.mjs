// Writes fixtures/scenarios/<page>.json from the page model (run once; Chromium).
import { createRequire } from "node:module";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
const { chromium } = createRequire(import.meta.url)("playwright");
const src = readFileSync(new URL("./scenarios.js", import.meta.url), "utf8").replace("export function", "function");
const out = new URL("../../fixtures/scenarios/", import.meta.url);
mkdirSync(out, { recursive: true });
const b = await chromium.launch();
const p = await b.newPage();
for (const pg of ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"]) {
  await p.goto(`http://localhost:5173/?page=${pg}`);
  await p.waitForFunction(() => window.__spike && document.querySelector("main").dataset.ready === "1");
  const sc = await p.evaluate(`${src}; buildScenarios()`);
  writeFileSync(new URL(`${pg}.json`, out), JSON.stringify(sc, null, 1));
  console.log(pg, sc.map((s) => s.name).join(" "));
}
await b.close();
