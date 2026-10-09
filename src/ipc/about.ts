import { invoke } from "@tauri-apps/api/core";
import type { AboutInfo } from "./generated/AboutInfo";

export type { AboutInfo };

function isAboutInfo(value: unknown): value is AboutInfo {
  if (typeof value !== "object" || value === null) return false;
  const v: Record<string, unknown> = { ...value };
  return (
    typeof v.version === "string" &&
    typeof v.pdfiumReady === "boolean" &&
    (typeof v.pdfiumError === "string" || v.pdfiumError === null)
  );
}

/** Asks Rust for the versions and whether a worker can load pdfium. */
export async function getAbout(): Promise<AboutInfo> {
  const value: unknown = await invoke("get_about");
  if (!isAboutInfo(value)) {
    throw new Error("get_about returned an unexpected value");
  }
  return value;
}
