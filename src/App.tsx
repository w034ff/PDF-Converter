import { useEffect, useState } from "react";
import { getAbout, type AboutInfo } from "./ipc/about";

// Until the screens (T10–T12) and the about dialog (T13) exist, the window
// shows whether the bundled pdfium works, so installers can be checked (T01).
export function App() {
  const [about, setAbout] = useState<AboutInfo | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    getAbout().then(setAbout, (e: unknown) => setFailure(String(e)));
  }, []);

  return (
    <main>
      <h1>PDF Converter</h1>
      {about && (
        <p data-testid="pdfium-status">
          {about.pdfiumReady
            ? `pdfium ${about.pdfiumVersion}: OK (app ${about.version})`
            : `pdfium ${about.pdfiumVersion}: ${about.pdfiumError ?? ""}`}
        </p>
      )}
      {failure && <p data-testid="pdfium-status">{failure}</p>}
    </main>
  );
}
