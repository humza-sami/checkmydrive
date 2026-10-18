import { describe, expect, it } from "vitest";
import md from "../../../engine/I18N_KEYS.md?raw";
import en from "../locales/en-US.json";


function flatten(o: Record<string, unknown>, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(o)) {
    if (v && typeof v === "object") Object.assign(out, flatten(v as Record<string, unknown>, `${prefix}${k}.`));
    else out[`${prefix}${k}`] = String(v);
  }
  return out;
}
const flat = flatten(en as Record<string, unknown>);

/** code -> placeholder names in the documented en-US text */
function engineCodes(): Map<string, Set<string>> {
  const map = new Map<string, Set<string>>();
  for (const line of md.split("\n")) {
    if (!line.startsWith("| `")) continue;
    const cells = line.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
    const m = cells[0].match(/^`([a-z_]+(?:\.[a-z_]+)+)`/);
    if (!m) continue;
    const text = cells[cells.length - 1];
    map.set(m[1], new Set([...text.matchAll(/\{(\w+)\}/g)].map((x) => x[1])));
  }
  return map;
}
const placeholders = (s: string) => new Set([...s.matchAll(/\{\{(\w+)\}\}/g)].map((x) => x[1]));

// Codes produced by the desktop shell itself (not the engine) or by the UI as fallbacks.
const SHELL_ERRORS = ["unknown", "title", "retry", "choose", "refused", "engine_failed", "engine_missing", "auth_cancelled", "elevation_unavailable"];

describe("en-US.json matches engine/I18N_KEYS.md", () => {
  const codes = engineCodes();

  it("parses a sensible number of codes", () => {
    expect(codes.size).toBeGreaterThan(80);
  });

  it("contains every code the engine emits", () => {
    const missing = [...codes.keys()].filter((c) => !(c in flat));
    expect(missing).toEqual([]);
  });

  it("uses the same interpolation params as the engine text", () => {
    const bad: string[] = [];
    for (const [code, params] of codes) {
      if (!(code in flat)) continue;
      const have = placeholders(flat[code]);
      if ([...params].sort().join() !== [...have].sort().join()) bad.push(`${code}: engine {${[...params]}} vs ui {${[...have]}}`);
    }
    expect(bad).toEqual([]);
  });

  it("has no orphaned engine-style codes that nothing emits", () => {
    const orphans = Object.keys(flat).filter((k) => {
      if (!/^(issue|progress|suitability|error)\./.test(k)) return false;
      if (/^(issue|progress|suitability)\.(unknown|working)$/.test(k) || k.startsWith("suitability.fallback.")) return false;
      if (k.startsWith("error.") && SHELL_ERRORS.includes(k.slice(6))) return false;
      if (k.endsWith("_seconds")) return !codes.has(k.replace(/_seconds$/, "")); // i18next context variant
      return !codes.has(k);
    });
    expect(orphans).toEqual([]);
  });
});
