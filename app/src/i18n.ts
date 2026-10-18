import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enUS from "./locales/en-US.json";
import { readPref, writePref } from "./lib/settings";

export const LANGUAGES: { code: string; name: string; rtl?: boolean }[] = [
  { code: "en-US", name: "English (US)" },
  { code: "es", name: "Español" },
  { code: "zh-CN", name: "简体中文" },
  { code: "zh-TW", name: "繁體中文" },
  { code: "hi", name: "हिन्दी" },
  { code: "ar", name: "العربية", rtl: true },
  { code: "bn", name: "বাংলা" },
  { code: "pt-BR", name: "Português (Brasil)" },
  { code: "pt-PT", name: "Português (Portugal)" },
  { code: "ru", name: "Русский" },
  { code: "ja", name: "日本語" },
  { code: "de", name: "Deutsch" },
  { code: "fr", name: "Français" },
  { code: "ur", name: "اردو", rtl: true },
  { code: "id", name: "Bahasa Indonesia" },
  { code: "tr", name: "Türkçe" },
  { code: "ko", name: "한국어" },
  { code: "vi", name: "Tiếng Việt" },
  { code: "it", name: "Italiano" },
  { code: "fa", name: "فارسی", rtl: true },
  { code: "pl", name: "Polski" },
  { code: "uk", name: "Українська" },
  { code: "nl", name: "Nederlands" },
  { code: "th", name: "ไทย" },
  { code: "ms", name: "Bahasa Melayu" },
  { code: "ro", name: "Română" },
  { code: "el", name: "Ελληνικά" },
  { code: "cs", name: "Čeština" },
  { code: "sv", name: "Svenska" },
  { code: "hu", name: "Magyar" },
  { code: "he", name: "עברית", rtl: true },
  { code: "da", name: "Dansk" },
  { code: "fi", name: "Suomi" },
  { code: "no", name: "Norsk" },
  { code: "sk", name: "Slovenčina" },
  { code: "bg", name: "Български" },
  { code: "sw", name: "Kiswahili" },
  { code: "ta", name: "தமிழ்" },
  { code: "te", name: "తెలుగు" },
  { code: "mr", name: "मराठी" },
];

const RTL = new Set(LANGUAGES.filter((l) => l.rtl).map((l) => l.code));
const loaders = import.meta.glob(["./locales/*.json", "!./locales/en-US.json"], { import: "default" }) as Record<
  string,
  () => Promise<Record<string, unknown>>
>;

/** Map an OS/browser locale tag to one of our supported codes. */
export function matchLocale(tag: string | undefined | null): string {
  if (!tag) return "en-US";
  const codes = LANGUAGES.map((l) => l.code);
  const lower = tag.toLowerCase();
  const exact = codes.find((c) => c.toLowerCase() === lower);
  if (exact) return exact;
  const [base, region] = lower.split(/[-_]/);
  if (base === "zh") return region && ["tw", "hk", "mo", "hant"].includes(region) ? "zh-TW" : "zh-CN";
  if (base === "pt") return region === "pt" ? "pt-PT" : "pt-BR";
  if (base === "nb" || base === "nn") return "no";
  if (base === "iw") return "he";
  if (base === "in") return "id";
  if (base === "en") return "en-US";
  return codes.find((c) => c.toLowerCase() === base) ?? "en-US";
}

export function applyDirection(lng: string) {
  const root = document.documentElement;
  root.lang = lng;
  root.dir = RTL.has(lng) ? "rtl" : "ltr";
}

export async function loadLanguage(lng: string): Promise<void> {
  if (lng !== "en-US" && !i18n.hasResourceBundle(lng, "translation")) {
    const load = loaders[`./locales/${lng}.json`];
    if (load) {
      try {
        i18n.addResourceBundle(lng, "translation", await load(), true, true);
      } catch {
        /* fall back to en-US */
      }
    }
  }
  await i18n.changeLanguage(lng);
  applyDirection(lng);
}

export async function setLanguage(lng: string) {
  writePref("lang", lng);
  await loadLanguage(lng);
}

export async function initI18n() {
  const saved = readPref("lang");
  const lng = saved && LANGUAGES.some((l) => l.code === saved) ? saved : matchLocale(navigator.language);
  await i18n.use(initReactI18next).init({
    resources: { "en-US": { translation: enUS } },
    lng: "en-US",
    fallbackLng: "en-US",
    interpolation: { escapeValue: false },
    returnEmptyString: false,
  });
  await loadLanguage(lng);
  return i18n;
}

export default i18n;
