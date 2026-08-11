<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { copyText } from "$lib/clipboard";
  import {
    getInstallPath,
    listAccounts,
    clearLogin,
    switchAccount,
    forgetAccount,
    forgetAccounts,
  } from "$lib/api/steam";
  import type { SteamAccount } from "$lib/api/types";
  import { t, fmt, lang, tNow, accountLabel } from "$lib/i18n";
  import { hue, initial } from "$lib/avatar";
  import { toast, toastLoading } from "$lib/toast";
  import { toastError } from "$lib/errors";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import {
    steamAccounts,
    steamRunning,
    refreshSteamRunning,
  } from "$lib/stores/steam";
  import * as Avatar from "$lib/components/ui/avatar/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as RadioGroup from "$lib/components/ui/radio-group/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import { cn } from "$lib/utils.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import LayoutGridIcon from "@lucide/svelte/icons/layout-grid";
  import ListIcon from "@lucide/svelte/icons/list";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";

  let installPath = $state<string>("");
  let accounts = $state<SteamAccount[]>([]);
  let offline = $state(false);
  let switching = $state(false);

  // ---- card / list view, remembered per page (default list) ----
  let view = $state<"list" | "card">("list");
  function setView(v: "list" | "card") {
    view = v;
    try {
      localStorage.setItem("sm-view-steam", v);
    } catch {
      /* ignore */
    }
  }

  // open folder state, kept in memory across re-renders
  let openFolders = $state(new Set<string>());

  // hidden accounts (localStorage sm-hidden-steam)
  let hidden = $state<string[]>([]);

  function loadHidden() {
    try {
      const raw = localStorage.getItem("sm-hidden-steam");
      const parsed = raw ? JSON.parse(raw) : [];
      // Guard against a corrupted (non-array) value: `hidden.includes(...)` runs
      // in render, so a number/object/null would throw and blank the list.
      hidden = Array.isArray(parsed)
        ? parsed.filter((x): x is string => typeof x === "string")
        : [];
    } catch {
      hidden = [];
    }
  }
  function saveHidden() {
    localStorage.setItem("sm-hidden-steam", JSON.stringify(hidden));
  }
  function clearHidden() {
    hidden = [];
    saveHidden();
  }

  const visible = $derived(
    accounts.filter((a) => !hidden.includes(a.accountName)),
  );

  // ---- same-name grouping (group by personaName when count > 1) ----
  type Entry =
    | { single: SteamAccount }
    | { folder: string; items: SteamAccount[] };

  const entries = $derived.by<Entry[]>(() => {
    const items = visible;
    const groups = new Map<string, SteamAccount[]>();
    for (const a of items) {
      const n = a.personaName || "";
      if (!groups.has(n)) groups.set(n, []);
      groups.get(n)!.push(a);
    }
    const out: Entry[] = [];
    const seen = new Set<string>();
    for (const a of items) {
      const n = a.personaName || "";
      if (seen.has(n)) continue;
      seen.add(n);
      const g = groups.get(n)!;
      if (n && g.length > 1) out.push({ folder: n, items: g });
      else for (const x of g) out.push({ single: x });
    }
    return out;
  });

  function toggleFolder(name: string) {
    const next = new Set(openFolders);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    openFolders = next;
  }

  // ---- batch selection mode ----
  let selMode = $state(false);
  let selected = $state(new Set<string>());

  function setSelMode(on: boolean) {
    selMode = on;
    selected = new Set();
  }
  function toggleSel(key: string) {
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selected = next;
  }
  function selectAll() {
    // Only what is actually on screen: singles, plus members of folders that
    // are expanded. Accounts inside a collapsed folder never show a checkbox,
    // so "select all" must not sweep them in silently.
    const keys: string[] = [];
    for (const e of entries) {
      if ("single" in e) keys.push(e.single.accountName);
      else if (openFolders.has(e.folder))
        for (const a of e.items) keys.push(a.accountName);
    }
    selected = new Set(keys);
  }
  function clearSel() {
    selected = new Set();
  }

  async function loadAccounts() {
    accounts = await listAccounts();
    steamAccounts.set(accounts);
    // fetch avatars after the list renders
    for (const a of accounts) fetchAvatar(a.steamId64);
  }

  async function refresh() {
    try {
      await loadAccounts();
      toast(tNow("toastRefresh1"), fmt(tNow("toastRefresh2"), { n: accounts.length }));
    } catch (e) {
      toastError(e);
    }
  }

  async function reloadAccountsQuietly() {
    try {
      await loadAccounts();
    } catch (e) {
      toastError(e);
    }
  }

  async function clearAutoLogin() {
    try {
      await clearLogin();
      toast(tNow("toastClear1"), tNow("toastClear2"));
    } catch (e) {
      toastError(e);
    }
  }

  async function switchTo(a: SteamAccount) {
    if ((a.mostRecent && $steamRunning) || switching || selMode) return;
    switching = true;
    const launchOffline = offline;
    const accountDisplay = accountLabel($lang, a.personaName, a.accountName);
    // With Steam closed there is nothing to switch from or shut down — the
    // action is a launch, and the messages say so instead of "closing Steam".
    const launching = !$steamRunning;
    // Spinner stays up for the whole (multi-second) kill/rewrite/relaunch; the
    // success message is shown only once switchAccount actually resolves, so a
    // failure never flashes a false "Signed in" toast.
    toastLoading(
      fmt(tNow(launching ? "toastLaunch1" : "toastSwitch1"), {
        p: a.personaName,
      }),
    );
    try {
      await switchAccount(a.accountName, launchOffline);
      await loadAccounts();
      // The switch just relaunched Steam — re-probe so the chip and tray flip
      // to "signed in" without waiting for the next interval tick.
      refreshSteamRunning();
      toast(
        "",
        fmt(tNow(launching ? "toastLaunch2" : "toastSwitch2"), {
          a: accountDisplay,
          off: launchOffline ? tNow("offlineSuffix") : "",
        }),
      );
    } catch (e) {
      toastError(e);
    } finally {
      switching = false;
    }
  }

  // ---- copy the install path ----
  // The rendered label collapses runs of spaces, so selecting it by hand can
  // yield an invalid path. The button copies the exact path string the page
  // already fetched, every space included.
  async function copyPath(p: string) {
    if (!p) return;
    if (await copyText(p)) toast(tNow("toastCopied"), p);
    else toast("", tNow("errCopy"), true);
  }

  // ---- Steam delete (hide / forget) dialog: single + batch ----
  let steamDeleteAccount = $state<SteamAccount | null>(null);
  let steamDeleteBatch = $state<string[] | null>(null);
  let steamDeleteMode = $state<"hide" | "forget">("hide");
  let steamDeleteOpen = $state(false);
  let steamToolbarFocusTarget = $state<HTMLButtonElement | null>(null);
  let focusSteamToolbarOnDeleteClose = false;

  function openSteamDelete(a: SteamAccount) {
    focusSteamToolbarOnDeleteClose = false;
    steamDeleteAccount = a;
    steamDeleteBatch = null;
    steamDeleteMode = "hide";
    steamDeleteOpen = true;
  }
  function openSteamDeleteBatch() {
    if (selected.size === 0) return;
    focusSteamToolbarOnDeleteClose = false;
    steamDeleteBatch = [...selected];
    steamDeleteAccount = null;
    steamDeleteMode = "hide";
    steamDeleteOpen = true;
  }
  function closeSteamDelete() {
    steamDeleteOpen = false;
    steamDeleteAccount = null;
    steamDeleteBatch = null;
  }

  function onSteamDeleteOpenChange(open: boolean) {
    if (!open) closeSteamDelete();
  }

  function onSteamDeleteCloseAutoFocus(event: Event) {
    if (!focusSteamToolbarOnDeleteClose) return;
    focusSteamToolbarOnDeleteClose = false;
    event.preventDefault();
    steamToolbarFocusTarget?.focus();
  }

  function activateDialogTrigger(
    props: { onclick?: unknown; onkeydown?: unknown },
    event: MouseEvent | KeyboardEvent,
    handlerName: "onclick" | "onkeydown",
  ) {
    const handler = props[handlerName];
    if (typeof handler === "function") handler(event);
  }

  async function confirmSteamDelete() {
    // ---- batch ----
    if (steamDeleteBatch) {
      const batch = steamDeleteBatch;
      const mode = steamDeleteMode;
      focusSteamToolbarOnDeleteClose = true;
      closeSteamDelete();
      if (mode === "hide") {
        const set = new Set(hidden);
        for (const name of batch) set.add(name);
        hidden = [...set];
        saveHidden();
        setSelMode(false);
        toast("", fmt(tNow("toastHideN"), { n: batch.length }));
      } else {
        try {
          const n = await forgetAccounts(batch);
          await loadAccounts();
          // The forget killed Steam without relaunching it — re-probe.
          refreshSteamRunning();
          setSelMode(false);
          toast("", fmt(tNow("toastForgetN"), { n }));
        } catch (e) {
          toastError(e);
        }
      }
      return;
    }
    // ---- single ----
    const a = steamDeleteAccount;
    if (!a) return;
    focusSteamToolbarOnDeleteClose = true;
    if (steamDeleteMode === "hide") {
      if (!hidden.includes(a.accountName)) {
        hidden = [...hidden, a.accountName];
        saveHidden();
      }
      closeSteamDelete();
      toast("", fmt(tNow("toastHide"), { a: a.accountName }));
    } else {
      const name = a.accountName;
      closeSteamDelete();
      try {
        await forgetAccount(name);
        await loadAccounts();
        // The forget killed Steam without relaunching it — re-probe.
        refreshSteamRunning();
        toast("", fmt(tNow("toastForget"), { a: name }));
      } catch (e) {
        toastError(e);
      }
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !steamDeleteOpen && selMode) setSelMode(false);
  }

  function onFocus() {
    reloadAccountsQuietly();
  }

  let unlistenTray: UnlistenFn | undefined;

  onMount(() => {
    loadHidden();
    try {
      const v = localStorage.getItem("sm-view-steam");
      if (v === "card" || v === "list") view = v;
    } catch {
      /* ignore */
    }
    (async () => {
      try {
        installPath = await getInstallPath();
      } catch (e) {
        toastError(e);
      }
    })();
    reloadAccountsQuietly();
    window.addEventListener("focus", onFocus);
    window.addEventListener("keydown", onKeydown);
    // A tray quick-switch changes the active account while this window may
    // already be visible (no focus event fires) — re-read the list so the
    // active ring moves.
    listen("accounts-changed", () => {
      reloadAccountsQuietly();
    })
      .then((un) => {
        unlistenTray = un;
      })
      .catch(() => {
        /* event API unavailable; ignore */
      });
  });
  onDestroy(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("keydown", onKeydown);
    }
    unlistenTray?.();
  });
</script>

<Dialog.Root bind:open={steamDeleteOpen} onOpenChange={onSteamDeleteOpenChange}>
  <section class="page">
    <h2 class="page-title">
      <span>{$t("steamTitle")}</span>
      <span class="count">· {accounts.length}</span>
    </h2>
    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
    <p class="page-sub">{@html $t("steamSub")}</p>

    <Card.Root
      size="sm"
      class="mb-4 inline-flex max-w-full flex-row items-center gap-2 rounded-lg border border-border bg-muted px-2.5 py-1.5 font-mono text-[11.5px] text-muted-foreground shadow-inner"
    >
      <span class="shrink-0">{$t("installedAt")}</span>
      <b class="truncate font-semibold text-foreground" title={installPath}>{installPath}</b>
      {#if installPath}
        <Button
          variant="outline"
          size="xs"
          class="ml-1 shrink-0 font-mono"
          onclick={() => copyPath(installPath)}
        >
          <CopyIcon data-icon="inline-start" aria-hidden="true" />
          <span>{$t("copyBtn")}</span>
        </Button>
      {/if}
    </Card.Root>

    <div class="mb-4 flex flex-wrap items-center gap-2">
      <Button bind:ref={steamToolbarFocusTarget} variant="outline" onclick={refresh}>
        <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
        <span>{$t("refreshBtn")}</span>
      </Button>
      <Button variant="ghost" onclick={clearAutoLogin}>{$t("clearLogin")}</Button>
      <ToggleGroup.Root
        type="single"
        required
        value={view}
        variant="outline"
        aria-label="View"
        onValueChange={(value) =>
          (value === "list" || value === "card") && setView(value)}
      >
        <ToggleGroup.Item value="list" aria-label={$t("viewList")}>
          <ListIcon data-icon="inline-start" aria-hidden="true" />
          <span>{$t("viewList")}</span>
        </ToggleGroup.Item>
        <ToggleGroup.Item value="card" aria-label={$t("viewCards")}>
          <LayoutGridIcon data-icon="inline-start" aria-hidden="true" />
          <span>{$t("viewCards")}</span>
        </ToggleGroup.Item>
      </ToggleGroup.Root>
      <Button
        variant={selMode ? "secondary" : "outline"}
        aria-pressed={selMode}
        onclick={() => setSelMode(!selMode)}
      >{$t("select")}</Button>
      {#if hidden.length}
        <Button variant="link" size="sm" onclick={clearHidden}>
          {fmt($t("showHidden"), { n: hidden.length })}
        </Button>
      {/if}
      <span class="flex-1"></span>
      <div class="flex items-center gap-2">
        <Checkbox id="steam-offline" bind:checked={offline} />
        <Label for="steam-offline" class="text-[12.5px] text-foreground">
          {$t("offlineLabel")}
        </Label>
      </div>
    </div>

    {#if selMode}
      <Card.Root
        size="sm"
        class="mb-3.5 flex-row flex-wrap items-center gap-2 rounded-lg border border-dashed border-primary/45 bg-muted px-3 py-2"
      >
        <b class="text-[13px] text-foreground">
          {fmt($t("selCount"), { n: selected.size })}
        </b>
        <Button variant="ghost" size="sm" onclick={selectAll}>{$t("selAll")}</Button>
        <Button variant="ghost" size="sm" onclick={clearSel}>{$t("selNone")}</Button>
        <span class="flex-1"></span>
        <Dialog.Trigger disabled={selected.size === 0}>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="destructive"
              disabled={selected.size === 0}
              onclick={(event) => {
                openSteamDeleteBatch();
                activateDialogTrigger(props, event, "onclick");
              }}
              onkeydown={(event) => {
                if (event.key === "Enter" || event.key === " ") openSteamDeleteBatch();
                activateDialogTrigger(props, event, "onkeydown");
              }}
            >
              <Trash2Icon data-icon="inline-start" aria-hidden="true" />
              {$t("delBtn")}
            </Button>
          {/snippet}
        </Dialog.Trigger>
        <Button variant="outline" onclick={() => setSelMode(false)}>{$t("cancel")}</Button>
      </Card.Root>
    {/if}

    {#if view === "card"}
      <div class="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-2.5">
        {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.accountName))}
          {#if "single" in entry}
            {@render steamCard(entry.single, false)}
          {:else}
            {@const open = openFolders.has(entry.folder)}
            {@const col = hue(entry.folder)}
            <Card.Root
              class="cursor-pointer items-center gap-1.5 overflow-visible border px-2.5 py-4 text-center shadow-sm transition hover:-translate-y-px hover:shadow-md"
              style={`background: color-mix(in srgb, ${col} 16%, var(--win)); border-color: color-mix(in srgb, ${col} 40%, transparent);`}
              title={$t("folderTitle")}
              role="button"
              tabindex={0}
              onclick={() => toggleFolder(entry.folder)}
              onkeydown={(e) =>
                (e.key === "Enter" || e.key === " ") &&
                (e.preventDefault(), toggleFolder(entry.folder))}
            >
              <Avatar.Root class="size-14 rounded-xl shadow-md">
                <Avatar.Fallback
                  class="rounded-xl text-xl font-bold text-white"
                  style={`background: ${col}`}
                >{initial(entry.folder)}</Avatar.Fallback>
              </Avatar.Root>
              <div class="flex items-center gap-1 text-[13px] font-bold text-foreground">
                {#if open}
                  <ChevronDownIcon class="size-3.5" aria-hidden="true" />
                {:else}
                  <ChevronRightIcon class="size-3.5" aria-hidden="true" />
                {/if}
                <span class="break-all">{entry.folder}</span>
              </div>
              <span class="font-mono text-[10.5px] text-muted-foreground">
                {fmt($t("folderCount"), { n: entry.items.length })}
              </span>
            </Card.Root>
            {#if open}
              {#each entry.items as a (a.accountName)}
                {@render steamCard(a, true, col)}
              {/each}
            {/if}
          {/if}
        {/each}
      </div>
    {:else}
      <div class="flex flex-col gap-2">
        {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.accountName))}
          {#if "single" in entry}
            {@render steamRow(entry.single, false)}
          {:else}
            {@const open = openFolders.has(entry.folder)}
            <Card.Root
              class="cursor-pointer flex-row items-center gap-3 overflow-visible border border-[rgba(108,113,196,0.35)] bg-[linear-gradient(90deg,rgba(108,113,196,0.1),rgba(108,113,196,0.02)_50%),var(--surface)] px-3.5 py-3 transition hover:-translate-y-px hover:border-[rgba(108,113,196,0.6)] hover:shadow-md"
              title={$t("folderTitle")}
              role="button"
              tabindex={0}
              onclick={() => toggleFolder(entry.folder)}
              onkeydown={(e) =>
                (e.key === "Enter" || e.key === " ") &&
                (e.preventDefault(), toggleFolder(entry.folder))}
            >
              {#if open}
                <ChevronDownIcon class="size-3 text-[var(--violet)]" aria-hidden="true" />
              {:else}
                <ChevronRightIcon class="size-3 text-[var(--violet)]" aria-hidden="true" />
              {/if}
              <Avatar.Root class="size-10 rounded-xl shadow-md">
                <Avatar.Fallback class="rounded-xl bg-gradient-to-br from-[var(--violet)] to-[var(--blue)] text-[13.5px] font-bold text-white">
                  {initial(entry.folder)}
                </Avatar.Fallback>
              </Avatar.Root>
              <div class="min-w-0">
                <div class="truncate text-sm font-bold text-foreground">{entry.folder}</div>
                <div class="mt-px text-xs text-muted-foreground">
                  {fmt($t("folderCount"), { n: entry.items.length })}
                </div>
              </div>
              <Badge
                variant="secondary"
                class="ml-auto bg-[rgba(108,113,196,0.16)] text-[var(--violet)]"
              >
                <FolderIcon data-icon="inline-start" aria-hidden="true" />
                {fmt($t("folderCount"), { n: entry.items.length })}
              </Badge>
            </Card.Root>
            {#if open}
              {#each entry.items as a (a.accountName)}
                {@render steamRow(a, true)}
              {/each}
            {/if}
          {/if}
        {/each}
      </div>
    {/if}

    <div class="mt-4 flex flex-wrap gap-4 text-[11.5px] text-muted-foreground">
      <span class="inline-flex items-center gap-1.5">
        <i class="block size-2.5 rounded-[3px] bg-[var(--green)]"></i>
        <span>{$t("legendActive")}</span>
      </span>
      <span class="inline-flex items-center gap-1.5">
        <i class="block size-2.5 rounded-[3px] bg-[var(--violet)]"></i>
        <span>{$t("legendFolder")}</span>
      </span>
    </div>
  </section>

  {#if steamDeleteAccount || steamDeleteBatch}
    <Dialog.Content
      class="max-w-[min(460px,calc(100%-2rem))] gap-3 p-5"
      onCloseAutoFocus={onSteamDeleteCloseAutoFocus}
    >
      <Dialog.Header>
        <Dialog.Title>
          {#if steamDeleteBatch}
            {fmt($t("sdelTitleN"), { n: steamDeleteBatch.length })}
          {:else if steamDeleteAccount}
            {fmt($t("sdelTitle"), {
              a: steamDeleteAccount.personaName || steamDeleteAccount.accountName,
              p:
                $lang === "en"
                  ? ` (${steamDeleteAccount.accountName})`
                  : `（${steamDeleteAccount.accountName}）`,
            })}
          {/if}
        </Dialog.Title>
        <Dialog.Description class="sr-only">
          {steamDeleteBatch
            ? $t("sdelForgetDN")
            : steamDeleteAccount
              ? fmt($t("sdelForgetD"), { a: steamDeleteAccount.accountName })
              : ""}
        </Dialog.Description>
      </Dialog.Header>

      <RadioGroup.Root
        name="sdel-mode"
        value={steamDeleteMode}
        onValueChange={(value) => {
          if (value === "hide" || value === "forget") steamDeleteMode = value;
        }}
      >
        <Label
          for="sdel-hide"
          class="items-start rounded-lg border border-border p-3 transition-colors hover:border-primary/50 hover:bg-accent"
        >
          <RadioGroup.Item id="sdel-hide" value="hide" class="mt-0.5" />
          <span>
            <span class="block text-[12.5px] font-semibold text-foreground">
              {$t("sdelHideT")}
            </span>
            <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
              {$t("sdelHideD")}
            </span>
          </span>
        </Label>
        <Label
          for="sdel-forget"
          class="items-start rounded-lg border border-border p-3 transition-colors hover:border-destructive/45 hover:bg-destructive/5"
        >
          <RadioGroup.Item
            id="sdel-forget"
            value="forget"
            class="mt-0.5 data-checked:border-destructive data-checked:bg-destructive"
          />
          <span>
            <span class="block text-[12.5px] font-semibold text-foreground">
              {$t("sdelForgetT")}
            </span>
            <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
              {steamDeleteBatch
                ? $t("sdelForgetDN")
                : steamDeleteAccount
                  ? fmt($t("sdelForgetD"), { a: steamDeleteAccount.accountName })
                  : ""}
            </span>
          </span>
        </Label>
      </RadioGroup.Root>

      <Dialog.Footer class="-mx-5 -mb-5 mt-1 p-4">
        <Dialog.Close>
          {#snippet child({ props })}
            <Button {...props} variant="outline">{$t("cancel")}</Button>
          {/snippet}
        </Dialog.Close>
        <Button variant="destructive" onclick={confirmSteamDelete}>
          <Trash2Icon data-icon="inline-start" aria-hidden="true" />
          {$t("removeBtn")}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  {/if}
</Dialog.Root>

{#snippet steamRow(a: SteamAccount, child: boolean)}
  {@const uri = $avatars[a.steamId64]}
  {@const picked = selected.has(a.accountName)}
  {@const active = a.mostRecent && $steamRunning}
  <Card.Root
    class={cn(
      "relative flex-row items-center gap-3 overflow-visible border border-border px-3.5 py-3 shadow-sm transition hover:-translate-y-px hover:border-primary/45 hover:shadow-md",
      active &&
        "border-[rgba(133,153,0,0.5)] bg-[linear-gradient(90deg,rgba(133,153,0,0.1),rgba(133,153,0,0.03)_40%),var(--win)] before:absolute before:inset-y-2 before:left-0 before:w-[3px] before:rounded-r-sm before:bg-[var(--green)]",
      child &&
        "ml-[34px] after:absolute after:-left-[19px] after:top-1/2 after:h-px after:w-[13px] after:bg-border",
      !active && !selMode && "cursor-pointer select-none",
      selMode && picked && "border-primary ring-2 ring-primary/25",
    )}
    title={selMode ? undefined : active ? undefined : $t("rowTitle")}
    ondblclick={() => switchTo(a)}
  >
    {#if selMode}
      <Checkbox
        checked={picked}
        aria-label={a.accountName}
        onCheckedChange={(checked) => checked !== picked && toggleSel(a.accountName)}
      />
    {/if}
    <Avatar.Root class="size-10 rounded-xl shadow-md">
      {#if uri}
        <Avatar.Image class="rounded-xl" src={uri} alt="" />
      {/if}
      <Avatar.Fallback
        class="rounded-xl font-bold text-white"
        style={`background: ${hue(a.accountName)}`}
      >{initial(a.personaName)}</Avatar.Fallback>
    </Avatar.Root>
    <div class="min-w-0">
      <div class="truncate text-sm font-bold text-foreground">{a.personaName}</div>
      <div class="mt-px truncate text-xs text-muted-foreground">{a.accountName}</div>
      <div class="mt-1 truncate font-mono text-[11px] text-muted-foreground">
        {a.steamId64}
      </div>
    </div>
    <div class="ml-auto flex items-center gap-2.5">
      {#if active}
        <Badge class="bg-[rgba(133,153,0,0.16)] text-[var(--green)] ring-1 ring-[rgba(133,153,0,0.25)]">
          {$t("activePill")}
        </Badge>
      {:else}
        <Badge variant="secondary" class="text-muted-foreground">{$t("dblPill")}</Badge>
      {/if}
      {#if !selMode}
        <Dialog.Trigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="destructive"
              onclick={(event) => {
                event.stopPropagation();
                openSteamDelete(a);
                activateDialogTrigger(props, event, "onclick");
              }}
              onkeydown={(event) => {
                if (event.key === "Enter" || event.key === " ") openSteamDelete(a);
                activateDialogTrigger(props, event, "onkeydown");
              }}
            >{$t("delBtn")}</Button>
          {/snippet}
        </Dialog.Trigger>
      {/if}
    </div>
  </Card.Root>
{/snippet}

{#snippet steamCard(a: SteamAccount, child: boolean, folderColor?: string)}
  {@const uri = $avatars[a.steamId64]}
  {@const picked = selected.has(a.accountName)}
  {@const active = a.mostRecent && $steamRunning}
  <Card.Root
    class={cn(
      "relative items-center gap-1.5 overflow-visible border border-border px-2.5 py-4 text-center shadow-sm transition hover:-translate-y-px hover:border-primary/45 hover:shadow-md",
      active && "border-[rgba(133,153,0,0.55)] ring-2 ring-[rgba(133,153,0,0.22)]",
      child && "border-[color:color-mix(in_srgb,var(--folder-color)_30%,transparent)] bg-[color:color-mix(in_srgb,var(--folder-color)_10%,var(--win))]",
      !active && !selMode && "cursor-pointer select-none",
      selMode && picked && "border-primary ring-2 ring-primary/25",
    )}
    style={folderColor ? `--folder-color: ${folderColor}` : undefined}
    title={selMode ? undefined : active ? undefined : $t("rowTitle")}
    ondblclick={() => switchTo(a)}
  >
    {#if selMode}
      <Checkbox
        class="absolute top-2 left-2"
        checked={picked}
        aria-label={a.accountName}
        onCheckedChange={(checked) => checked !== picked && toggleSel(a.accountName)}
      />
    {:else}
      <Dialog.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon-xs"
            class="absolute top-1.5 right-1.5 text-muted-foreground"
            title={$t("delBtn")}
            aria-label={$t("delBtn")}
            onclick={(event) => {
              event.stopPropagation();
              openSteamDelete(a);
              activateDialogTrigger(props, event, "onclick");
            }}
            onkeydown={(event) => {
              if (event.key === "Enter" || event.key === " ") openSteamDelete(a);
              activateDialogTrigger(props, event, "onkeydown");
            }}
          >
            <EllipsisIcon aria-hidden="true" />
          </Button>
        {/snippet}
      </Dialog.Trigger>
    {/if}
    <Avatar.Root class="size-14 rounded-xl shadow-md">
      {#if uri}
        <Avatar.Image class="rounded-xl" src={uri} alt="" />
      {/if}
      <Avatar.Fallback
        class="rounded-xl text-xl font-bold text-white"
        style={`background: ${hue(a.accountName)}`}
      >{initial(a.personaName)}</Avatar.Fallback>
    </Avatar.Root>
    <div class="break-all text-[13px] leading-tight font-bold text-foreground">
      {a.personaName}
    </div>
    <div class="break-all font-mono text-[10.5px] text-muted-foreground">
      {a.accountName}
    </div>
    {#if active}
      <Badge class="bg-[rgba(133,153,0,0.16)] text-[var(--green)] ring-1 ring-[rgba(133,153,0,0.25)]">
        {$t("activePill")}
      </Badge>
    {/if}
  </Card.Root>
{/snippet}
