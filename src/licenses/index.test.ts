import { describe, expect, it } from "vitest";
import { parseThirdPartyLicenses, thirdPartyLicenses } from ".";

describe("third-party licenses", () => {
  it("include pdfium and the 14 notices bundled with it", () => {
    const ids = thirdPartyLicenses.map((license) => license.id);
    expect(ids).toContain("pdfium-binaries");
    expect(ids.filter((id) => id.startsWith("pdfium/"))).toHaveLength(14);
  });

  it("include Rust crates and npm packages", () => {
    const ecosystems = new Set(
      thirdPartyLicenses.flatMap((license) =>
        license.packages.map((pkg) => pkg.ecosystem),
      ),
    );
    expect(ecosystems).toEqual(new Set(["cargo", "npm", "native"]));
  });

  it("rejects a list of the wrong shape", () => {
    expect(() => parseThirdPartyLicenses({})).toThrow();
    expect(() =>
      parseThirdPartyLicenses({
        licenses: [
          {
            id: "MIT",
            name: "MIT License",
            text: "",
            packages: [{ name: "a", version: "1", ecosystem: "pip" }],
          },
        ],
      }),
    ).toThrow();
  });
});
