<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { listSupportedGames } from "$lib/api/steam";
  import type { GameInfo } from "$lib/api/types";
  import { lang, setLang, t, type Lang } from "$lib/i18n";
  import { toastError } from "$lib/errors";
  import Toast from "$lib/components/Toast.svelte";

  let { children } = $props();

  type Palette = "solarized" | "steam" | "forest" | "iris";
  type Theme = "auto" | "light" | "dark";

  let palette = $state<Palette>("solarized");
  let theme = $state<Theme>("auto");
  let games = $state<GameInfo[]>([]);

  // Route → display id, used to map a supported-game id to its page path.
  const GAME_PATH: Record<string, string> = {
    master_duel: "/games/master-duel",
  };

  const isSteam = $derived(page.url.pathname.startsWith("/steam"));

  function applyAppearance() {
    const root = document.documentElement;
    if (theme === "auto") delete root.dataset.theme;
    else root.dataset.theme = theme;
    if (palette === "solarized") delete root.dataset.palette;
    else root.dataset.palette = palette;
    localStorage.setItem("sm-theme", theme);
    localStorage.setItem("sm-palette", palette);
  }

  function onPalette(e: Event) {
    palette = (e.currentTarget as HTMLSelectElement).value as Palette;
    applyAppearance();
  }
  function onTheme(e: Event) {
    theme = (e.currentTarget as HTMLSelectElement).value as Theme;
    applyAppearance();
  }
  function onLang(e: Event) {
    setLang((e.currentTarget as HTMLSelectElement).value as Lang);
  }

  function gamePath(g: GameInfo): string {
    return GAME_PATH[g.id] ?? `/games/${g.id}`;
  }
  function isGameActive(g: GameInfo): boolean {
    return page.url.pathname.startsWith(gamePath(g));
  }

  onMount(() => {
    const storedTheme = localStorage.getItem("sm-theme") as Theme | null;
    const storedPalette = localStorage.getItem("sm-palette") as Palette | null;
    if (storedTheme) theme = storedTheme;
    if (storedPalette) palette = storedPalette;
    applyAppearance();

    (async () => {
      try {
        games = await listSupportedGames();
      } catch (e) {
        toastError(e);
      }
    })();
  });
</script>

<div class="app">
  <aside class="nav">
    <div class="brand">
      <span class="logo">
        <svg viewBox="0 0 24 24"
          ><path
            d="M12 2a10 10 0 00-3.6 19.3c.1-1 .2-2 .5-2.6-1.6-.3-3.2-.8-3.2-3.6a2.8 2.8 0 01.8-2 2.6 2.6 0 01.1-1.9s.7-.2 2.2.8a7.6 7.6 0 014 0c1.5-1 2.2-.8 2.2-.8a2.6 2.6 0 01.1 1.9 2.8 2.8 0 01.8 2c0 2.8-1.7 3.3-3.3 3.5.3.2.5.7.5 1.5v2.5A10 10 0 0012 2z"
          /></svg
        >
      </span>
      <div>
        <div class="name">steam-mate</div>
        <div class="ver">v0.1.0</div>
      </div>
    </div>

    <a class="nav-item" class:active={isSteam} href="/steam">
      <span class="ico">
        <svg viewBox="0 0 24 24"
          ><path
            d="M12 2a10 10 0 00-10 10 10 10 0 007 9.5l3.2-4.6a3.4 3.4 0 102.8-5.3 3.4 3.4 0 00-3 1.8l-4.6 1.9a2.6 2.6 0 100 .3zM18 11.6a2.2 2.2 0 11-2.2-2.2A2.2 2.2 0 0118 11.6z"
          /></svg
        >
      </span>
      {$t("steamTitle")}
      <span class="dot"></span>
    </a>

    <div class="nav-label">{$t("navGames")}</div>
    {#each games as g (g.id)}
      <a class="nav-item sub" class:active={isGameActive(g)} href={gamePath(g)}>
        <span class="ico">
          <svg viewBox="0 0 24 24"
            ><path
              d="M7 7h10a5 5 0 015 5 4 4 0 01-7.2 2.4l-.3-.4H9.5l-.3.4A4 4 0 012 12a5 5 0 015-5zm-1 3v2H4v2h2v2h2v-2h2v-2H8v-2zm10.5 0a1.25 1.25 0 100 2.5 1.25 1.25 0 000-2.5zm-2 3a1.25 1.25 0 100 2.5 1.25 1.25 0 000-2.5z"
            /></svg
          >
        </span>
        <span>{g.id === "master_duel" ? $t("navMd") : g.displayName}</span>
      </a>
    {/each}

    <div class="lang-row first">
      <svg class="lang-globe" viewBox="0 0 24 24"
        ><path
          d="M12 3a9 9 0 000 18h1.5a2.5 2.5 0 001.9-4.1 1.5 1.5 0 011.1-2.4H19a3 3 0 003-3c0-4.7-4.5-8.5-10-8.5zM6.5 12A1.5 1.5 0 118 10.5 1.5 1.5 0 016.5 12zm3-4A1.5 1.5 0 1111 6.5 1.5 1.5 0 019.5 8zm5 0A1.5 1.5 0 1116 6.5 1.5 1.5 0 0114.5 8zm3 4a1.5 1.5 0 111.5-1.5 1.5 1.5 0 01-1.5 1.5z"
        /></svg
      >
      <select
        class="lang-sel"
        aria-label="Color"
        value={palette}
        onchange={onPalette}
      >
        <option value="solarized">Solarized</option>
        <option value="steam">Steam</option>
        <option value="forest">Forest</option>
        <option value="iris">Iris</option>
      </select>
    </div>
    <div class="lang-row">
      <svg class="lang-globe" viewBox="0 0 24 24"
        ><path d="M12.3 3a9 9 0 108.7 11.4A7.2 7.2 0 0112.3 3z" /></svg
      >
      <select
        class="lang-sel"
        aria-label="Theme"
        value={theme}
        onchange={onTheme}
      >
        <option value="auto">{$t("themeAuto")}</option>
        <option value="light">{$t("themeLight")}</option>
        <option value="dark">{$t("themeDark")}</option>
      </select>
    </div>
    <div class="lang-row">
      <svg class="lang-globe" viewBox="0 0 24 24"
        ><path
          d="M12 2a10 10 0 100 20 10 10 0 000-20zm6.9 6h-3a15.6 15.6 0 00-1.3-3.5A8 8 0 0118.9 8zM12 4a13.8 13.8 0 011.8 4h-3.6A13.8 13.8 0 0112 4zM4.3 14a8.2 8.2 0 010-4h3.4a16.8 16.8 0 000 4zm.8 2h3a15.6 15.6 0 001.3 3.5A8 8 0 015.1 16zm3-8h-3a8 8 0 014.3-3.5A15.6 15.6 0 008.1 8zM12 20a13.8 13.8 0 01-1.8-4h3.6A13.8 13.8 0 0112 20zm2.3-6H9.7a14.7 14.7 0 010-4h4.6a14.7 14.7 0 010 4zm.6 5.5a15.6 15.6 0 001.3-3.5h3a8 8 0 01-4.3 3.5zM16.3 14a16.8 16.8 0 000-4h3.4a8.2 8.2 0 010 4z"
        /></svg
      >
      <select
        class="lang-sel"
        aria-label="Language"
        value={$lang}
        onchange={onLang}
      >
        <option value="en">English</option>
        <option value="zh">简体中文</option>
        <option value="zht">繁體中文</option>
        <option value="ja">日本語</option>
      </select>
    </div>
  </aside>

  <main class="main">
    {@render children?.()}
  </main>
</div>

<Toast />
