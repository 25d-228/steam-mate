<script lang="ts">
  import "../app.css";
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { page } from "$app/state";
  import { listSupportedGames } from "$lib/api/steam";
  import type { GameInfo } from "$lib/api/types";
  import { lang, setLang, t, type Lang } from "$lib/i18n";
  import { hue, initial } from "$lib/avatar";
  import { toastError } from "$lib/errors";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import * as Avatar from "$lib/components/ui/avatar/index.js";
  import * as NativeSelect from "$lib/components/ui/native-select/index.js";
  import * as Sidebar from "$lib/components/ui/sidebar/index.js";
  import { Toaster } from "$lib/components/ui/sonner/index.js";
  import Gamepad2Icon from "@lucide/svelte/icons/gamepad-2";
  import LanguagesIcon from "@lucide/svelte/icons/languages";
  import PaletteIcon from "@lucide/svelte/icons/palette";
  import SunMoonIcon from "@lucide/svelte/icons/sun-moon";
  import UsersIcon from "@lucide/svelte/icons/users";
  import {
    steamAccounts,
    ensureSteamAccounts,
    refreshSteamAccounts,
    steamRunning,
    refreshSteamRunning,
  } from "$lib/stores/steam";

  let { children } = $props();

  // MostRecent names the auto-login target; it is "signed in" only while a
  // Steam process actually exists. With Steam closed the chip and tray must
  // not claim anyone is signed in.
  const current = $derived($steamAccounts.find((a) => a.mostRecent) ?? null);
  const signedIn = $derived($steamRunning ? current : null);

  // Fetch its avatar as soon as we know who is signed in.
  $effect(() => {
    if (current) fetchAvatar(current.steamId64);
  });

  type Palette =
    | "solarized"
    | "steam"
    | "forest"
    | "iris"
    | "nord"
    | "gruvbox"
    | "rosepine";
  type Theme = "auto" | "light" | "dark";

  let palette = $state<Palette>("steam");
  let theme = $state<Theme>("auto");
  let games = $state<GameInfo[]>([]);

  // Route → display id, used to map a supported-game id to its page path.
  const GAME_PATH: Record<string, string> = {
    master_duel: "/games/master-duel",
  };

  const isSteam = $derived(page.url.pathname.startsWith("/steam"));

  // The root layout wraps every route, including the frameless tray popup at
  // /tray. That window renders its own chrome and runs its own wiring, so the
  // layout must contribute nothing but the children there.
  const isTray = $derived(page.url.pathname.startsWith("/tray"));

  function applyAppearance() {
    const root = document.documentElement;
    if (theme === "auto") delete root.dataset.theme;
    else root.dataset.theme = theme;
    // "solarized" matches no [data-palette] rule, so the bare :root vars apply.
    root.dataset.palette = palette;
  }

  // Persist only on an explicit pick — applyAppearance() must not write the
  // startup default to storage as if the user had chosen it.
  function onPalette(e: Event) {
    palette = (e.currentTarget as HTMLSelectElement).value as Palette;
    applyAppearance();
    localStorage.setItem("sm-palette", palette);
  }
  function onTheme(e: Event) {
    theme = (e.currentTarget as HTMLSelectElement).value as Theme;
    applyAppearance();
    localStorage.setItem("sm-theme", theme);
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

  let unlisten: UnlistenFn | undefined;
  let unlistenErr: UnlistenFn | undefined;
  let runTimer: ReturnType<typeof setInterval> | undefined;
  // How often to re-probe whether Steam is actually running.
  const RUNNING_POLL_MS = 5000;

  onMount(() => {
    const storedTheme = localStorage.getItem("sm-theme") as Theme | null;
    // Builds before 0.2.2 persisted the startup default ("solarized") on every
    // launch without the user choosing it, so a stored "solarized" carries no
    // signal — drop it once and let the Steam default apply. From now on the
    // value is written only on an explicit pick, so choosing Solarized sticks.
    let storedPalette = localStorage.getItem("sm-palette") as Palette | null;
    if (storedPalette === "solarized") {
      localStorage.removeItem("sm-palette");
      storedPalette = null;
    }
    if (storedTheme) theme = storedTheme;
    if (storedPalette) palette = storedPalette;
    applyAppearance();

    // The tray popup wants the same palette but none of the chrome wiring
    // below (probe interval, focus/event listeners, account preload) — it does
    // its own. Bail out once appearance is applied.
    if (isTray) return;

    (async () => {
      try {
        games = await listSupportedGames();
      } catch (e) {
        toastError(e);
      }
    })();

    // Populate the signed-in chip even before the Steam page is visited.
    ensureSteamAccounts();

    // Keep the "is Steam actually running" probe fresh: now, on focus, and on
    // a slow tick — the user can open or quit Steam outside the app anytime.
    refreshSteamRunning();
    window.addEventListener("focus", refreshSteamRunning);
    runTimer = setInterval(refreshSteamRunning, RUNNING_POLL_MS);

    // The backend emits "accounts-changed" after a tray quick-switch (and any
    // other out-of-band change); refresh the shared store so the chip, tray
    // text, and Steam page all react.
    listen("accounts-changed", () => {
      refreshSteamAccounts().catch(() => {
        /* best-effort */
      });
      refreshSteamRunning();
    })
      .then((un) => {
        unlisten = un;
      })
      .catch(() => {
        /* event API unavailable; ignore */
      });

    // A tray-initiated switch that fails has no page to report to — surface
    // it through the global toast so the click isn't silently swallowed.
    listen<string>("switch-error", (e) => {
      toastError(e.payload);
    })
      .then((un) => {
        unlistenErr = un;
      })
      .catch(() => {
        /* event API unavailable; ignore */
      });
  });

  onDestroy(() => {
    unlisten?.();
    unlistenErr?.();
    if (runTimer) clearInterval(runTimer);
    // Unlike onMount, onDestroy also runs during prerender — guard the
    // window access (same pattern as the pages).
    if (typeof window !== "undefined")
      window.removeEventListener("focus", refreshSteamRunning);
  });
</script>

{#if isTray}
  {@render children?.()}
{:else}
  <div
    data-slot="sidebar-wrapper"
    class="flex h-screen min-h-0 w-full overflow-hidden [animation:win-in_0.45s_cubic-bezier(0.2,0.8,0.25,1)_both] motion-reduce:animate-none"
    style="--sidebar-width: 13.25rem;"
  >
    <Sidebar.Root
      class="border-r border-sidebar-border"
      role="navigation"
      aria-label="Primary navigation"
    >
      <Sidebar.Header class="gap-3 px-3 pt-3 pb-2">
        <div class="flex items-center gap-2.5 px-1 py-1">
          <span
            class="flex size-7 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-primary to-[var(--accent-2)] text-primary-foreground shadow-sm"
          >
            <UsersIcon class="size-4" aria-hidden="true" />
          </span>
          <span class="truncate text-sm font-bold tracking-[0.01em] text-sidebar-foreground">
            steam-mate
          </span>
        </div>

        {#if $steamRunning}
          <div
            class="flex items-center gap-2 rounded-lg border border-sidebar-border bg-background/45 p-2"
            title={signedIn?.accountName ?? ""}
          >
            <Avatar.Root size="sm" class="rounded-md">
              {#if signedIn && $avatars[signedIn.steamId64]}
                <Avatar.Image
                  class="rounded-md"
                  src={$avatars[signedIn.steamId64]}
                  alt=""
                />
              {/if}
              <Avatar.Fallback
                class="rounded-md text-[10px] font-bold text-white"
                style={`background: ${signedIn
                  ? hue(signedIn.accountName)
                  : "var(--border)"}`}
              >
                {signedIn ? initial(signedIn.personaName) : "—"}
              </Avatar.Fallback>
              <Avatar.Badge class="bg-[var(--green)]" />
            </Avatar.Root>
            <span class="flex min-w-0 flex-col leading-tight">
              <span
                class="truncate text-[9.5px] font-bold tracking-[0.08em] text-muted-foreground uppercase"
              >{$t("signedInAs")}</span>
              <b class="truncate text-xs text-sidebar-foreground">
                {signedIn ? signedIn.personaName : "—"}
              </b>
            </span>
          </div>
        {/if}
      </Sidebar.Header>

      <Sidebar.Separator />
      <Sidebar.Content>
        <Sidebar.Group class="pt-2">
          <Sidebar.GroupContent>
            <Sidebar.Menu>
              <Sidebar.MenuItem>
                <Sidebar.MenuButton
                  isActive={isSteam}
                  class="data-[active=true]:bg-primary data-[active=true]:text-primary-foreground"
                >
                  {#snippet child({ props })}
                    <a href="/steam" {...props}>
                      <UsersIcon aria-hidden="true" />
                      <span>{$t("steamTitle")}</span>
                    </a>
                  {/snippet}
                </Sidebar.MenuButton>
                <Sidebar.MenuBadge aria-hidden="true">
                  <span
                    class="size-1.5 rounded-full bg-[var(--green)] shadow-[0_0_0_3px_rgba(133,153,0,0.18)]"
                  ></span>
                </Sidebar.MenuBadge>
              </Sidebar.MenuItem>
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        </Sidebar.Group>

        <Sidebar.Group class="pt-0">
          <Sidebar.GroupLabel>{$t("navGames")}</Sidebar.GroupLabel>
          <Sidebar.GroupContent>
            <Sidebar.Menu>
              {#each games as g (g.id)}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton
                    isActive={isGameActive(g)}
                    class="pl-3 data-[active=true]:bg-primary data-[active=true]:text-primary-foreground"
                  >
                    {#snippet child({ props })}
                      <a href={gamePath(g)} {...props}>
                        <Gamepad2Icon aria-hidden="true" />
                        <span>
                          {g.id === "master_duel" ? $t("navMd") : g.displayName}
                        </span>
                      </a>
                    {/snippet}
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        </Sidebar.Group>
      </Sidebar.Content>

      <Sidebar.Separator />
      <Sidebar.Footer class="gap-1 px-3 py-3">
        <label class="flex items-center gap-2">
          <PaletteIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <NativeSelect.Root
            class="min-w-0 flex-1 w-full"
            size="sm"
            aria-label="Color palette"
            value={palette}
            onchange={onPalette}
          >
            <NativeSelect.Option value="steam">Steam</NativeSelect.Option>
            <NativeSelect.Option value="solarized">Solarized</NativeSelect.Option>
            <NativeSelect.Option value="forest">Forest</NativeSelect.Option>
            <NativeSelect.Option value="iris">Iris</NativeSelect.Option>
            <NativeSelect.Option value="nord">Nord</NativeSelect.Option>
            <NativeSelect.Option value="gruvbox">Gruvbox</NativeSelect.Option>
            <NativeSelect.Option value="rosepine">Rosé Pine</NativeSelect.Option>
          </NativeSelect.Root>
        </label>

        <label class="flex items-center gap-2">
          <SunMoonIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <NativeSelect.Root
            class="min-w-0 flex-1 w-full"
            size="sm"
            aria-label="Theme"
            value={theme}
            onchange={onTheme}
          >
            <NativeSelect.Option value="auto">{$t("themeAuto")}</NativeSelect.Option>
            <NativeSelect.Option value="light">{$t("themeLight")}</NativeSelect.Option>
            <NativeSelect.Option value="dark">{$t("themeDark")}</NativeSelect.Option>
          </NativeSelect.Root>
        </label>

        <label class="flex items-center gap-2">
          <LanguagesIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <NativeSelect.Root
            class="min-w-0 flex-1 w-full"
            size="sm"
            aria-label="Language"
            value={$lang}
            onchange={onLang}
          >
            <NativeSelect.Option value="en">English</NativeSelect.Option>
            <NativeSelect.Option value="zh">简体中文</NativeSelect.Option>
            <NativeSelect.Option value="zht">繁體中文</NativeSelect.Option>
            <NativeSelect.Option value="ja">日本語</NativeSelect.Option>
          </NativeSelect.Root>
        </label>
      </Sidebar.Footer>
    </Sidebar.Root>

    <Sidebar.Inset
      class="shell-content h-screen min-h-0 min-w-0 overflow-auto bg-[var(--win)] px-[26px] pt-[22px] pb-[30px]"
    >
      {@render children?.()}
    </Sidebar.Inset>
    <Toaster
      theme={theme === "auto" ? "system" : theme}
      position="bottom-center"
    />
  </div>
{/if}
