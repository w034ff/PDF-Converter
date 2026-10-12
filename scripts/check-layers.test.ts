import { describe, expect, it } from "vitest";
import { checkLayers } from "./check-layers.js";

describe("checkLayers", () => {
  it("reports no violations when pdfconv-core and pdfconv-worker do not reach Tauri", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-image", name: "image" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-core",
            deps: [
              {
                name: "image",
                pkg: "pkg-image",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-worker",
            deps: [],
          },
          {
            id: "pkg-image",
            deps: [],
          },
        ],
      },
    };

    expect(checkLayers(metadata)).toEqual([]);
  });

  it("reports a violation when reaching tauri transitively with intermediate package in path", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-foo", name: "foo" },
        { id: "pkg-tauri", name: "tauri" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-core",
            deps: [
              {
                name: "foo",
                pkg: "pkg-foo",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-worker",
            deps: [],
          },
          {
            id: "pkg-foo",
            deps: [
              {
                name: "tauri",
                pkg: "pkg-tauri",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-tauri",
            deps: [],
          },
        ],
      },
    };

    const violations = checkLayers(metadata);
    expect(violations).toEqual(["pdfconv-core -> foo -> tauri"]);
    expect(violations[0]).toContain("foo");
  });

  it("ignores dev-dependencies when checking worker dependencies", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-tauri", name: "tauri" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-core",
            deps: [],
          },
          {
            id: "pkg-worker",
            deps: [
              {
                name: "tauri",
                pkg: "pkg-tauri",
                dep_kinds: [{ kind: "dev" }],
              },
            ],
          },
          {
            id: "pkg-tauri",
            deps: [],
          },
        ],
      },
    };

    expect(checkLayers(metadata)).toEqual([]);
  });

  it("reports a violation when reaching a package starting with tauri-", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-dialog", name: "tauri-plugin-dialog" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-core",
            deps: [
              {
                name: "tauri_plugin_dialog",
                pkg: "pkg-dialog",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-worker",
            deps: [],
          },
          {
            id: "pkg-dialog",
            deps: [],
          },
        ],
      },
    };

    expect(checkLayers(metadata)).toEqual([
      "pdfconv-core -> tauri-plugin-dialog",
    ]);
  });

  it("uses the package name rather than the library name in the violation path", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-foo-bar", name: "foo-bar" },
        { id: "pkg-tauri", name: "tauri" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-core",
            deps: [
              {
                name: "foo_bar",
                pkg: "pkg-foo-bar",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-worker",
            deps: [],
          },
          {
            id: "pkg-foo-bar",
            deps: [
              {
                name: "tauri",
                pkg: "pkg-tauri",
                dep_kinds: [{ kind: null }],
              },
            ],
          },
          {
            id: "pkg-tauri",
            deps: [],
          },
        ],
      },
    };

    const violations = checkLayers(metadata);
    expect(violations).toEqual(["pdfconv-core -> foo-bar -> tauri"]);
    expect(violations[0]).toContain("foo-bar");
    expect(violations[0]).not.toContain("foo_bar");
  });

  it("throws an error when a root package like pdfconv-core is missing", () => {
    const metadata = {
      packages: [
        { id: "pkg-worker", name: "pdfconv-worker" },
        { id: "pkg-image", name: "image" },
      ],
      resolve: {
        nodes: [
          {
            id: "pkg-worker",
            deps: [],
          },
          {
            id: "pkg-image",
            deps: [],
          },
        ],
      },
    };

    expect(() => checkLayers(metadata)).toThrow("pdfconv-core");
  });

  it("throws an error when resolve is null", () => {
    const metadata = {
      packages: [
        { id: "pkg-core", name: "pdfconv-core" },
        { id: "pkg-worker", name: "pdfconv-worker" },
      ],
      resolve: null,
    };

    expect(() => checkLayers(metadata)).toThrow();
  });
});
