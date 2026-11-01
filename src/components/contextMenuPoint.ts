/**
 * Where a `ContextMenu` opens for `event`, on `origin`: at the pointer, or at
 * a corner of `origin` for Shift+F10 and the menu key, which say where the
 * focus is rather than the pointer.
 */
export function contextMenuPoint(
  event: { clientX: number; clientY: number },
  origin: Element,
): { x: number; y: number } {
  const box = origin.getBoundingClientRect();
  const inside =
    event.clientX >= box.left &&
    event.clientX <= box.right &&
    event.clientY >= box.top &&
    event.clientY <= box.bottom;
  return {
    x: inside ? event.clientX : box.left,
    y: inside ? event.clientY : box.top,
  };
}
