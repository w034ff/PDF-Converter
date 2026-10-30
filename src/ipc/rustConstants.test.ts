import { describe, expect, it } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { OUTPUT_PASSWORD_MAX_LENGTH } from "../features/images-to-pdf/outputPassword";
import {
  MAX_PDF_PAGES,
  PAGE_VIEW_MAX_SIDE,
  RENDER_DPI_MAX,
  RENDER_DPI_MIN,
} from "../state";

/**
 * Constants of the front end that copy a Rust constant, because the screen
 * checks the value before it asks (a dpi field, a password field, the sizes
 * a page is requested at). The values are read from the Rust sources so that
 * changing one side without the other fails here. Every such copy is listed:
 * add a row where a new one is made.
 */
const REPOSITORY_ROOT = path.resolve(__dirname, "../..");

const RUST_COPIES: readonly {
  name: string;
  value: number;
  rustFile: string;
  rustName: string;
}[] = [
  {
    name: "PAGE_VIEW_MAX_SIDE",
    value: PAGE_VIEW_MAX_SIDE,
    rustFile: "crates/worker/src/lib.rs",
    rustName: "PAGE_VIEW_MAX_SIDE",
  },
  {
    name: "RENDER_DPI_MIN",
    value: RENDER_DPI_MIN,
    rustFile: "crates/worker/src/lib.rs",
    rustName: "RENDER_DPI_MIN",
  },
  {
    name: "RENDER_DPI_MAX",
    value: RENDER_DPI_MAX,
    rustFile: "crates/worker/src/lib.rs",
    rustName: "RENDER_DPI_MAX",
  },
  {
    name: "MAX_PDF_PAGES",
    value: MAX_PDF_PAGES,
    rustFile: "crates/worker/src/lib.rs",
    rustName: "MAX_PDF_PAGES",
  },
  {
    name: "OUTPUT_PASSWORD_MAX_LENGTH",
    value: OUTPUT_PASSWORD_MAX_LENGTH,
    rustFile: "crates/core/src/pdf_encrypt.rs",
    rustName: "MAX_OUTPUT_PASSWORD_LEN",
  },
];

/** The integer a `pub const NAME: type = 1_000;` of `source` is set to, or `null`. */
function readRustConstant(source: string, name: string): number | null {
  const declaration = new RegExp(
    `^pub const ${name}: [a-z0-9]+ = ([0-9_]+);$`,
    "m",
  ).exec(source);
  return declaration === null
    ? null
    : Number.parseInt(declaration[1].replaceAll("_", ""), 10);
}

describe("constants copied from Rust", () => {
  it.each(RUST_COPIES)(
    "$name is $rustName in $rustFile",
    ({ value, rustFile, rustName }) => {
      const source = fs.readFileSync(
        path.join(REPOSITORY_ROOT, rustFile),
        "utf-8",
      );
      expect(readRustConstant(source, rustName)).toBe(value);
    },
  );

  it("reads a constant the way Rust writes it", () => {
    const source =
      "pub const MAX_SIDE: u32 = 2_400;\nconst PRIVATE: u32 = 1;\n";
    expect(readRustConstant(source, "MAX_SIDE")).toBe(2400);
    expect(readRustConstant(source, "PRIVATE")).toBeNull();
  });
});
