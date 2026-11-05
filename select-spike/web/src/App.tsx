import { useEffect, useState } from "react";
import type { OcrPage } from "./core/ocr";

export const PAGES = ["h1_two_columns", "v2_two_tiers", "v4_dialogue", "m1_mixed"];
/** CSS px per pt at 100 % (pdf.js uses the same 96 / 72). */
export const PX_PER_PT = 96 / 72;

function param(name: string, dflt: string): string {
  return new URLSearchParams(location.search).get(name) ?? dflt;
}

export function App() {
  const [name, setName] = useState(param("page", PAGES[0]));
  const [zoom, setZoom] = useState(Number(param("zoom", "1")));
  const [page, setPage] = useState<OcrPage | null>(null);

  useEffect(() => {
    setPage(null);
    void fetch(`./${name}.json`)
      .then((r) => r.json() as Promise<OcrPage>)
      .then(setPage);
  }, [name]);

  const scale = zoom * PX_PER_PT;
  return (
    <div className="app">
      <header>
        <select value={name} onChange={(e) => setName(e.target.value)}>
          {PAGES.map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
        <button onClick={() => setZoom(1)}>100 %</button>
        <button onClick={() => setZoom(2)}>200 %</button>
      </header>
      <main>
        {page && (
          <div className="page" style={{ width: page.widthPt * scale, height: page.heightPt * scale }}>
            <img src={`./${name}.png`} alt="" draggable={false} />
          </div>
        )}
      </main>
    </div>
  );
}
