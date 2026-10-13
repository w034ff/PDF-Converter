import { useState } from "react";

/**
 * Whether a list that was empty earlier in this component's life now has
 * items. A list that already has items when the component mounts (switching
 * tabs) does not count, so the view only fades in when it replaces the drop
 * zone (design §10.1).
 *
 * The answer stays `true` while the list has items and resets once it is
 * empty again, so each refill counts anew.
 */
export function useFilledFromEmpty(isEmpty: boolean): boolean {
  const [wasEmpty, setWasEmpty] = useState(isEmpty);
  const [isFilledFromEmpty, setIsFilledFromEmpty] = useState(false);
  if (wasEmpty !== isEmpty) {
    setWasEmpty(isEmpty);
    setIsFilledFromEmpty(!isEmpty);
  }
  return isFilledFromEmpty;
}
