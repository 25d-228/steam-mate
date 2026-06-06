// Small Svelte i18n store.
//
// Holds the current language (persisted in localStorage as `sm-lang`, defaulting
// from navigator.language), exposes `t(key)` with an English fallback, and
// `fmt(str, vars)` which replaces `{x}` placeholders. Components subscribe to the
// `lang` store so they re-render when the language changes.

import { writable, derived, get } from "svelte/store";
import en from "./en.json";
import zh from "./zh.json";
import zht from "./zht.json";
import ja from "./ja.json";

export type Lang = "en" | "zh" | "zht" | "ja";
type Dict = Record<string, string>;

const DICTS: Record<Lang, Dict> = { en, zh, zht, ja };

/** Pick the default language from the browser locale. */
function detectLang(): Lang {
  const nav = (
    typeof navigator !== "undefined" ? navigator.language || "en" : "en"
  ).toLowerCase();
  if (
    nav.startsWith("zh-tw") ||
    nav.startsWith("zh-hk") ||
    nav.startsWith("zh-hant")
  )
    return "zht";
  if (nav.startsWith("zh")) return "zh";
  if (nav.startsWith("ja")) return "ja";
  return "en";
}

/** Read the stored language, or fall back to the detected one. */
function initialLang(): Lang {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem("sm-lang");
    if (stored && stored in DICTS) return stored as Lang;
  }
  return detectLang();
}

/** The current language. Subscribe to re-render on change. */
export const lang = writable<Lang>(initialLang());

/** Document `lang` attribute for the current language. */
function htmlLang(l: Lang): string {
  return l === "zh" ? "zh-CN" : l === "zht" ? "zh-TW" : l === "ja" ? "ja" : "en";
}

/** Change the active language and persist it. */
export function setLang(l: Lang) {
  lang.set(l);
  if (typeof localStorage !== "undefined") localStorage.setItem("sm-lang", l);
  if (typeof document !== "undefined")
    document.documentElement.lang = htmlLang(l);
}

/** Look up a key for a language, with an English fallback, then the key itself. */
function lookup(l: Lang, key: string): string {
  return DICTS[l]?.[key] ?? DICTS.en[key] ?? key;
}

/** Replace `{x}` placeholders in `str` with values from `vars`. */
export function fmt(str: string, vars: Record<string, unknown> = {}): string {
  return str.replace(/\{(\w+)\}/g, (_, k) =>
    vars[k] != null ? String(vars[k]) : "",
  );
}

/**
 * A derived store giving a `t(key)` function for the current language.
 * Reading `$t` in markup makes the component re-render on language change.
 */
export const t = derived(lang, ($lang) => (key: string) => lookup($lang, key));

/** Non-reactive lookup for the current language (use outside reactive markup). */
export function tNow(key: string): string {
  return lookup(get(lang), key);
}

// Keep the document lang attribute in sync from the start.
if (typeof document !== "undefined") {
  document.documentElement.lang = htmlLang(get(lang));
}
