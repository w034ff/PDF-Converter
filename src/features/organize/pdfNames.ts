/** The name of a PDF's file without its extension, as the caption of a page shows it. */
export function fileStem(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}
