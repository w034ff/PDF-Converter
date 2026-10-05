import { DropZone } from "../../components";
import { useAppState } from "../../state";
import { getTranslations } from "../../i18n";

/**
 * View area for PDF to Images (to be fully implemented in T12).
 * Renders DropZone in the empty state.
 */
export function PdfToImagesView() {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <DropZone
      title={t.dropZone.titlePdfs}
      description={t.dropZone.descriptionPdfs}
      addFilesLabel={t.dropZone.addPdfs}
      addFolderLabel={t.dropZone.addFolder}
    />
  );
}
