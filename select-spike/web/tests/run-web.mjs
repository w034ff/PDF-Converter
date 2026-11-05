// Replays fixtures/scenarios/*.json on the HTML versions and compares the copied text.
//   node tests/run-web.mjs <chromium|webkitgtk> <native|native-nofix|custom> [zoom] [shots dir]
// webkitgtk needs Xvfb on $DISPLAY and WebKitWebDriver on :4444 (see README).
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
const [browser, modeArg, zoomArg = "1", shots] = process.argv.slice(2);
const zoom = Number(zoomArg);
const mode = modeArg.startsWith("native") ? "native" : "custom";
const fix = modeArg === "native-nofix" ? "0" : "1";
const PAGES = ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"];
const SENTINEL = "<empty>";
const W = zoom > 1 ? 2300 : 1700, H = zoom > 1 ? 2500 : 1300;

async function chromiumDriver() {
  const { chromium } = createRequire(import.meta.url)("playwright");
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: W, height: H } });
  await ctx.grantPermissions(["clipboard-read", "clipboard-write"]);
  const p = await ctx.newPage();
  return {
    goto: (u) => p.goto(u),
    eval: (js) => p.evaluate(js),
    move: (x, y) => p.mouse.move(x, y),
    down: () => p.mouse.down(),
    up: () => p.mouse.up(),
    keyDown: (k) => p.keyboard.down(k === "ctrl" ? "Control" : k === "shift" ? "Shift" : k),
    keyUp: (k) => p.keyboard.up(k === "ctrl" ? "Control" : k === "shift" ? "Shift" : k),
    clearClip: () => p.evaluate((s) => navigator.clipboard.writeText(s), SENTINEL),
    readClip: () => p.evaluate(() => navigator.clipboard.readText()),
    shot: (path) => p.screenshot({ path }),
    close: () => b.close(),
  };
}

async function webkitDriver() {
  const base = "http://localhost:4444";
  const call = async (method, path, body) => {
    const r = await fetch(base + path, { method, headers: { "content-type": "application/json" }, body: body && JSON.stringify(body) });
    const j = await r.json();
    if (j.value && j.value.error) throw new Error(`${path}: ${JSON.stringify(j.value).slice(0, 300)} ${body ? JSON.stringify(body).slice(0, 300) : ""}`);
    return j.value;
  };
  const s = await call("POST", "/session", { capabilities: { alwaysMatch: { browserName: "MiniBrowser", "webkitgtk:browserOptions": { binary: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser", args: ["--automation"] } } } });
  const S = `/session/${s.sessionId}`;
  await call("POST", `${S}/window/rect`, { x: 0, y: 0, width: W, height: H + 80 });
  // WebKitWebDriver now and then answers an action with an empty error; one retry clears it.
  const ptr = (actions) => {
    const body = { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions }] };
    return call("POST", `${S}/actions`, body).catch(async (e) => {
      console.log("   retry after", e.message);
      await new Promise((r) => setTimeout(r, 300));
      return call("POST", `${S}/actions`, body);
    });
  };
  const key = (type, k) => call("POST", `${S}/actions`, { actions: [{ type: "key", id: "kbd", actions: [{ type, value: k === "ctrl" ? "" : k === "shift" ? "" : k }] }] });
  // xclip -i forks a process that serves the selection; it must not hold our stdout open.
  const clip = (input) =>
    input === undefined
      ? execFileSync("xclip", ["-o", "-selection", "clipboard"], { encoding: "utf8" })
      : execFileSync("xclip", ["-i", "-selection", "clipboard"], { input, stdio: ["pipe", "ignore", "ignore"] });
  return {
    goto: (u) => call("POST", `${S}/url`, { url: u }),
    eval: (js) => call("POST", `${S}/execute/sync`, { script: `return (${js})`, args: [] }),
    move: (x, y) => ptr([{ type: "pointerMove", duration: 0, origin: "viewport", x: Math.round(x), y: Math.round(y) }]),
    down: () => ptr([{ type: "pointerDown", button: 0 }]),
    up: () => ptr([{ type: "pointerUp", button: 0 }]),
    keyDown: (k) => key("keyDown", k),
    keyUp: (k) => key("keyUp", k),
    clearClip: async () => clip(SENTINEL),
    readClip: async () => { try { return clip(); } catch { return SENTINEL; } },
    shot: async (path) => writeFileSync(path, Buffer.from(await call("GET", `${S}/screenshot`), "base64")),
    close: () => call("DELETE", S),
  };
}

const d = browser === "chromium" ? await chromiumDriver() : await webkitDriver();
let pass = 0, fail = 0, nearN = 0;
function closeTo(a, b) {
  const A = [...a], B = [...b];
  let i = 0;
  while (i < A.length && i < B.length && A[i] === B[i]) i++;
  let j = 0;
  while (j < A.length - i && j < B.length - i && A[A.length - 1 - j] === B[B.length - 1 - j]) j++;
  return Math.max(A.length, B.length) - i - j <= 4 && Math.abs(A.length - B.length) <= 4;
}
const results = [];
for (const pg of PAGES) {
  const scenarios = JSON.parse(readFileSync(new URL(`../../fixtures/scenarios/${pg}.json`, import.meta.url), "utf8"));
  for (const sc of scenarios) {
    await d.goto(`http://localhost:5173/?page=${pg}&mode=${mode}&fix=${fix}&zoom=${zoom}`);
    for (let i = 0; i < 100; i++) {
      if (await d.eval(`!!(window.__spike && document.querySelector("main").dataset.ready === "1" && document.querySelector(".page img").complete && document.fonts.status === "loaded")`)) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    await new Promise((r) => setTimeout(r, 150));
    const box = await d.eval(`(() => { const r = document.querySelector(".page").getBoundingClientRect(); return [r.left, r.top, window.__spike.scale]; })()`);
    const toClient = ([x, y]) => [box[0] + x * box[2], box[1] + y * box[2]];
    const panelPoint = (line, ch) => d.eval(`(() => {
      const sp = document.querySelector('.vpanel [data-pl="${line}"][data-pc="${ch}"]');
      let r;
      if (sp) r = sp.getBoundingClientRect();
      else {
        const div = document.querySelectorAll(".vtext > div")[${line}];
        const t = div.firstChild, range = document.createRange();
        const idx = [...t.data].slice(0, ${ch}).join("").length;
        range.setStart(t, idx); range.setEnd(t, idx + [...t.data][${ch}].length);
        r = range.getBoundingClientRect();
      }
      return [r.left + r.width / 2, r.top + r.height * 0.3];
    })()`);
    await d.clearClip();
    for (const st of sc.steps) {
      if (st.op === "drag") {
        const pts = st.pts.map(toClient);
        await d.move(...pts[0]);
        await d.down();
        for (const p of pts.slice(1)) await d.move(...p);
        await d.up();
        await new Promise((r) => setTimeout(r, 80));
      } else if (st.op === "click") {
        if (st.shift) await d.keyDown("shift");
        await d.move(...toClient(st.pt));
        await d.down();
        await d.up();
        if (st.shift) await d.keyUp("shift");
        await new Promise((r) => setTimeout(r, 80));
      } else if (st.op === "key") {
        const k = st.key.split("+")[1];
        await d.keyDown("ctrl");
        await d.keyDown(k);
        await d.keyUp(k);
        await d.keyUp("ctrl");
        await new Promise((r) => setTimeout(r, 80));
      } else if (st.op === "search") {
        await d.eval(`(() => { const el = document.getElementById("search"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, ${JSON.stringify(st.q)}); el.dispatchEvent(new Event("input", { bubbles: true })); return 1; })()`);
        await new Promise((r) => setTimeout(r, 100));
      } else if (st.op === "panel-click") {
        await d.move(...(await panelPoint(st.line, st.char)));
        await d.down();
        await d.up();
      } else if (st.op === "panel-drag") {
        const a = await panelPoint(...st.from);
        const b = await panelPoint(...st.to);
        // Start at the top of the first glyph and end at the bottom of the last one.
        await d.move(a[0], a[1] - 4);
        await d.down();
        for (let i = 1; i <= 8; i++) await d.move(a[0] + (b[0] - a[0]) * i / 8, a[1] - 4 + (b[1] + 14 - a[1]) * i / 8);
        await d.up();
      }
    }
    let ok, got;
    if (sc.rects) {
      got = await d.eval(`[...document.querySelectorAll(".bands.hit > div")].map((e) => [e.offsetLeft, e.offsetTop, e.offsetWidth, e.offsetHeight].map((v) => v / window.__spike.scale))`);
      const want = sc.rects.map((r) => [r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0]);
      ok = got.length === want.length && got.every((g, i) => g.every((v, j) => Math.abs(v - want[i][j]) < 1));
      got = JSON.stringify(got.map((g) => g.map((v) => +v.toFixed(1))));
    } else {
      got = (await d.readClip()).replace(/\r\n/g, "\n");
      // Copying an empty selection leaves the clipboard alone.
      if (got === SENTINEL && sc.copy === "") got = "";
      ok = got === sc.copy;
    }
    if (shots) {
      mkdirSync(shots, { recursive: true });
      await d.shot(`${shots}/${pg}-${sc.name}.png`);
    }
    // "near": the same text but for at most 2 characters at either end (a caret placed one glyph off).
    const near = !ok && !sc.rects && got !== SENTINEL && closeTo(got, sc.copy);
    ok ? pass++ : near ? nearN++ : fail++;
    results.push({ page: pg, name: sc.name, ok, near, got: ok ? undefined : got, want: ok ? undefined : sc.copy ?? sc.query });
    console.log(`${ok ? "PASS" : near ? "NEAR" : "FAIL"} ${pg} ${sc.name}`);
    if (!ok) console.log(`   want ${JSON.stringify((sc.copy ?? "").slice(0, 120))}\n   got  ${JSON.stringify(got.slice(0, 120))}`);
  }
}
await d.close();
console.log(`${browser} ${modeArg} zoom ${zoom}: ${pass} pass, ${nearN} near, ${fail} fail`);
writeFileSync(`/tmp/claude-0/result-${browser}-${modeArg}-${zoom}.json`, JSON.stringify(results, null, 1));
