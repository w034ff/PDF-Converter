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

  const declarationRegex = /(--color-[\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g;
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
      "--color-progress-track",
    ];

    for (const token of expectedTokens) {
      expect(content).toContain(token);
    }
  });

  it.each(["light", "dark"] as const)(
    "meets WCAG AA contrast for accent pairs in the %s theme",
    (theme) => {
      const tokens = readThemeTokens(theme);

      // Pairs as app.css draws them: .btn-primary and .seg put
      // text-on-accent over accent (accent-hover on hover), and links use
      // accent (accent-hover on hover) as text over the app and surface.
      const pairs: [string, string][] = [
        ["--color-text-on-accent", "--color-accent"],
        ["--color-text-on-accent", "--color-accent-hover"],
        ["--color-accent", "--color-bg-app"],
        ["--color-accent", "--color-bg-surface"],
        ["--color-accent-hover", "--color-bg-app"],
        ["--color-accent-hover", "--color-bg-surface"],
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
});
