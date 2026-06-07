<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { t, setLang, type Lang } from "$lib/i18n";
  import { hue, initial } from "$lib/avatar";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import {
    steamAccounts,
    steamRunning,
    refreshSteamAccounts,
    refreshSteamRunning,
  } from "$lib/stores/steam";

  // MostRecent names the auto-login target; it is the "signed in" account only
  // while Steam is actually running, exactly like the sidebar chip.
  const current = $derived($steamAccounts.find((a) => a.mostRecent) ?? null);

  // The clicked row stays busy until its switch resolves; further clicks are
  // ignored while one is in flight.
  let switching = $state<string | null>(null);

  /** Re-read everything the popup shows; fetch any missing avatars. */
  function refresh() {
    // This webview initialized its language/appearance once at app start; the
    // main window may have changed them since. Re-read the persisted choices
    // so the popup opens localized and in the current palette.
    try {
      const l = localStorage.getItem("sm-lang") as Lang | null;
      if (l) setLang(l);
      const root = document.documentElement;
      const th = localStorage.getItem("sm-theme");
      if (!th || th === "auto") delete root.dataset.theme;
      else root.dataset.theme = th;
      root.dataset.palette = localStorage.getItem("sm-palette") || "steam";
    } catch {
      /* ignore */
    }
    refreshSteamAccounts()
      .then((list) => {
        // Prefetch from the FRESH list, not the store value of the old tick.
        for (const a of list) fetchAvatar(a.steamId64);
      })
      .catch(() => {
        /* best-effort */
      });
    refreshSteamRunning();
  }

  async function switchTo(accountName: string) {
    if (switching) return;
    const a = $steamAccounts.find((x) => x.accountName === accountName);
    if (!a || a.mostRecent) return;
    switching = accountName;
    try {
      await invoke("steam_switch_account", { accountName, offlineMode: false });
      await refreshSteamAccounts();
      refreshSteamRunning();
      // Let the main window react (chip, Steam page, etc.).
      await emit("accounts-changed");
      await getCurrentWindow().hide();
    } catch (e) {
      // Even a failed switch may have changed real state (Steam killed, file
      // flow half-applied) — the main window must re-read truth either way.
      await emit("accounts-changed");
      // No page here to report to — hand the message to the main layout's toast.
      await emit("switch-error", e instanceof Error ? e.message : String(e));
      await getCurrentWindow().hide();
    } finally {
      switching = null;
    }
  }

  async function openMain() {
    await getCurrentWindow().hide();
    const main = await Window.getByLabel("main");
    if (main) {
      await main.show();
      await main.unminimize();
      await main.setFocus();
    }
  }

  async function exit() {
    await invoke("app_exit");
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") getCurrentWindow().hide();
  }

  let unlistenShow: UnlistenFn | undefined;
  let unlistenFocus: UnlistenFn | undefined;

  onMount(() => {
    // The backend emits this right before showing the popup — re-read state so
    // the list, header, and dot are fresh on every open.
    listen("tray-popup-will-show", () => {
      refresh();
    })
      .then((un) => {
        unlistenShow = un;
      })
      .catch(() => {
        /* event API unavailable; ignore */
      });

    // A popup is a transient surface: dismiss it the moment it loses focus —
    // EXCEPT mid-switch: the relaunching Steam steals foreground focus, and
    // hiding then would yank the busy row away while work is in flight.
    getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (!payload && !switching) getCurrentWindow().hide();
      })
      .then((un) => {
        unlistenFocus = un;
      })
      .catch(() => {
        /* unavailable; ignore */
      });

    window.addEventListener("keydown", onKeydown);

    // Also populate once on mount in case the window is created already shown.
    refresh();
  });

  onDestroy(() => {
    unlistenShow?.();
    unlistenFocus?.();
    if (typeof window !== "undefined")
      window.removeEventListener("keydown", onKeydown);
  });
</script>

<div class="traymenu">
  <div class="tm-head">
    {#if $steamRunning}
      {$t("signedInAs")} <b>{current ? current.personaName : "—"}</b>
    {:else}
      {$t("steamOff")}
    {/if}
  </div>

  <div class="tm-list">
    {#each $steamAccounts as a (a.accountName)}
      {@const uri = $avatars[a.steamId64]}
      {@const busy = switching === a.accountName}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="tm-item"
        class:busy
        role={a.mostRecent ? undefined : "button"}
        tabindex={a.mostRecent ? undefined : 0}
        onclick={() => switchTo(a.accountName)}
        onkeydown={(e) =>
          (e.key === "Enter" || e.key === " ") &&
          (e.preventDefault(), switchTo(a.accountName))}
      >
        <span class="mini" style="background:{hue(a.accountName)}">
          {#if uri}
            <img src={uri} alt="" />
          {/if}
          {initial(a.personaName)}
        </span>
        <span class="tm-n">{a.personaName}</span>
        <span class="tm-a">{a.accountName}</span>
        {#if busy}
          <span class="sp"></span>
        {:else if $steamRunning && a.mostRecent}
          <span class="on">●</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="tm-sep"></div>

  <div
    class="tm-item"
    role="button"
    tabindex="0"
    onclick={openMain}
    onkeydown={(e) =>
      (e.key === "Enter" || e.key === " ") && (e.preventDefault(), openMain())}
  >
    {$t("trayOpen")}
  </div>
  <div
    class="tm-item"
    role="button"
    tabindex="0"
    onclick={exit}
    onkeydown={(e) =>
      (e.key === "Enter" || e.key === " ") && (e.preventDefault(), exit())}
  >
    {$t("trayExit")}
  </div>
</div>

<style>
  /* The popup fills its own frameless window; a solid surface with a single
     hairline border. The window itself is opaque and rectangular, so no
     border-radius — rounded corners would just expose the page background —
     and the body behind us must match the menu surface, not the app
     backdrop's gradient. */
  :global(body) {
    background: var(--win);
  }
  .traymenu {
    box-sizing: border-box;
    width: 100vw;
    height: 100vh;
    background: var(--win);
    border: 1px solid var(--border);
    padding: 6px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .tm-head {
    padding: 7px 10px;
    font-size: 11px;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
    margin-bottom: 4px;
    flex: none;
  }
  .tm-head b {
    color: var(--emph);
  }
  .tm-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 2px 0;
  }
  .tm-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 12.5px;
    color: var(--fg);
    -webkit-user-select: none;
    user-select: none;
  }
  .tm-item:hover {
    background: var(--accent-weak);
  }
  .tm-item.busy {
    opacity: 0.6;
    pointer-events: none;
  }
  .tm-item .mini {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    position: relative;
    overflow: hidden;
    flex: none;
    display: grid;
    place-items: center;
    color: #fff;
    font-size: 9px;
    font-weight: 700;
  }
  .tm-item .mini img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .tm-item .tm-n {
    color: var(--emph);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tm-item .tm-a {
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    flex: none;
  }
  .tm-item .on {
    margin-left: auto;
    color: var(--green);
    font-size: 10px;
    flex: none;
  }
  .tm-item .sp {
    margin-left: auto;
    flex: none;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: tm-spin 0.7s linear infinite;
  }
  @keyframes tm-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .tm-sep {
    height: 1px;
    background: var(--border);
    margin: 5px 0;
    flex: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .tm-item .sp {
      animation: none;
    }
  }
</style>
