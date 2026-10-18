import { beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import i18n from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import en from "../locales/en-US.json";
import { Results } from "../screens/Results";
import { MOCK_DRIVES, mockReport } from "../lib/mock";
import { liveStatuses } from "../screens/Running";

beforeAll(async () => {
  await i18n.use(initReactI18next).init({
    resources: { "en-US": { translation: en } }, lng: "en-US", fallbackLng: "en-US", interpolation: { escapeValue: false },
  });
});

const view = (id: string, mode: "safe" | "wipe") =>
  render(
    <I18nextProvider i18n={i18n}>
      <Results report={mockReport(MOCK_DRIVES.find((d) => d.id === id)!, mode)} onAnother={() => {}} />
    </I18nextProvider>,
  );

describe("Results view", () => {
  it("renders the 'buy' verdict for a healthy drive", () => {
    view("disk4", "safe");
    expect(screen.getByTestId("verdict")).toHaveTextContent("Good to buy");
    expect(screen.getByTestId("score")).toHaveTextContent("96");
    expect(screen.getByText(/Safely store up to 900 GB of 1 TB/)).toBeInTheDocument();
  });
  it("renders the 'caution' verdict", () => {
    view("disk6", "safe");
    expect(screen.getByTestId("verdict")).toHaveTextContent("Use with caution");
  });
  it("renders the 'avoid' verdict with critical issues first", () => {
    view("disk5", "safe");
    expect(screen.getByTestId("verdict")).toHaveTextContent("Avoid");
    const items = screen.getByTestId("issues").querySelectorAll("li");
    expect(items[0].className).toContain("critical");
    expect(items[items.length - 1].className).not.toContain("critical");
  });
  it("calls out fake capacity", () => {
    view("disk6", "wipe");
    expect(screen.getByText(/lying about its size/)).toBeInTheDocument();
  });
  it("has translations for every code the mock emits", () => {
    for (const d of MOCK_DRIVES) for (const m of ["safe", "wipe"] as const)
      for (const i of mockReport(d, m).issues) expect(i18n.exists(i.code, { count: 1 }), i.code).toBe(true);
  });
});

describe("live checklist", () => {
  it("marks earlier phases done and the current one running", () => {
    const s = liveStatuses({ type: "progress", phase: "surface_read", percent: 50, phase_percent: 40, message_code: "x" });
    expect(s.smart_health).toBe("done");
    expect(s.surface_read).toBe("running");
    expect(s.stability).toBe("pending");
  });
});
