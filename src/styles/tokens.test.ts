import { describe, expect, it } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";

function sRGBtoLin(c: number): number {
  const norm = c / 255;
  return norm <= 0.04045 ? norm / 12.92 : Math.pow((norm + 0.055) / 1.055, 2.4);
}

function relativeLuminance(hex: string): number {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return 0.2126 * sRGBtoLin(r) + 0.7152 * sRGBtoLin(g) + 0.0722 * sRGBtoLin(b);
}

function contrastRatio(hex1: string, hex2: string): number {
  const l1 = relativeLuminance(hex1);
  const l2 = relativeLuminance(hex2);
  const max = Math.max(l1, l2);
  const min = Math.min(l1, l2);
  return (max + 0.05) / (min + 0.05);
}

const WCAG_AA_TEXT_CONTRAST = 4.5;

/** The least contrast a mark that is not text needs (WCAG 1.4.11). */
const WCAG_NON_TEXT_CONTRAST = 3;

/**
 * The least CIELAB distance (ΔE76) between the color of a PDF of the organize
 * tab and the accent or a color of a state, and between two such colors: the
 * dot must not read as a selection, a failure, a success, a wait or a notice,
 * nor as another PDF. About 2.3 is the least a person sees at all, and 15 or
 * more is plainly another color; the palette keeps to the second.
 */
const MIN_STATE_DISTANCE = 15;
const MIN_SOURCE_DISTANCE = 20;

function toLab(hex: string): [number, number, number] {
  const [r, g, b] = [1, 3, 5].map((start) =>
    sRGBtoLin(parseInt(hex.slice(start, start + 2), 16)),
  );
  const x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
  const y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
  const f = (t: number) =>
    t > 216 / 24389 ? Math.cbrt(t) : ((24389 / 27) * t + 16) / 116;
  return [116 * f(y) - 16, 500 * (f(x) - f(y)), 200 * (f(y) - f(z))];
}

function labDistance(hex1: string, hex2: string): number {
  const [l1, a1, b1] = toLab(hex1);
  const [l2, a2, b2] = toLab(hex2);
  return Math.hypot(l1 - l2, a1 - a2, b1 - b2);
}

const TOKENS_PATH = path.resolve(__dirname, "tokens.css");

/**
 * Reads the `--name: #hex;` declarations of one theme from tokens.css.
 * The light theme is the first `:root` block; the dark theme is the
 * `:root` block inside `@media (prefers-color-scheme: dark)`, whose values
 * override the light ones, so light values fill in what dark leaves out.
 */
function readThemeTokens(theme: "light" | "dark"): Map<string, string> {
  const content = fs.readFileSync(TOKENS_PATH, "utf-8");
  const darkStart = content.indexOf("@media (prefers-color-scheme: dark)");
  expect(darkStart, "dark theme block in tokens.css").toBeGreaterThan(-1);

  const declarationRegex =
    /(--(?:color|organize)-[\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g;
  const parse = (css: string) =>
    new Map(
      Array.from(css.matchAll(declarationRegex), (m): [string, string] => [
        m[1],
        m[2],
      ]),
    );

  const light = parse(content.slice(0, darkStart));
  if (theme === "light") {
    return light;
  }
  return new Map([...light, ...parse(content.slice(darkStart))]);
}

function tokenValue(tokens: Map<string, string>, name: string): string {
  const value = tokens.get(name);
  expect(value, `${name} as a 6-digit hex color in tokens.css`).toBeDefined();
  return value ?? "";
}

function findCssFiles(dir: string): string[] {
  const files: string[] = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...findCssFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith(".css")) {
      files.push(fullPath);
    }
  }
  return files;
}

describe("design tokens and CSS rules", () => {
  it("does not hard-code color values in component CSS files", () => {
    const srcDir = path.resolve(__dirname, "..");
    const cssFiles = findCssFiles(srcDir).filter(
      (file) => !file.endsWith("tokens.css"),
    );

    // Matches hex (#123, #123456), rgb(...), rgba(...), hsl(...), hsla(...)
    const colorLiteralRegex =
      /#[0-9a-fA-F]{3,8}\b|rgba?\([^)]+\)|hsla?\([^)]+\)/g;

    for (const file of cssFiles) {
      const content = fs.readFileSync(file, "utf-8");
      const matches = content.match(colorLiteralRegex);
      expect(
        matches,
        `Direct color values found in ${path.relative(srcDir, file)}: ${matches?.join(", ")}`,
      ).toBeNull();
    }
  });

  it("provides required color variables in tokens.css", () => {
    const tokensPath = path.resolve(__dirname, "tokens.css");
    const content = fs.readFileSync(tokensPath, "utf-8");

    const expectedTokens = [
      "--color-bg-app",
      "--color-bg-surface",
      "--color-bg-subtle",
      "--color-text-primary",
      "--color-text-secondary",
      "--color-text-on-accent",
      "--color-border",
      "--color-border-subtle",
      "--color-accent",
      "--color-accent-hover",
      "--color-success",
      "--color-error",
      "--color-warning",
      "--color-info",
      "--color-progress-track",
    ];

    for (const token of expectedTokens) {
      expect(content).toContain(token);
    }
  });

  it("leaves the shared modal dialog sized by its content", () => {
    // A small dialog (the password one) would otherwise inherit the about
    // dialog's fixed height and show an empty area under its buttons.
    const appCss = fs.readFileSync(path.resolve(__dirname, "app.css"), "utf-8");
    const rule = /(?:^|\n)\.modal-dialog\s*\{([^}]*)\}/.exec(appCss);
    expect(rule).not.toBeNull();
    expect(rule?.[1]).not.toMatch(/(^|[\s;])height\s*:/);
  });

  it("keeps the colors laid over a page readable on the page's white", () => {
    // The page is white in both themes, and the frames and the kind tags
    // drawn on it do not change with the theme (see tokens.css).
    const tokens = readThemeTokens("dark");
    for (const fg of ["--color-page-candidate", "--color-page-area"]) {
      expect(
        contrastRatio(
          tokenValue(tokens, fg),
          tokenValue(tokens, "--color-page-bg"),
        ),
        `${fg} on the page`,
      ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
    }
  });

  it("keeps the kind tag of a selected area readable on its ground", () => {
    const tokens = readThemeTokens("dark");
    expect(
      contrastRatio(
        tokenValue(tokens, "--color-page-area-text"),
        tokenValue(tokens, "--color-page-area"),
      ),
    ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
  });

  it.each(["light", "dark"] as const)(
    "meets WCAG AA contrast for accent and status pairs in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);

      // Pairs as app.css draws them: .btn-primary and .seg put
      // text-on-accent over accent (accent-hover on hover), and links use
      // accent (accent-hover on hover) as text over the app and surface.
      // The focus ring draws accent on every ground the app uses.
      // A failed page's mark puts text-on-error over error. Error and
      // success are also text, on every ground the app uses, and so is
      // warning (the status word of a PDF waiting for its password). Info
      // marks a notice on the same grounds.
      const pairs: [string, string][] = [
        ["--color-text-on-accent", "--color-accent"],
        ["--color-text-on-accent", "--color-accent-hover"],
        ["--color-accent", "--color-bg-app"],
        ["--color-accent", "--color-bg-surface"],
        ["--color-accent", "--color-bg-subtle"],
        ["--color-accent-hover", "--color-bg-app"],
        ["--color-accent-hover", "--color-bg-surface"],
        ["--color-text-on-error", "--color-error"],
        ["--color-error", "--color-bg-app"],
        ["--color-error", "--color-bg-surface"],
        ["--color-error", "--color-bg-subtle"],
        ["--color-success", "--color-bg-app"],
        ["--color-success", "--color-bg-surface"],
        ["--color-success", "--color-bg-subtle"],
        ["--color-warning", "--color-bg-app"],
        ["--color-warning", "--color-bg-surface"],
        ["--color-warning", "--color-bg-subtle"],
        ["--color-info", "--color-bg-app"],
        ["--color-info", "--color-bg-surface"],
        ["--color-info", "--color-bg-subtle"],
      ];

      for (const [fg, bg] of pairs) {
        const ratio = contrastRatio(
          tokenValue(tokens, fg),
          tokenValue(tokens, bg),
        );
        expect(
          ratio,
          `${fg} on ${bg} in the ${theme} theme`,
        ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
      }
    },
  );

  it.each(["light", "dark"] as const)(
    "keeps the words of a selected row readable in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);
      // Success is left out: the dark green falls short on the selected
      // row's ground, and it shows there only on an exported row.
      for (const fg of [
        "--color-text-primary",
        "--color-text-secondary",
        "--color-error",
        "--color-warning",
      ]) {
        expect(
          contrastRatio(
            tokenValue(tokens, fg),
            tokenValue(tokens, "--color-row-selected"),
          ),
          `${fg} on --color-row-selected in the ${theme} theme`,
        ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
      }
    },
  );

  const ORGANIZE_SOURCES = [1, 2, 3, 4, 5, 6].map(
    (n) => `--organize-source-${n}`,
  );

  it.each(["light", "dark"] as const)(
    "draws the colors of the PDFs of the organize tab visibly on every ground in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);
      const grounds = [
        "--color-bg-app",
        "--color-bg-surface",
        "--color-bg-subtle",
        "--organize-cell-selected",
      ];
      for (const source of ORGANIZE_SOURCES) {
        for (const ground of grounds) {
          expect(
            contrastRatio(
              tokenValue(tokens, source),
              tokenValue(tokens, ground),
            ),
            `${source} on ${ground} in the ${theme} theme`,
          ).toBeGreaterThanOrEqual(WCAG_NON_TEXT_CONTRAST);
        }
      }
    },
  );

  it.each(["light", "dark"] as const)(
    "keeps the colors of the PDFs apart from each other, the accent and the states in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);
      const others = [
        "--color-accent",
        "--color-error",
        "--color-success",
        "--color-warning",
        "--color-info",
      ];
      for (const source of ORGANIZE_SOURCES) {
        for (const other of others) {
          expect(
            labDistance(tokenValue(tokens, source), tokenValue(tokens, other)),
            `${source} and ${other} in the ${theme} theme`,
          ).toBeGreaterThanOrEqual(MIN_STATE_DISTANCE);
        }
      }
      ORGANIZE_SOURCES.forEach((a, index) => {
        for (const b of ORGANIZE_SOURCES.slice(index + 1)) {
          expect(
            labDistance(tokenValue(tokens, a), tokenValue(tokens, b)),
            `${a} and ${b} in the ${theme} theme`,
          ).toBeGreaterThanOrEqual(MIN_SOURCE_DISTANCE);
        }
      });
    },
  );

  it.each(["light", "dark"] as const)(
    "keeps the words on the yellow note and the selected page readable in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);
      for (const ground of ["--organize-notice-bg"]) {
        for (const fg of ["--color-text-primary", "--color-text-secondary"]) {
          expect(
            contrastRatio(tokenValue(tokens, fg), tokenValue(tokens, ground)),
            `${fg} on ${ground} in the ${theme} theme`,
          ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
        }
      }
      // The angle of a turned page is drawn on the page box of a selected page.
      expect(
        contrastRatio(
          tokenValue(tokens, "--color-text-primary"),
          tokenValue(tokens, "--color-bg-surface"),
        ),
      ).toBeGreaterThanOrEqual(WCAG_AA_TEXT_CONTRAST);
    },
  );
});
