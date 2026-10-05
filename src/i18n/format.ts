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
