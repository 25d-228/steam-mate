<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { copyText } from "$lib/clipboard";
  import {
    getInstallPath,
    listAccounts,
    clearLogin,
    switchAccount,
    forgetAccount,
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

  async function relist() {
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
    const off = offline;
    const disp = accountLabel($lang, a.personaName, a.accountName);
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
      await switchAccount(a.accountName, off);
      await loadAccounts();
      // The switch just relaunched Steam — re-probe so the chip and tray flip
      // to "signed in" without waiting for the next interval tick.
      refreshSteamRunning();
      toast(
        "",
        fmt(tNow(launching ? "toastLaunch2" : "toastSwitch2"), {
          a: disp,
          off: off ? tNow("offlineSuffix") : "",
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
  let sdelAccount = $state<SteamAccount | null>(null);
  let sdelBatch = $state<string[] | null>(null);
  let sdelMode = $state<"hide" | "forget">("hide");

  function openSDelete(a: SteamAccount) {
    sdelAccount = a;
    sdelBatch = null;
    sdelMode = "hide";
  }
  function openSDeleteBatch() {
    if (selected.size === 0) return;
    sdelBatch = [...selected];
    sdelAccount = null;
    sdelMode = "hide";
  }
  function closeSDelete() {
    sdelAccount = null;
    sdelBatch = null;
  }

  async function confirmSDelete() {
    // ---- batch ----
    if (sdelBatch) {
      const batch = sdelBatch;
      const mode = sdelMode;
      closeSDelete();
      if (mode === "hide") {
        const set = new Set(hidden);
        for (const name of batch) set.add(name);
        hidden = [...set];
        saveHidden();
        setSelMode(false);
        toast("", fmt(tNow("toastHideN"), { n: batch.length }));
      } else {
        try {
          const n = await invoke<number>("steam_forget_accounts", {
            accountNames: batch,
          });
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
    const a = sdelAccount;
    if (!a) return;
    if (sdelMode === "hide") {
      if (!hidden.includes(a.accountName)) {
        hidden = [...hidden, a.accountName];
        saveHidden();
      }
      closeSDelete();
      toast("", fmt(tNow("toastHide"), { a: a.accountName }));
    } else {
      const name = a.accountName;
      closeSDelete();
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
    if (e.key === "Escape") {
      if (sdelAccount || sdelBatch) closeSDelete();
      else if (selMode) setSelMode(false);
    }
  }

  function onFocus() {
    relist();
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
    relist();
    window.addEventListener("focus", onFocus);
    window.addEventListener("keydown", onKeydown);
    // A tray quick-switch changes the active account while this window may
    // already be visible (no focus event fires) — re-read the list so the
    // active ring moves.
    listen("accounts-changed", () => {
      relist();
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

<section class="page">
  <h2 class="page-title">
    <span>{$t("steamTitle")}</span>
    <span class="count">· {accounts.length}</span>
  </h2>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  <p class="page-sub">{@html $t("steamSub")}</p>

  <div class="pathline">
    <span>{$t("installedAt")}</span>
    <b>{installPath}</b>
    {#if installPath}
      <button class="copy" onclick={() => copyPath(installPath)}>
        <svg viewBox="0 0 24 24"
          ><path
            d="M16 1H4a2 2 0 00-2 2v14h2V3h12zm3 4H8a2 2 0 00-2 2v14a2 2 0 002 2h11a2 2 0 002-2V7a2 2 0 00-2-2zm0 16H8V7h11z"
          /></svg
        ><span>{$t("copyBtn")}</span>
      </button>
    {/if}
  </div>

  <div class="toolbar">
    <button class="btn" onclick={refresh}>
      <span class="ico"
        ><svg viewBox="0 0 24 24"
          ><path
            d="M17.65 6.35A8 8 0 1020 12h-2a6 6 0 11-1.76-4.24L13 11h7V4z"
          /></svg
        ></span
      >
      <span>{$t("refreshBtn")}</span>
    </button>
    <button class="btn ghost" onclick={clearAutoLogin}>{$t("clearLogin")}</button>
    <span class="seg" role="group" aria-label="View">
      <button class:active={view === "list"} onclick={() => setView("list")}
        >☰ <span>{$t("viewList")}</span></button
      >
      <button class:active={view === "card"} onclick={() => setView("card")}
        >▦ <span>{$t("viewCards")}</span></button
      >
    </span>
    <button
      class="btn"
      class:sel-active={selMode}
      onclick={() => setSelMode(!selMode)}>{$t("select")}</button
    >
    {#if hidden.length}
      <button class="link" onclick={clearHidden}
        >{fmt($t("showHidden"), { n: hidden.length })}</button
      >
    {/if}
    <span class="spacer"></span>
    <label class="check">
      <input type="checkbox" bind:checked={offline} />
      <span>{$t("offlineLabel")}</span>
    </label>
  </div>

  {#if selMode}
    <div class="batchbar">
      <b>{fmt($t("selCount"), { n: selected.size })}</b>
      <button class="btn ghost" onclick={selectAll}>{$t("selAll")}</button>
      <button class="btn ghost" onclick={clearSel}>{$t("selNone")}</button>
      <span class="spacer"></span>
      <button
        class="btn danger"
        disabled={selected.size === 0}
        onclick={openSDeleteBatch}>{$t("delBtn")}</button
      >
      <button class="btn" onclick={() => setSelMode(false)}>{$t("cancel")}</button>
    </div>
  {/if}

  {#if view === "card"}
    <div class="grid">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.accountName))}
        {#if "single" in entry}
          {@render steamCard(entry.single, false)}
        {:else}
          {@const open = openFolders.has(entry.folder)}
          {@const col = hue(entry.folder)}
          <div
            class="card folder-card"
            style="--fc:{col}"
            title={$t("folderTitle")}
            role="button"
            tabindex="0"
            onclick={() => toggleFolder(entry.folder)}
            onkeydown={(e) =>
              (e.key === "Enter" || e.key === " ") &&
              (e.preventDefault(), toggleFolder(entry.folder))}
          >
            <div class="cav" style="background:{col}">
              {initial(entry.folder)}
            </div>
            <div class="cname">
              {open ? "▾" : "▸"}
              {entry.folder}
            </div>
            <div class="csub">
              {fmt($t("folderCount"), { n: entry.items.length })}
            </div>
          </div>
          {#if open}
            {#each entry.items as a (a.accountName)}
              {@render steamCard(a, true, col)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {:else}
    <div class="list">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.accountName))}
        {#if "single" in entry}
          {@render steamRow(entry.single, false)}
        {:else}
          {@const open = openFolders.has(entry.folder)}
          <div
            class="row folder"
            title={$t("folderTitle")}
            role="button"
            tabindex="0"
            onclick={() => toggleFolder(entry.folder)}
            onkeydown={(e) =>
              (e.key === "Enter" || e.key === " ") &&
              (e.preventDefault(), toggleFolder(entry.folder))}
          >
            <span class="chev">{open ? "▼" : "▶"}</span>
            <div class="av stack">{initial(entry.folder)}</div>
            <div class="who">
              <div class="acct">{entry.folder}</div>
              <div class="persona">
                {fmt($t("folderCount"), { n: entry.items.length })}
              </div>
            </div>
            <div class="end">
              <span class="pill folder"
                >{fmt($t("folderCount"), { n: entry.items.length })}</span
              >
            </div>
          </div>
          {#if open}
            {#each entry.items as a (a.accountName)}
              {@render steamRow(a, true)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {/if}

  <div class="legend">
    <span><i style="background:var(--green)"></i> <span>{$t("legendActive")}</span></span>
    <span><i style="background:var(--violet)"></i> <span>{$t("legendFolder")}</span></span>
  </div>
</section>

{#snippet steamRow(a: SteamAccount, child: boolean)}
  {@const uri = $avatars[a.steamId64]}
  {@const picked = selected.has(a.accountName)}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="row"
    class:is-active={a.mostRecent && $steamRunning}
    class:child
    class:can-switch={(!a.mostRecent || !$steamRunning) && !selMode}
    class:selectable={selMode}
    class:selected={selMode && picked}
    title={selMode ? undefined : a.mostRecent && $steamRunning ? undefined : $t("rowTitle")}
    role={selMode ? "button" : a.mostRecent && $steamRunning ? undefined : "button"}
    tabindex={selMode ? 0 : a.mostRecent && $steamRunning ? undefined : 0}
    onclick={() => selMode && toggleSel(a.accountName)}
    onkeydown={(e) =>
      selMode &&
      (e.key === "Enter" || e.key === " ") &&
      (e.preventDefault(), toggleSel(a.accountName))}
    ondblclick={() => switchTo(a)}
  >
    {#if selMode}
      <span class="selbox">✓</span>
    {/if}
    <div class="av" style="background:{hue(a.accountName)}">
      {#if uri}
        <img src={uri} alt="" />
      {/if}
      {initial(a.personaName)}
    </div>
    <div class="who">
      <div class="acct">{a.personaName}</div>
      <div class="persona">{a.accountName}</div>
      <div class="meta">{a.steamId64}</div>
    </div>
    <div class="end">
      {#if a.mostRecent && $steamRunning}
        <span class="pill active">{$t("activePill")}</span>
      {:else}
        <span class="pill muted">{$t("dblPill")}</span>
      {/if}
      {#if !selMode}
        <button
          class="btn danger-line"
          onclick={(e) => {
            e.stopPropagation();
            openSDelete(a);
          }}>{$t("delBtn")}</button
        >
      {/if}
    </div>
  </div>
{/snippet}

{#snippet steamCard(a: SteamAccount, child: boolean, fc?: string)}
  {@const uri = $avatars[a.steamId64]}
  {@const picked = selected.has(a.accountName)}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="card"
    class:is-active={a.mostRecent && $steamRunning}
    class:child-card={child}
    class:can-switch={(!a.mostRecent || !$steamRunning) && !selMode}
    class:selectable={selMode}
    class:selected={selMode && picked}
    style={fc ? `--fc:${fc}` : undefined}
    title={selMode ? undefined : a.mostRecent && $steamRunning ? undefined : $t("rowTitle")}
    role={selMode || !a.mostRecent || !$steamRunning ? "button" : undefined}
    tabindex={selMode || !a.mostRecent || !$steamRunning ? 0 : undefined}
    onclick={() => selMode && toggleSel(a.accountName)}
    onkeydown={(e) =>
      selMode &&
      (e.key === "Enter" || e.key === " ") &&
      (e.preventDefault(), toggleSel(a.accountName))}
    ondblclick={() => switchTo(a)}
  >
    {#if selMode}
      <span class="selbox">✓</span>
    {:else}
      <button
        class="more"
        title={$t("delBtn")}
        onclick={(e) => {
          e.stopPropagation();
          openSDelete(a);
        }}>⋯</button
      >
    {/if}
    <div class="cav" style="background:{hue(a.accountName)}">
      {#if uri}
        <img src={uri} alt="" />
      {/if}
      {initial(a.personaName)}
    </div>
    <div class="cname">{a.personaName}</div>
    <div class="csub">{a.accountName}</div>
    {#if a.mostRecent && $steamRunning}
      <span class="pill active">{$t("activePill")}</span>
    {/if}
  </div>
{/snippet}

{#if sdelAccount || sdelBatch}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeSDelete()}
  >
    <div class="modal" role="dialog" aria-modal="true">
      {#if sdelBatch}
        <h3>{fmt($t("sdelTitleN"), { n: sdelBatch.length })}</h3>
        <label class="opt">
          <input type="radio" name="sdel-mode" value="hide" bind:group={sdelMode} />
          <span>
            <div class="ot">{$t("sdelHideT")}</div>
            <div class="od">{$t("sdelHideD")}</div>
          </span>
        </label>
        <label class="opt danger">
          <input type="radio" name="sdel-mode" value="forget" bind:group={sdelMode} />
          <span>
            <div class="ot">{$t("sdelForgetT")}</div>
            <div class="od">{$t("sdelForgetDN")}</div>
          </span>
        </label>
      {:else if sdelAccount}
        {@const a = sdelAccount}
        <h3>
          {fmt($t("sdelTitle"), {
            a: a.personaName || a.accountName,
            p:
              $lang === "en"
                ? ` (${a.accountName})`
                : `（${a.accountName}）`,
          })}
        </h3>
        <label class="opt">
          <input type="radio" name="sdel-mode" value="hide" bind:group={sdelMode} />
          <span>
            <div class="ot">{$t("sdelHideT")}</div>
            <div class="od">{$t("sdelHideD")}</div>
          </span>
        </label>
        <label class="opt danger">
          <input type="radio" name="sdel-mode" value="forget" bind:group={sdelMode} />
          <span>
            <div class="ot">{$t("sdelForgetT")}</div>
            <div class="od">{fmt($t("sdelForgetD"), { a: a.accountName })}</div>
          </span>
        </label>
      {/if}
      <div class="actions">
        <span class="spacer"></span>
        <button class="btn" onclick={closeSDelete}>{$t("cancel")}</button>
        <button class="btn danger" onclick={confirmSDelete}>{$t("removeBtn")}</button>
      </div>
    </div>
  </div>
{/if}
