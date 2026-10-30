import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import * as fs from "node:fs";
import * as path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../../ipc";
import {
  DEFAULT_SETTINGS,
  addThroughMenu,
  callsOf,
  cellOf,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
} from "./organizeTestSupport";

beforeEach(() => {
  installFakeResizeObserver(700, 600);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

function savedSettings(
  calls: { cmd: string; args: Record<string, unknown> }[],
) {
  return callsOf(calls, "save_settings").map((call) => call.args);
}

const SAVED: Settings = {
  ...DEFAULT_SETTINGS,
  organize: {
    output: "split",
    encrypt: true,
    outputDir: { dirLabel: "parts" },
  },
};

describe("the settings of the organize tab", () => {
  it("start from what get_settings gave", async () => {
    await openOrganizeTab({ settings: SAVED });

    expect(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    ).toBeChecked();
  });

  it("start without a password when none was chosen", async () => {
    await openOrganizeTab();

    expect(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    ).not.toBeChecked();
  });

  it("are sent with save_settings when the password choice changes", async () => {
    const calls = await openOrganizeTab();

    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    await flush();

    const saved = savedSettings(calls);
    expect(saved).toHaveLength(1);
    expect(saved[0]).toMatchObject({
      revision: 1,
      settings: { organize: { output: "single", encrypt: true } },
    });
  });

  it("send the output as get_settings gave it, unchanged", async () => {
    const calls = await openOrganizeTab({ settings: SAVED });

    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    await flush();

    expect(savedSettings(calls)[0]).toMatchObject({
      settings: { organize: { output: "split", encrypt: false } },
    });
  });

  it("send nothing about the folder, which only pick_output_dir changes", async () => {
    const calls = await openOrganizeTab({ settings: SAVED });

    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    await flush();

    const settings = savedSettings(calls)[0].settings;
    expect(settings).toMatchObject({
      organize: { output: "split", encrypt: false },
    });
    expect(JSON.stringify(settings)).not.toMatch(/outputDir|parts/);
  });

  it("are sent again with a higher revision at the next change", async () => {
    const calls = await openOrganizeTab();

    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    await flush();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    await flush();

    expect(savedSettings(calls).map((call) => call.revision)).toEqual([1, 2]);
  });

  it("are not sent when a PDF is added or a page is turned or deleted", async () => {
    const calls = await openOrganizeTab({
      settings: SAVED,
      pdfs: [organizePdf(1, "a.pdf", 3)],
    });

    pressCell(cellOf(1, 1));
    releasePointer();
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    await addThroughMenu();

    expect(savedSettings(calls)).toEqual([]);
  });
});

describe("the note on the saved PDF", () => {
  it("is hidden in a window lower than 720 px", () => {
    const css = fs.readFileSync(
      path.resolve(__dirname, "organize.css"),
      "utf-8",
    );
    const rule =
      /@media \(max-height: (\d+)px\)\s*\{\s*\.organize-settings-about\s*\{\s*display: none;/.exec(
        css,
      );

    // "Lower than 720" is at most 719.
    expect(rule?.[1]).toBe("719");
  });
});
