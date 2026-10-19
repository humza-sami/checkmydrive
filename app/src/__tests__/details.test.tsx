import { beforeAll, describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import i18n from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import en from "../locales/en-US.json";
import { Results } from "../screens/Results";
import { MOCK_DRIVES, mockReport } from "../lib/mock";
import { formatLatency, formatSpeed, rawMetrics, testHighlights } from "../lib/details";
import { buildHtmlReport } from "../lib/report";

beforeAll(async () => {
  await i18n.use(initReactI18next).init({ resources: { "en-US": { translation: en } }, lng: "en-US", fallbackLng: "en-US", interpolation: { escapeValue: false } });
});

const real = () => mockReport(MOCK_DRIVES.find((d) => d.id === "sda")!, "safe");
const test = (id: string) => real().tests.find((x) => x.id === id)!;
const text = (id: string) => testHighlights(i18n.t, test(id), "en-US").map((h) => (h.value ? `${h.label}: ${h.value}` : h.label)).join(" | ");

describe("friendly test details (real failing ST2000LM007 report)", () => {
  it("shows a few translated highlights per test", () => {
    expect(text("smart_health")).toBe("Replaced bad areas: 736 | Unreadable areas: 40 | Temperature (°C): 29");
    expect(text("surface_read")).toBe("Read errors: 1 | Slowest response: 12.6 s | Slow spots: 1 | Spots checked: 13 of 576");
    expect(text("stability")).toBe("New bad areas during test: 296 | New unreadable areas during test: 16 | Input/output errors: 1");
    expect(text("selftest_short")).toBe("Result: Did not finish in time");
  });

  it("hides nulls and internal fields", () => {
    const all = real().tests.map((x) => text(x.id)).join(" | ");
    for (const bad of ["null", "true", "false", "protocol", "device type", "smart compared", "serial known", "first bad offsets", "latency ms p50", "46976073728", "available spare", "undefined"])
      expect(all.toLowerCase(), bad).not.toContain(bad.toLowerCase());
  });

  it("does not report a 0 speed when the engine could not measure one", () => {
    expect(text("surface_read")).not.toMatch(/speed/i);
    expect(testHighlights(i18n.t, { ...test("surface_read"), metrics: { read_mbps_avg: 0.27 } }, "en-US")[0].value).toBe("0.27");
  });

  it("formats speeds and latencies sensibly", () => {
    expect(formatSpeed(0.27)).toBe("0.27");
    expect(formatSpeed(0.3)).toBe("0.3");
    expect(formatSpeed(17.46)).toBe("17.5");
    expect(formatSpeed(912.4)).toBe("912");
    expect(formatLatency(12599.73)).toBe("12.6 s");
    expect(formatLatency(129.2)).toBe("129 ms");
    expect(formatLatency(4.1)).toBe("4.1 ms");
  });

  it("keeps the full raw metrics (nulls shown as a dash) for the dialog and the saved report", () => {
    const raw = Object.fromEntries(rawMetrics(test("smart_health")));
    expect(raw.protocol).toBe("ata");
    expect(raw.available_spare_percent).toBe("—");
    expect(Object.keys(raw).length).toBeGreaterThan(10);
    const html = buildHtmlReport(real(), i18n.t, "en-US", "ltr");
    expect(html).toContain("protocol: ata");
    expect(html).toContain("Replaced bad areas: 736");
  });
});

const view = (r = real()) =>
  render(<I18nextProvider i18n={i18n}><Results report={r} onAnother={() => {}} /></I18nextProvider>);

describe("Results view for the real failing drive", () => {
  it("is BAD 0/100 Avoid and explains why", () => {
    view();
    expect(screen.getByTestId("score")).toHaveTextContent("0");
    expect(screen.getByTestId("verdict")).toHaveTextContent("Avoid");
    const why = screen.getByTestId("why-score");
    expect(why).toHaveTextContent("Why this score");
    expect(why).toHaveTextContent("some parts of the drive could not be read");
    expect(why).toHaveTextContent("many bad sectors");
    expect(why).toHaveTextContent("unreadable areas");
  });

  it("omits the explanation box when the report has no reasons (perfect score / old engine)", () => {
    const r = { ...real(), score_reasons: [] };
    view(r);
    expect(screen.queryByTestId("why-score")).toBeNull();
  });

  it("renders friendly details, no raw keys, and opens the raw dialog", () => {
    view();
    const matrix = screen.getByTestId("matrix");
    expect(matrix).toHaveTextContent("Replaced bad areas: 736");
    expect(matrix).toHaveTextContent("Spots checked: 13 of 576");
    expect(matrix).not.toHaveTextContent(/null|serial known|first bad offsets/i);
    const selfRow = within(matrix).getByText("Built-in self-check").closest("tr")!;
    expect(selfRow).toHaveTextContent("Skipped"); // a timed-out self-check is never "Passed"
    expect(selfRow).not.toHaveTextContent("Passed");
    fireEvent.click(within(matrix).getAllByRole("button", { name: "Details…" })[1]);
    expect(screen.getByTestId("raw-metrics")).toHaveTextContent("protocol");
  });

  it("merged unreadable-areas issue and no slow-drive noise", () => {
    view();
    const list = screen.getByTestId("issues");
    expect(list).toHaveTextContent("Unreadable areas the drive could not repair: 40");
    expect(list).not.toHaveTextContent(/slow \(/);
    expect(list).toHaveTextContent("only check 13 of 576 spots");
  });
});

describe("every code the real-report mock emits is translated", () => {
  it("issues and score reasons exist in en-US", () => {
    const r = real();
    for (const i of r.issues) expect(i18n.exists(i.code, { count: 1 }), i.code).toBe(true);
    for (const c of r.score_reasons ?? []) expect(i18n.exists(c), c).toBe(true);
  });
});
