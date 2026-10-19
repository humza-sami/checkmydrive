import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import i18n from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import en from "../locales/en-US.json";
import { FULL_DISK_ACCESS_URL, FullDiskAccessDialog } from "../components/FullDiskAccessDialog";
import { ModeChoice } from "../screens/ModeChoice";
import { MOCK_DRIVES } from "../lib/mock";
import type { DriveInfo } from "../lib/types";

afterEach(cleanup);
const openExternal = vi.fn();
vi.mock("../lib/links", () => ({ openExternal: (u: string) => openExternal(u), REPO_URL: "x", APP_VERSION: "0" }));

beforeAll(async () => {
  await i18n.use(initReactI18next).init({
    resources: { "en-US": { translation: en } }, lng: "en-US", fallbackLng: "en-US", interpolation: { escapeValue: false },
  });
});

const wrap = (ui: React.ReactElement) => render(<I18nextProvider i18n={i18n}>{ui}</I18nextProvider>);

describe("Full Disk Access dialog", () => {
  it("explains the steps and opens the Privacy_AllFiles pane", () => {
    const onClose = vi.fn();
    const onRetry = vi.fn();
    wrap(<FullDiskAccessDialog onClose={onClose} onRetry={onRetry} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(4);
    expect(FULL_DISK_ACCESS_URL).toBe("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles");
    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    expect(openExternal).toHaveBeenCalledWith(FULL_DISK_ACCESS_URL);
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[1]);
    expect(onClose).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalled();
    expect(screen.getByText(/checkmydrive-engine/, { selector: "li" })).toBeTruthy();
  });
});

describe("wipe confirmation token", () => {
  const withSerial = MOCK_DRIVES.find((d) => d.serial && !d.is_system_disk) as DriveInfo;
  const noSerial: DriveInfo = { ...withSerial, id: "disk6", serial: null };

  it("requires the serial when the drive has one (device id is not accepted)", () => {
    const onStart = vi.fn();
    wrap(<ModeChoice drive={withSerial} onStart={onStart} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByLabelText(/Full wipe test/));
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: withSerial.id } });
    expect((screen.getByRole("button", { name: "Erase and run full test" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: withSerial.serial! } });
    fireEvent.click(screen.getByRole("button", { name: "Erase and run full test" }));
    expect(onStart).toHaveBeenCalledWith("wipe", withSerial.serial);
  });

  it("falls back to the device id when the drive reports no serial", () => {
    const onStart = vi.fn();
    wrap(<ModeChoice drive={noSerial} onStart={onStart} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByLabelText(/Full wipe test/));
    expect(screen.getByText(/type its device name: disk6/)).toBeTruthy();
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "disk7" } });
    expect(screen.getByText("The text does not match the device name yet.")).toBeTruthy();
    fireEvent.change(input, { target: { value: "disk6" } });
    fireEvent.click(screen.getByRole("button", { name: "Erase and run full test" }));
    expect(onStart).toHaveBeenCalledWith("wipe", "disk6");
  });

  it("never offers wipe on the system disk", () => {
    wrap(<ModeChoice drive={{ ...noSerial, is_system_disk: true }} onStart={vi.fn()} onCancel={vi.fn()} />);
    expect((screen.getByLabelText(/Full wipe test/) as HTMLInputElement).disabled).toBe(true);
  });
});
