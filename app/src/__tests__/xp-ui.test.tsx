import { beforeAll, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import i18n from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import en from "../locales/en-US.json";
import { MenuBar, type MenuDef } from "../components/MenuBar";
import { HealthTile } from "../components/DriveOverview";
import { ProgressBar } from "../components/ProgressBar";
import { TitleBar } from "../components/TitleBar";

beforeAll(async () => {
  await i18n.use(initReactI18next).init({
    resources: { "en-US": { translation: en } }, lng: "en-US", fallbackLng: "en-US", interpolation: { escapeValue: false },
  });
});

const wrap = (ui: React.ReactElement) => render(<I18nextProvider i18n={i18n}>{ui}</I18nextProvider>);

describe("menu bar", () => {
  const make = (onStart = vi.fn(), onLang = vi.fn()): MenuDef[] => [
    { id: "test", label: "Test", items: [{ id: "start", label: "Start Test", onSelect: onStart }, { id: "x", label: "Disabled", disabled: true }] },
    { id: "view", label: "View", items: [{ id: "language", label: "Language", submenu: [{ id: "lang:es", label: "Español", checked: false, onSelect: onLang }] }] },
  ];

  it("opens with the mouse and runs the chosen item", () => {
    const onStart = vi.fn();
    wrap(<MenuBar menus={make(onStart)} label="Menu" />);
    fireEvent.click(screen.getByRole("menuitem", { name: "Test" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Start Test" }));
    expect(onStart).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("works from the keyboard, including submenus and Escape", () => {
    const onLang = vi.fn();
    wrap(<MenuBar menus={make(vi.fn(), onLang)} label="Menu" />);
    const view = screen.getByRole("menuitem", { name: "View" });
    view.focus();
    fireEvent.keyDown(view, { key: "ArrowDown" });
    const lang = screen.getByRole("menuitem", { name: "Language" });
    expect(lang).toHaveFocus();
    fireEvent.keyDown(lang, { key: "ArrowRight" });
    const es = screen.getByRole("menuitemradio", { name: "Español" });
    expect(es).toHaveFocus();
    fireEvent.keyDown(es, { key: "Escape" });
    expect(screen.queryByRole("menuitemradio")).toBeNull();
    fireEvent.keyDown(lang, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(view).toHaveFocus();
    expect(onLang).not.toHaveBeenCalled();
  });

  it("skips disabled items when moving with the arrow keys", () => {
    wrap(<MenuBar menus={make()} label="Menu" />);
    const top = screen.getByRole("menuitem", { name: "Test" });
    top.focus();
    fireEvent.keyDown(top, { key: "ArrowDown" });
    const start = screen.getByRole("menuitem", { name: "Start Test" });
    fireEvent.keyDown(start, { key: "ArrowDown" });
    expect(start).toHaveFocus();
  });
});

describe("XP chrome", () => {
  it("title bar has translated minimize / maximize / close buttons", () => {
    wrap(<TitleBar />);
    for (const n of ["Minimize", "Maximize", "Close"]) expect(screen.getByRole("button", { name: n })).toBeInTheDocument();
    expect(screen.getByText("CheckMyDrive")).toBeInTheDocument();
  });
  it("health tile shows the word and score", () => {
    wrap(<HealthTile tone="caution" score={62} />);
    expect(screen.getByRole("img")).toHaveAccessibleName(/Caution.*62/);
  });
  it("progress bar exposes its value", () => {
    wrap(<ProgressBar value={45} label="Overall progress: 45%" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "45");
  });
});
