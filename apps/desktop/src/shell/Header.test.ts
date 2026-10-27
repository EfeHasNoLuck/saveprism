// @vitest-environment node
import i18next from "i18next";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeAll, describe, expect, it } from "vitest";
import type { OpenInfo } from "@/api/commands";
import { initI18n } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { Header } from "./Header";
import { NotifyProvider } from "./NotifyProvider";
import { SettingsProvider } from "./SettingsProvider";

const noop = () => undefined;

function header(file: OpenInfo | null): string {
  const bar = createElement(Header, {
    file,
    onCloseFile: noop,
    onWhereSaves: noop,
    onOpenSettings: noop,
  });
  return renderToStaticMarkup(
    createElement(
      ThemeProvider,
      null,
      createElement(SettingsProvider, null, createElement(NotifyProvider, null, bar)),
    ),
  );
}

const FILE = {
  fileName: "Save1.json",
  path: "C:/Games/Save1.json",
  codec: "format.json",
  engine: null,
  pasted: false,
} as unknown as OpenInfo;

beforeAll(async () => {
  initI18n();
  await i18next.changeLanguage("en");
});

describe("Header", () => {
  it("keeps the brand, language menu and settings on the start screen", () => {
    const html = header(null);
    expect(html).toContain("SavePrism");
    expect(html).toContain("Language");
    expect(html).toContain("Settings");
    expect(html).toContain("EN");
  });

  it("removes Paste code, Open file and the theme toggle from the start screen", () => {
    const html = header(null);
    expect(html).not.toContain("Paste code");
    expect(html).not.toContain("Open file");
    expect(html).not.toContain("Switch to dark theme");
    expect(html).not.toContain("Switch to light theme");
    expect(html).not.toContain("Close file");
  });

  it("shows Close file and Where is my save while a file is open", () => {
    const html = header(FILE);
    expect(html).toContain("Close file");
    expect(html).toContain("Where is my save?");
    expect(html).not.toContain("Paste code");
  });

  it("shows Close file for a pasted code too", () => {
    const html = header({ ...FILE, pasted: true });
    expect(html).toContain("Close file");
    expect(html).not.toContain("Paste code");
  });
});
