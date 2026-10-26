import type { ReactNode } from "react";
import { ChevronDownIcon } from "./ChevronDownIcon";
import "./SelectBox.css";

export interface SelectBoxProps {
  /** The `select`, which keeps its own label, value and handlers. */
  children: ReactNode;
}

/**
 * Draws the arrow of the `select` it wraps with `ChevronDownIcon`, so that it
 * matches the menu buttons' arrow and sits as far from the edge (design
 * §10.1). The WebView's own arrow cannot be moved off the right edge.
 */
export function SelectBox({ children }: SelectBoxProps) {
  return (
    <span className="select-box">
      {children}
      <ChevronDownIcon />
    </span>
  );
}
