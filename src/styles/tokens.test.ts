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

  it("meets contrast ratio requirements for terracotta accent colors", () => {
    // Light mode: terracotta #b4492b vs white #ffffff and app bg #f4f3f0
    const lightAccent = "#b4492b";
    const lightBgApp = "#f4f3f0";
    const white = "#ffffff";

    const lightVsWhite = contrastRatio(lightAccent, white);
    const lightVsBg = contrastRatio(lightAccent, lightBgApp);
    expect(lightVsWhite).toBeGreaterThanOrEqual(4.5);
    expect(lightVsBg).toBeGreaterThanOrEqual(4.5);

    // Dark mode: terracotta #e07353
    const darkAccent = "#e07353";
    const darkBgSurface = "#262522";
    const darkBgApp = "#1c1b18";
    const darkTextOnAccent = "#1c1b18";

    const darkVsSurface = contrastRatio(darkAccent, darkBgSurface);
    const darkVsApp = contrastRatio(darkAccent, darkBgApp);
    const darkVsText = contrastRatio(darkAccent, darkTextOnAccent);
    // Over dark background, accent contrast ratio must be >= 4.5
    expect(darkVsSurface).toBeGreaterThanOrEqual(4.5);
    expect(darkVsApp).toBeGreaterThanOrEqual(4.5);
    // On dark accent button, text contrast ratio must be >= 4.5
    expect(darkVsText).toBeGreaterThanOrEqual(4.5);
  });
});
