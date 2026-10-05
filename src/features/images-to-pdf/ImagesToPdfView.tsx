import { DropZone } from "../../components";
import { useAppState } from "../../state";
import { getTranslations } from "../../i18n";

/**
 * View area for Images to PDF (to be fully implemented in T11).
 * Renders DropZone in the empty state according to Main.dc.html.
 */
export function ImagesToPdfView() {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <DropZone
      title={t.dropZone.titleImages}
      description={t.dropZone.descriptionImages}
      addFilesLabel={t.dropZone.addImages}
      addFolderLabel={t.dropZone.addFolder}
    />
  );
}
