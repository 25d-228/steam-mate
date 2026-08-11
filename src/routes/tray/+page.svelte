<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { switchAccount } from "$lib/api/steam";
  import { appExit } from "$lib/api/app";
  import { t, setLang, type Lang } from "$lib/i18n";
  import { hue, initial } from "$lib/avatar";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import {
    steamAccounts,
    steamRunning,
    refreshSteamAccounts,
    refreshSteamRunning,
  } from "$lib/stores/steam";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import PanelTopOpenIcon from "@lucide/svelte/icons/panel-top-open";
  import PowerIcon from "@lucide/svelte/icons/power";

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
      await switchAccount(accountName, false);
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
    await appExit();
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

<Card.Root
  class="h-screen w-screen gap-0 rounded-none bg-[var(--win)] py-0 ring-1 ring-border ring-inset"
>
  <header class="shrink-0 px-2.5 py-2 text-[11px] text-muted-foreground">
    {#if $steamRunning}
      {$t("signedInAs")}
      <strong class="font-semibold text-foreground">
        {current ? current.personaName : "—"}
      </strong>
    {:else}
      {$t("steamOff")}
    {/if}
  </header>

  <Separator />

  <div class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1">
    {#each $steamAccounts as a (a.accountName)}
      {@const uri = $avatars[a.steamId64]}
      {@const busy = switching === a.accountName}
      <Button
        variant="ghost"
        class="h-auto min-h-8 w-full justify-start gap-2 px-2.5 py-1.5 text-left text-[12.5px] font-normal text-foreground disabled:opacity-60"
        disabled={a.mostRecent || switching !== null}
        aria-busy={busy}
        onclick={() => switchTo(a.accountName)}
      >
        <Avatar.Root class="size-[18px] rounded-[5px]">
          {#if uri}
            <Avatar.Image class="rounded-[5px]" src={uri} alt="" />
          {/if}
          <Avatar.Fallback
            class="rounded-[5px] text-[9px] font-bold text-white"
            style={`background: ${hue(a.accountName)}`}
          >
            {initial(a.personaName)}
          </Avatar.Fallback>
        </Avatar.Root>
        <span class="min-w-0 flex-1 truncate font-semibold">{a.personaName}</span>
        <span class="shrink-0 font-mono text-[10px] text-muted-foreground">
          {a.accountName}
        </span>
        {#if busy}
          <LoaderCircleIcon
            class="ml-auto size-3 shrink-0 animate-spin text-primary motion-reduce:animate-none"
            aria-hidden="true"
          />
        {:else if $steamRunning && a.mostRecent}
          <Badge
            class="ml-auto size-2 shrink-0 rounded-full bg-[var(--green)] p-0"
            title={$t("signedIn")}
          >
            <span class="sr-only">{$t("signedIn")}</span>
          </Badge>
        {/if}
      </Button>
    {/each}
  </div>

  <Separator />

  <footer class="shrink-0 px-1.5 py-1">
    <Button
      variant="ghost"
      class="w-full justify-start px-2.5 text-[12.5px] font-normal"
      onclick={openMain}
    >
      <PanelTopOpenIcon data-icon="inline-start" aria-hidden="true" />
      {$t("trayOpen")}
    </Button>
    <Button
      variant="ghost"
      class="w-full justify-start px-2.5 text-[12.5px] font-normal"
      onclick={exit}
    >
      <PowerIcon data-icon="inline-start" aria-hidden="true" />
      {$t("trayExit")}
    </Button>
  </footer>
</Card.Root>

<style>
  /* The popup fills its own frameless window; a solid surface with a single
     hairline border. The window itself is opaque and rectangular, so no
     border-radius — rounded corners would just expose the page background —
     and the body behind us must match the menu surface, not the app
     backdrop's gradient. */
  :global(body) {
    background: var(--win);
  }
</style>
