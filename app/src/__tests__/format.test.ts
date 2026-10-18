import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, hoursToYears, scoreTone, verdictFromScore, formatManufactureDate } from "../lib/format";
import { matchLocale } from "../i18n";

describe("formatBytes", () => {
  it("uses decimal units like drive makers", () => {
    expect(formatBytes(500_000_000_000, "en-US")).toBe("500 GB");
    expect(formatBytes(2_000_000_000_000, "en-US")).toBe("2 TB");
    expect(formatBytes(1_500_000_000, "en-US")).toBe("1.5 GB");
    expect(formatBytes(999, "en-US")).toBe("999 B");
  });
  it("handles bad input", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(-5)).toBe("—");
  });
});

describe("hoursToYears", () => {
  it("converts power-on hours", () => {
    expect(hoursToYears(8760)).toBe(1);
    expect(hoursToYears(41230)).toBe(4.7);
    expect(hoursToYears(0)).toBe(0);
    expect(hoursToYears(null)).toBeNull();
  });
});

describe("misc helpers", () => {
  it("formats durations", () => {
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(125)).toBe("2:05");
    expect(formatDuration(3700)).toBe("1:01:40");
  });
  it("maps score to tone/verdict at the thresholds", () => {
    expect(scoreTone(80)).toBe("good");
    expect(scoreTone(79)).toBe("caution");
    expect(scoreTone(50)).toBe("caution");
    expect(scoreTone(49)).toBe("bad");
    expect(verdictFromScore(80)).toBe("buy");
    expect(verdictFromScore(10)).toBe("avoid");
  });
  it("formats manufacture dates", () => {
    expect(formatManufactureDate("2023-08", "en-US")).toBe("August 2023");
    expect(formatManufactureDate("2023-W18")).toContain("W18");
    expect(formatManufactureDate(null)).toBeNull();
  });
  it("matches OS locales to supported codes", () => {
    expect(matchLocale("es-MX")).toBe("es");
    expect(matchLocale("zh-Hant-TW")).toBe("zh-TW");
    expect(matchLocale("pt-PT")).toBe("pt-PT");
    expect(matchLocale("pt")).toBe("pt-BR");
    expect(matchLocale("nb-NO")).toBe("no");
    expect(matchLocale("xx")).toBe("en-US");
  });
});
