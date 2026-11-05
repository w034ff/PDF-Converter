import { useEffect, useState } from "react";
import type { OcrPage } from "./core/ocr";
import { CustomView } from "./custom/CustomView";
import { NativeView } from "./native/NativeView";
import { expose } from "./testHooks";

export const PAGES = ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"];
/** CSS px per pt at 100 % (pdf.js uses the same 96 / 72). */
export const PX_PER_PT = 96 / 72;

function param(name: string, dflt: string): string {
  return new URLSearchParams(location.search).get(name) ?? dflt;
}

export function App() {
  const [name, setName] = useState(param("page", PAGES[0]));
  const [zoom, setZoom] = useState(Number(param("zoom", "1")));
  const [mode, setMode] = useState(param("mode", "native"));
  const [fix, setFix] = useState(param("fix", "1") === "1");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState<OcrPage | null>(null);

  useEffect(() => {
    setPage(null);
    void fetch(`./${name}.json`)
      .then((r) => r.json() as Promise<OcrPage>)
      .then(setPage);
  }, [name]);

  const scale = zoom * PX_PER_PT;
  if (page) expose(page, scale);
  return (
    <div className="app">
      <header>
        <select value={name} onChange={(e) => setName(e.target.value)}>
          {PAGES.map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
        <select value={mode} onChange={(e) => setMode(e.target.value)}>
          <option value="native">ブラウザーの選択</option>
          <option value="custom">字の 4 点から自作</option>
        </select>
        <label>
          <input type="checkbox" checked={fix} onChange={(e) => setFix(e.target.checked)} /> pdf.js の工夫
        </label>
        <button onClick={() => setZoom(1)}>100 %</button>
        <button onClick={() => setZoom(2)}>200 %</button>
        <input id="search" placeholder="検索" value={query} onChange={(e) => setQuery(e.target.value)} />
      </header>
      <main id="ready" data-ready={page ? "1" : "0"}>
        {page && mode === "native" && <NativeView key={name} page={page} name={name} scale={scale} query={query} fix={fix} />}
        {page && mode === "custom" && <CustomView key={name} page={page} name={name} scale={scale} query={query} />}
      </main>
    </div>
  );
}
