import { describe, expect, it } from "vitest";
import i18next from "i18next";
import md from "../../../engine/I18N_KEYS.md?raw";
import en from "../locales/en-US.json";
import { COUNT_PARAM, tCode } from "../lib/codes";
import { LANGUAGES } from "../i18n";

const locales = import.meta.glob("../locales/*.json", { eager: true, import: "default" }) as Record<string, Record<string, unknown>>;
const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/;
/** "issue.smart.crc_errors_other" -> "issue.smart.crc_errors" */
const baseKey = (k: string) => k.replace(PLURAL_SUFFIX, "");


function flatten(o: Record<string, unknown>, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(o)) {
    if (v && typeof v === "object") Object.assign(out, flatten(v as Record<string, unknown>, `${prefix}${k}.`));
    else out[`${prefix}${k}`] = String(v);
  }
  return out;
}
const enFlat = flatten(en as Record<string, unknown>);
/** keys with plural suffixes collapsed to their base key (value = the `_other` / plain text) */
function collapse(f: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(f)) {
    const b = baseKey(k);
    if (b === k || k.endsWith("_other") || !(b in out)) out[b] = v;
  }
  return out;
}
const flat = collapse(enFlat);
/** placeholders used by any plural variant of a key */
const unionPlaceholders = (f: Record<string, string>, base: string) =>
  new Set(Object.entries(f).filter(([k]) => baseKey(k) === base).flatMap(([, v]) => [...placeholders(v)]));

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
      const have = unionPlaceholders(enFlat, code);
      if ([...params].sort().join() !== [...have].sort().join()) bad.push(`${code}: engine {${[...params]}} vs ui {${[...have]}}`);
    }
    expect(bad).toEqual([]);
  });

  it("has no orphaned engine-style codes that nothing emits", () => {
    const orphans = Object.keys(flat).filter((k) => {
      if (!/^(issue|progress|suitability|error)\./.test(k)) return false;
      if (/^(issue|progress|suitability)\.(unknown|working)$/.test(k) || k.startsWith("suitability.fallback.")) return false;
      if (k.startsWith("error.") && SHELL_ERRORS.includes(k.slice(6))) return false;
      if (k.endsWith("_seconds")) return !codes.has(k.replace(/_seconds$/, "")); // i18next context variant (plural suffix already stripped)
      return !codes.has(k);
    });
    expect(orphans).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// Plurals
// ---------------------------------------------------------------------------------------------
const pluralBases = [...new Set(Object.keys(enFlat).filter((k) => PLURAL_SUFFIX.test(k)).map(baseKey))].sort();

describe("plural keys (en-US)", () => {
  it("has plural keys, each with exactly _one and _other and no stray base key", () => {
    expect(pluralBases.length).toBeGreaterThanOrEqual(20);
    for (const b of pluralBases) {
      const forms = Object.keys(enFlat).filter((k) => baseKey(k) === b).map((k) => k.slice(b.length + 1)).sort();
      expect(forms, b).toEqual(["one", "other"]);
      expect(b in enFlat, `${b} should not also exist un-suffixed`).toBe(false);
    }
  });
  it("has no '(s)' hacks left", () => {
    expect(Object.entries(enFlat).filter(([, v]) => /\(s\)|\(es\)/.test(v)).map(([k]) => k)).toEqual([]);
  });
  it("covers every code that is pluralized by count", () => {
    const codes = engineCodes();
    for (const c of Object.keys(COUNT_PARAM)) expect(pluralBases.some((b) => b === c || b === `${c}_seconds`), c).toBe(true);
    // every engine code whose text puts `count` next to a noun/verb must be plural; "increased by {count}" is a bare delta
    const BARE_NUMBER = ["issue.stability.crc_errors_increased", "issue.smart.unreadable_sectors", "issue.smart.pending_sectors", "issue.smart.uncorrectable_sectors"];
    for (const [code, params] of codes) if (!BARE_NUMBER.includes(code) && params.has("count") && code in flat && /\{\{count\}\}/.test(flat[code])) expect(pluralBases, code).toContain(code);
  });
});

describe("plural keys (all locales)", () => {
  const files = Object.entries(locales).map(([p, d]) => [p.match(/([^/]+)\.json$/)![1], flatten(d)] as const);

  it("ships a locale file for every language in the picker", () => {
    expect(files.map(([l]) => l).sort()).toEqual(LANGUAGES.map((l) => l.code).sort());
  });

  for (const [lang, f] of files) {
    it(`${lang}: has all CLDR plural categories for every pluralized key, with placeholders preserved`, () => {
      const cats = new Intl.PluralRules(lang).resolvedOptions().pluralCategories;
      const problems: string[] = [];
      for (const b of pluralBases) {
        if (b in f) problems.push(`${b}: un-suffixed key must be removed`);
        const have = Object.keys(f).filter((k) => baseKey(k) === b).map((k) => k.slice(b.length + 1));
        for (const c of cats) if (!have.includes(c)) problems.push(`${b}: missing _${c}`);
        for (const h of have) if (!cats.includes(h as Intl.LDMLPluralRule)) problems.push(`${b}: unexpected _${h}`);
        const want = placeholders(enFlat[`${b}_other`]);
        for (const c of cats) {
          const text = f[`${b}_${c}`];
          if (text === undefined) continue;
          if (!text.trim()) problems.push(`${b}_${c}: empty`);
          const got = placeholders(text);
          // the `other` form must carry every placeholder; special forms ("one year", "two years") may drop the number only
          if (c === "other" && [...want].sort().join() !== [...got].sort().join()) problems.push(`${b}_other: placeholders {${[...got]}} != en {${[...want]}}`);
          for (const g of got) if (!want.has(g)) problems.push(`${b}_${c}: unknown placeholder {{${g}}}`);
          for (const w of want) if (!got.has(w) && !(c !== "other" && /^(count|years|hours|seconds)$/.test(w))) problems.push(`${b}_${c}: missing {{${w}}}`);
        }
      }
      expect(problems).toEqual([]);
    });

    it(`${lang}: non-plural strings are unchanged in shape (same keys and placeholders as en-US)`, () => {
      const problems: string[] = [];
      const a = collapse(f), b = collapse(enFlat);
      for (const k of Object.keys(b)) {
        if (!(k in a)) { problems.push(`missing ${k}`); continue; }
        if ([...placeholders(a[k])].sort().join() !== [...placeholders(b[k])].sort().join() && !pluralBases.includes(k)) problems.push(`${k}: placeholders differ`);
      }
      expect(problems).toEqual([]);
    });
  }
});

describe("rendering plurals", () => {
  const make = async (lng: string) => {
    const inst = i18next.createInstance();
    await inst.init({
      resources: { "en-US": { translation: en }, [lng]: { translation: locales[`../locales/${lng}.json`] } },
      lng, fallbackLng: "en-US", interpolation: { escapeValue: false },
    });
    return inst;
  };

  it("en-US results.age.years: 1 year / 2 years", async () => {
    const t = (await make("en-US")).t;
    expect(t("results.age.years", { count: 1 })).toBe("About 1 year of use");
    expect(t("results.age.years", { count: 2 })).toBe("About 2 years of use");
    expect(t("results.age.years", { count: 4.7 })).toBe("About 4.7 years of use");
  });

  it("ru results.age.years: 1 / 3 / 5", async () => {
    const t = (await make("ru")).t;
    expect(t("results.age.years", { count: 1 })).toBe("Использовался около 1 года");
    expect(t("results.age.years", { count: 3 })).toBe("Использовался около 3 лет");
    expect(t("results.age.years", { count: 5 })).toBe("Использовался около 5 лет");
  });

  it("tCode maps the quantity param to count for issue.age.* and the self-test timeout", async () => {
    const t = (await make("en-US")).t;
    expect(tCode(t, "issue.age.moderate_hours", { years: 1, hours: 8800 })).toContain("about 1 year (8800 hours)");
    expect(tCode(t, "issue.age.high_hours", { years: 4.7, hours: 41200 })).toContain("about 4.7 years");
    expect(tCode(t, "issue.selftest.timeout", { seconds: 1 })).toContain("within 1 second and");
    expect(tCode(t, "issue.selftest.timeout", { seconds: 90 })).toContain("within 90 seconds and");
    expect(tCode(t, "progress.selftest_short.running", { seconds: 1 })).toContain("1 second so far");
    expect(tCode(t, "progress.selftest_short.running", { seconds: 7 })).toContain("7 seconds so far");
    expect(tCode(t, "progress.selftest_short.running", { percent: 40 })).toContain("40% done");
  });

  it("tCode pluralizes count-param issues in en-US and ru", async () => {
    const en_ = (await make("en-US")).t;
    expect(tCode(en_, "issue.stability.timeouts", { count: 1 })).toBe("The drive timed out 1 time during the tests.");
    expect(tCode(en_, "issue.stability.timeouts", { count: 3 })).toBe("The drive timed out 3 times during the tests.");
    expect(tCode(en_, "issue.smart.reallocated_sectors", { count: 1 })).toContain("1 damaged area");
    expect(tCode(en_, "issue.smart.pending_sectors", { count: 40 })).toBe("Areas the drive cannot read right now and may replace later: 40");
    const ru = (await make("ru")).t;
    expect(tCode(ru, "issue.stability.new_bad_sectors", { count: 1 })).toContain("1 новая");
    expect(tCode(ru, "issue.stability.new_bad_sectors", { count: 3 })).toContain("3 новые");
    expect(tCode(ru, "issue.stability.new_bad_sectors", { count: 5 })).toContain("5 новых");
    expect(tCode(ru, "issue.selftest.timeout", { seconds: 21 })).toContain("21 секунду");
  });

  it("keeps Western digits for interpolated values in mr", async () => {
    const t = (await make("mr")).t;
    expect(tCode(t, "issue.stability.timeouts", { count: 12 })).toContain("12");
  });
});
