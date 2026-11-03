/** KB and MB on screen are of 1,024 bytes, as the OS file managers count them. */
const BYTES_PER_KB = 1024;
const BYTES_PER_MB = BYTES_PER_KB * BYTES_PER_KB;

/**
 * Fills each `{key}` in `template` with `values[key]` (design §10.2, SVG
 * Tracer §8.4). A placeholder without a value is left as it is, so a missing
 * value shows up on screen instead of disappearing.
 */
export function formatMessage(
  template: string,
  values: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (placeholder, key: string) =>
    Object.hasOwn(values, key) ? String(values[key]) : placeholder,
  );
}

/** `count` with the template for one or for several. */
export function pluralize(
  count: number,
  single: string,
  multiple: string,
): string {
  return formatMessage(count === 1 ? single : multiple, { count });
}

/**
 * A file size in whole bytes or KB, or in MB to one decimal place, rounded
 * to the nearest. A size that rounds to 1,024 KB reads as MB.
 */
export function formatByteSize(bytes: number): string {
  if (bytes < BYTES_PER_KB) {
    return `${bytes} B`;
  }
  const kb = Math.round(bytes / BYTES_PER_KB);
  if (kb < BYTES_PER_KB) {
    return `${kb} KB`;
  }
  return `${(bytes / BYTES_PER_MB).toFixed(1)} MB`;
}
