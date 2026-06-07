<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { copyText } from "$lib/clipboard";
  import { forgetAccount } from "$lib/api/steam";
  import * as md from "$lib/api/games/master-duel";
  import type { MdAccount, SteamAccount } from "$lib/api/types";
  import { asAppError } from "$lib/api/types";
  import { save } from "@tauri-apps/plugin-dialog";
  import { t, fmt, lang, tNow, accountLabel } from "$lib/i18n";
  import { toast } from "$lib/toast";
  import { toastError } from "$lib/errors";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import {
    steamAccounts,
    ensureSteamAccounts,
    refreshSteamAccounts,
    steamByLogin,
    steamRunning,
    refreshSteamRunning,
  } from "$lib/stores/steam";

  // ---- avatar fallback tile (colored initials) ----
  const AV_COLORS = [
    "#268bd2",
    "#2aa198",
    "#6c71c4",
    "#b58900",
    "#d33682",
    "#cb4b16",
  ];
  function initial(s: string): string {
    return (s.trim()[0] || "?").toUpperCase();
  }
  function hue(s: string): string {
    let h = 0;
    for (const c of s) h = (h * 31 + (c.codePointAt(0) ?? 0)) % AV_COLORS.length;
    return AV_COLORS[h];
  }

  type Sort = "unlinked" | "added" | "alpha";
  type View = "list" | "card";

  // A seed candidate from the backend: a profile that holds its own (un-shared)
  // copy of the cache, with its size. Used by the create-shared-cache flow's
  // "move an existing cache here" picker.
  type SeedCandidate = {
    folderId: string;
    accountName: string;
    sizeBytes: number;
  };

  let installPath = $state<string>("");
  let accounts = $state<MdAccount[]>([]);
  let order = $state<string[]>([]); // disk order, for "recently added"
  let sort = $state<Sort>("unlinked");
  let cacheBytes = $state<number | null>(null);
  // null = not yet checked; true/false = the cache exists or not.
  let cacheExists = $state<boolean | null>(null);
  let running = $state(false);
  let openFolders = $state(new Set<string>());

  // view mode (list / cards), remembered per page (localStorage sm-view-md)
  let view = $state<View>("list");

  let runningTimer: ReturnType<typeof setInterval> | undefined;

  // login (accountName) → Steam account, for the current list. A profile's
  // stored steamLogin resolves through this: present → that account (avatar,
  // selector value, forget option); absent → unmatched. No name matching.
  const byLogin = $derived.by<Map<string, SteamAccount>>(() => {
    const map = new Map<string, SteamAccount>();
    for (const s of $steamAccounts) map.set(s.accountName, s);
    return map;
  });

  /** The current Steam account a profile is assigned to, or null when unmatched. */
  function assignedSteam(a: MdAccount): SteamAccount | null {
    return (a.steamLogin && byLogin.get(a.steamLogin)) || null;
  }

  const cacheGb = $derived(
    cacheBytes == null
      ? "—"
      : `${(cacheBytes / 1024 / 1024 / 1024).toFixed(1)} GB`,
  );

  function idx(a: MdAccount): number {
    const i = order.indexOf(a.folderId);
    return i < 0 ? 0 : i;
  }

  function comparator(a: MdAccount, b: MdAccount): number {
    if (sort === "added") return idx(b) - idx(a);
    if (sort === "alpha")
      return (a.accountName || "").localeCompare(b.accountName || "") || idx(a) - idx(b);
    // unlinked first (default)
    return (
      ((a.isLinked ? 1 : 0) - (b.isLinked ? 1 : 0)) || idx(a) - idx(b)
    );
  }

  // ---- same-name grouping (group by accountName when count > 1) ----
  type Entry = { single: MdAccount } | { folder: string; items: MdAccount[] };

  const entries = $derived.by<Entry[]>(() => {
    const items = accounts.slice().sort(comparator);
    const groups = new Map<string, MdAccount[]>();
    for (const a of items) {
      const n = a.accountName || "";
      if (!groups.has(n)) groups.set(n, []);
      groups.get(n)!.push(a);
    }
    const out: Entry[] = [];
    const seen = new Set<string>();
    for (const a of items) {
      const n = a.accountName || "";
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

  function setView(v: View) {
    view = v;
    if (typeof localStorage !== "undefined")
      localStorage.setItem("sm-view-md", v);
    // leaving card/list keeps the folder open-state (shared across views)
  }

  async function loadAccounts() {
    accounts = await md.listAccounts();
    // disk order only grows so "recently added" stays stable across re-lists
    for (const a of accounts)
      if (!order.includes(a.folderId)) order = [...order, a.folderId];
  }

  // ---- shared-cache existence (drives the empty state + create flow) ----
  async function checkCacheExists(): Promise<boolean> {
    try {
      cacheExists = await invoke<boolean>("md_cache_exists");
    } catch {
      // leave previous value; an unknown state defaults to "exists" so the
      // normal cache box stays rather than offering a create flow on a fluke
      if (cacheExists == null) cacheExists = true;
    }
    return cacheExists ?? true;
  }

  // Fetch avatars for accounts assigned to a current Steam login. Reactive so it
  // also runs once the Steam list (and thus byLogin) arrives.
  $effect(() => {
    for (const a of accounts) {
      const s = assignedSteam(a);
      if (s) fetchAvatar(s.steamId64);
    }
  });

  // Auto-assign once: after both lists load, give each unassigned, named profile
  // the Steam login whose personaName matches it EXACTLY ONCE. Ambiguous names
  // (many 烙印 folders, several 烙印 logins) stay unassigned. Idempotent — once a
  // profile carries a steamLogin it is skipped, so re-lists don't reassign.
  let autoAssigned = false;
  $effect(() => {
    if (autoAssigned) return;
    const steam = $steamAccounts;
    if (!accounts.length || !steam.length) return;
    autoAssigned = true;
    (async () => {
      for (const a of accounts) {
        if (a.steamLogin || !a.accountName) continue;
        const matches = steam.filter((s) => s.personaName === a.accountName);
        if (matches.length !== 1) continue;
        try {
          await md.assignSteam(a.folderId, matches[0].accountName);
          accounts = accounts.map((x) =>
            x.folderId === a.folderId
              ? { ...x, steamLogin: matches[0].accountName }
              : x,
          );
        } catch {
          // a failed auto-assign just leaves the row unmatched; user can pick
        }
      }
    })();
  });

  // On a Steam account change: drop the assignment locally for any profile whose
  // selector value is the chosen login, then write it through the backend.
  async function assignSteam(a: MdAccount, login: string) {
    try {
      await md.assignSteam(a.folderId, login);
      accounts = accounts.map((x) =>
        x.folderId === a.folderId ? { ...x, steamLogin: login } : x,
      );
      toast(tNow("toastMeta1"), tNow("toastMeta2"));
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

  async function refresh() {
    try {
      await loadAccounts();
      await checkCacheExists();
      toast(tNow("mdRefresh1"), fmt(tNow("mdRefresh2"), { n: accounts.length }));
    } catch (e) {
      toastError(e);
    }
  }

  async function checkRunning(): Promise<boolean> {
    try {
      running = await md.isRunning();
    } catch {
      // leave previous value
    }
    return running;
  }

  // ---- inline rename ----
  let editingId = $state<string | null>(null);
  let editValue = $state("");

  async function startEdit(a: MdAccount) {
    if (running || selMode) return;
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    editingId = a.folderId;
    editValue = a.accountName;
  }
  function cancelEdit() {
    editingId = null;
  }
  async function commitEdit(a: MdAccount) {
    if (editingId !== a.folderId) return;
    const name = editValue.trim();
    editingId = null;
    if (name === a.accountName) return;
    try {
      await md.saveMetadata(a.folderId, name);
      await loadAccounts();
      if (name) toast(tNow("toastMeta1"), tNow("toastMeta2"));
    } catch (e) {
      toastError(e);
    }
  }

  // ---- link toggle ----
  async function toggleLink(a: MdAccount, wantLinked: boolean) {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    // No shared cache to link to — refuse and tell the user to create one first.
    if (wantLinked && cacheExists === false) {
      toast("", tNow("errNoCache"), true);
      await loadAccounts();
      return;
    }
    if (wantLinked) {
      try {
        await md.linkAccount(a.folderId);
        await afterLink(a, true);
      } catch (e) {
        const err = asAppError(e);
        if (err.kind === "JunctionFailed" && /file/i.test(err.msg ?? "")) {
          const ok = confirm(fmt(tNow("confirmForce"), { id: a.folderId }));
          if (!ok) {
            await loadAccounts();
            return;
          }
          try {
            await md.linkAccount(a.folderId, true);
            await afterLink(a, true);
          } catch (e2) {
            toastError(e2);
            await loadAccounts();
          }
        } else {
          toastError(e);
          await loadAccounts();
        }
      }
    } else {
      try {
        await md.unlinkAccount(a.folderId);
        await afterLink(a, false);
      } catch (e) {
        toastError(e);
        await loadAccounts();
      }
    }
  }

  async function afterLink(a: MdAccount, linked: boolean) {
    await loadAccounts();
    const n = a.accountName || a.folderId;
    toast(
      linked ? tNow("toastLink1") : tNow("toastUnlink1"),
      fmt(linked ? tNow("toastLink2") : tNow("toastUnlink2"), { n }),
    );
  }

  // ---- link / unlink all ----
  async function linkAll() {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    if (cacheExists === false) {
      toast("", tNow("errNoCache"), true);
      return;
    }
    try {
      const { linked, skipped } = await md.linkAll();
      await loadAccounts();
      toast(
        tNow("toastLink1"),
        skipped > 0
          ? fmt(tNow("toastLinkedAllSkipped"), { n: linked, k: skipped })
          : fmt(tNow("toastLinkedAll"), { n: linked }),
      );
    } catch (e) {
      toastError(e);
      await loadAccounts();
    }
  }

  async function unlinkAll() {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    try {
      const n = await md.unlinkAll();
      await loadAccounts();
      toast(tNow("toastUnlink1"), fmt(tNow("toastUnlinkedAll"), { n }));
    } catch (e) {
      toastError(e);
      await loadAccounts();
    }
  }

  // ---- delete dialog (single) ----
  let delAccount = $state<MdAccount | null>(null);
  let delAlsoSteam = $state(false);
  let delSteam = $state<SteamAccount | null>(null);

  async function openDelete(a: MdAccount) {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    delAccount = a;
    delAlsoSteam = false;
    // Only offer "also forget" when the assignment resolves to a real current
    // Steam account; an unassigned or stale login shows no checkbox.
    delSteam = steamByLogin($steamAccounts, a.steamLogin) ?? null;
  }
  function closeDelete() {
    delAccount = null;
    delSteam = null;
  }
  async function confirmDelete() {
    const a = delAccount;
    if (!a) return;
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    const n = a.accountName || a.folderId;
    const both = delAlsoSteam && delSteam;
    const steamName = delSteam?.accountName;
    closeDelete();
    try {
      // Do the recoverable Steam forget FIRST: if it fails (e.g. Steam not
      // installed), the irreversible MD delete below never runs, so we never
      // wipe a save folder under an error banner. If forget succeeds but the MD
      // delete then fails, only a re-addable Steam login was removed.
      if (both && steamName) {
        await forgetAccount(steamName);
        // refresh the shared Steam store so the persona/avatar map updates,
        // and re-probe running state — the forget killed Steam without
        // relaunching it.
        await refreshSteamAccounts().catch(() => {});
        refreshSteamRunning();
      }
      await md.deleteAccount(a.folderId);
      await loadAccounts();
      toast(
        fmt(tNow("toastDel1"), { n }),
        fmt(both ? tNow("toastDelBoth") : tNow("toastDelDone"), { n }),
      );
    } catch (e) {
      toastError(e);
      await loadAccounts();
    }
  }

  // ---- batch selection ----
  let selMode = $state(false);
  let selected = $state(new Set<string>());

  function setSelMode(on: boolean) {
    selMode = on;
    selected = new Set();
    if (on && editingId) editingId = null;
  }
  function toggleSel(folderId: string) {
    const next = new Set(selected);
    if (next.has(folderId)) next.delete(folderId);
    else next.add(folderId);
    selected = next;
  }
  function selectAll() {
    // Only what is actually on screen: singles plus members of expanded
    // folders — a collapsed folder's members never show a checkbox, so a
    // "select all" must not sweep them into an irreversible delete.
    const keys: string[] = [];
    for (const e of entries) {
      if ("single" in e) keys.push(e.single.folderId);
      else if (openFolders.has(e.folder))
        for (const a of e.items) keys.push(a.folderId);
    }
    selected = new Set(keys);
  }
  function clearSel() {
    selected = new Set();
  }

  // The selected profiles, in the current list's display order (so a batch
  // delete reads the same way the list does).
  const selectedAccounts = $derived(
    accounts.slice().sort(comparator).filter((a) => selected.has(a.folderId)),
  );

  // Distinct, still-existing Steam logins behind the current selection — drives
  // the batch delete's "also forget" checkbox and its {k} count.
  const selectedLogins = $derived.by<string[]>(() => {
    const out: string[] = [];
    const seen = new Set<string>();
    for (const a of selectedAccounts) {
      const s = steamByLogin($steamAccounts, a.steamLogin);
      if (s && !seen.has(s.accountName)) {
        seen.add(s.accountName);
        out.push(s.accountName);
      }
    }
    return out;
  });

  // ---- batch delete dialog ----
  let delBatch = $state<MdAccount[] | null>(null);
  let delBatchLogins = $state<string[]>([]);
  let delBatchAlsoSteam = $state(false);

  async function openDeleteBatch() {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    if (selected.size === 0) return;
    delBatch = selectedAccounts;
    delBatchLogins = selectedLogins;
    delBatchAlsoSteam = false;
  }
  function closeDeleteBatch() {
    delBatch = null;
    delBatchLogins = [];
  }
  async function confirmDeleteBatch() {
    const items = delBatch;
    if (!items || !items.length) return;
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    const logins = delBatchAlsoSteam ? delBatchLogins.slice() : [];
    closeDeleteBatch();
    try {
      // Steam-forget runs FIRST (one invoke for all distinct logins), then the
      // irreversible per-profile MD delete loop — the same ordering the single
      // delete uses, so a forget failure never wipes a save folder.
      if (logins.length) {
        await invoke<void>("steam_forget_accounts", { accountNames: logins });
        // The batch forget killed Steam without relaunching it — refresh both
        // the account map and the running probe.
        await refreshSteamAccounts().catch(() => {});
        refreshSteamRunning();
      }
      // One backend call for the whole batch: one running check, one install
      // resolve, and a profile that fails is skipped and reported instead of
      // stranding the rest half-deleted.
      const res = await invoke<{ deleted: number; failed: string[] }>(
        "md_delete_accounts",
        { folderIds: items.map((a) => a.folderId) },
      );
      await loadAccounts();
      if (res.failed.length) {
        toast("", fmt(tNow("errBatchSkipped"), { n: res.failed.length }), true);
      } else {
        toast(
          fmt(tNow("toastDelDoneN"), { n: res.deleted }),
          logins.length ? fmt(tNow("toastDelBothN"), { k: logins.length }) : "",
        );
      }
    } catch (e) {
      toastError(e);
      await loadAccounts();
    } finally {
      setSelMode(false);
    }
  }

  // ---- create shared cache flow ----
  let createOpen = $state(false);
  let createMode = $state<"seed" | "empty">("seed");
  let createSeedId = $state<string>("");
  let seedCandidates = $state<SeedCandidate[]>([]);

  function gb(bytes: number): string {
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  }

  async function openCreate() {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    try {
      const cands = await invoke<SeedCandidate[]>("md_seed_candidates");
      // largest-first; defensive even if the backend already sorts
      seedCandidates = cands.slice().sort((a, b) => b.sizeBytes - a.sizeBytes);
    } catch (e) {
      toastError(e);
      seedCandidates = [];
    }
    if (seedCandidates.length) {
      createMode = "seed";
      createSeedId = seedCandidates[0].folderId; // preselect the largest
    } else {
      // no candidates: the whole seed option is disabled, fall to "empty"
      createMode = "empty";
      createSeedId = "";
    }
    createOpen = true;
  }
  function closeCreate() {
    createOpen = false;
  }
  async function confirmCreate() {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return;
    }
    const seed = createMode === "seed" ? createSeedId || null : null;
    const seedName =
      seed != null
        ? (() => {
            const c = seedCandidates.find((x) => x.folderId === seed);
            return c ? c.accountName || c.folderId : seed;
          })()
        : "";
    closeCreate();
    try {
      await invoke<void>("md_create_cache", { seed });
      await checkCacheExists();
      // the seed profile becomes linked; re-list and refresh the size
      await loadAccounts();
      try {
        cacheBytes = await md.cacheSize();
      } catch {
        // size is best-effort; the box still shows "—"
      }
      toast(
        tNow("toastCreate1"),
        seed != null
          ? fmt(tNow("toastCreateSeed"), { n: seedName })
          : tNow("toastCreateEmpty"),
      );
    } catch (e) {
      toastError(e);
      await checkCacheExists();
      await loadAccounts();
    }
  }

  // ---- reveal shared cache in File Explorer ----
  async function revealCache() {
    try {
      await invoke<void>("md_reveal_cache");
    } catch (e) {
      toastError(e);
    }
  }

  // ---- copy the install path (exact, two-space form) ----
  // The rendered <b> label collapses runs of spaces, so hand-selecting it yields
  // "Yu-Gi-Oh! Master Duel" (one space) — an invalid path. This copies the exact
  // on-disk path the backend reported, two spaces included.
  async function copyInstallPath() {
    const p = installPath;
    if (!p) return;
    if (await copyText(p)) toast(tNow("toastCopied"), p);
    else toast("", tNow("errCopy"), true);
  }

  // ---- export ----
  async function openExport() {
    try {
      const path = await save({ defaultPath: "steam-mate-accounts.json" });
      if (!path) return;
      await md.exportToFile(path);
      toast("", tNow("toastExp"));
    } catch (e) {
      toastError(e);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      closeDelete();
      closeDeleteBatch();
      closeCreate();
    }
  }

  onMount(() => {
    if (typeof localStorage !== "undefined") {
      const v = localStorage.getItem("sm-view-md");
      if (v === "card" || v === "list") view = v;
    }
    (async () => {
      try {
        // The game can live in any Steam library, so ask the backend for the
        // real install dir rather than assuming the primary Steam root.
        installPath = await md.installPath();
      } catch {
        // pathline just stays empty
      }
    })();
    (async () => {
      try {
        cacheBytes = await md.cacheSize();
      } catch (e) {
        toastError(e);
      }
    })();
    checkCacheExists();
    ensureSteamAccounts();
    checkRunning();
    relist();
    runningTimer = setInterval(checkRunning, 5000);
    window.addEventListener("keydown", onKeydown);
  });
  onDestroy(() => {
    if (runningTimer) clearInterval(runningTimer);
    if (typeof window !== "undefined")
      window.removeEventListener("keydown", onKeydown);
  });
</script>

<section class="page">
  <h2 class="page-title">
    <span>{$t("mdTitle")}</span>
    <span class="count">· {accounts.length}</span>
  </h2>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  <p class="page-sub">{@html $t("mdSub")}</p>

  <div class="pathline">
    <span>{$t("installedAt")}</span>
    <b>{installPath}</b>
    <button class="copy" disabled={!installPath} onclick={copyInstallPath}>
      <svg viewBox="0 0 24 24"
        ><path
          d="M16 1H4a2 2 0 00-2 2v14h2V3h12zm3 4H8a2 2 0 00-2 2v14a2 2 0 002 2h11a2 2 0 002-2V7a2 2 0 00-2-2zm0 16H8V7h11z"
        /></svg
      ><span>{$t("copyBtn")}</span>
    </button>
  </div>

  {#if cacheExists === false}
    <div class="cache">
      <span class="bigicon" style="opacity:.4"
        ><svg viewBox="0 0 24 24"
          ><path d="M4 5h16v4H4zM4 11h16v4H4zM4 17h16v3H4z" /></svg
        ></span
      >
      <div class="ctext">
        <b>{$t("cacheNoneTitle")}</b>
        <div>{$t("cacheNoneDesc")}</div>
      </div>
      <button class="btn accent" disabled={running} onclick={openCreate}
        >{$t("createBtn")}</button
      >
    </div>
  {:else}
    <div class="cache">
      <span class="bigicon"
        ><svg viewBox="0 0 24 24"
          ><path d="M4 5h16v4H4zM4 11h16v4H4zM4 17h16v3H4z" /></svg
        ></span
      >
      <div class="ctext">
        <b>{$t("cacheTitle")}</b>
        <div>{$t("cacheDesc")}</div>
      </div>
      <span class="size">{cacheGb}</span>
      <button class="btn ghost" onclick={revealCache}>{$t("revealBtn")}</button>
    </div>
  {/if}

  {#if running}
    <div class="callout danger">
      <span class="ci"
        ><svg viewBox="0 0 24 24" fill="var(--red)"
          ><path
            d="M12 2L1 21h22zM12 16a1.3 1.3 0 110 2.6A1.3 1.3 0 0112 16zm-1.1-7h2.2l-.3 5.5h-1.6z"
          /></svg
        ></span
      >
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      <div>{@html $t("guardHtml")}</div>
    </div>
  {/if}

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
    <button class="btn" onclick={openExport}>
      <span class="ico"
        ><svg viewBox="0 0 24 24"
          ><path
            d="M5 20h14v-2H5zM12 2L6.5 7.5l1.4 1.4L11 5.8V16h2V5.8l3.1 3.1 1.4-1.4z"
          /></svg
        ></span
      >
      <span>{$t("exportBtn")}</span>
    </button>
    <button class="btn" disabled={running} onclick={linkAll}>
      <span>{$t("linkAll")}</span>
    </button>
    <button class="btn" disabled={running} onclick={unlinkAll}>
      <span>{$t("unlinkAll")}</span>
    </button>
    <select class="type-sel" aria-label="Sort" bind:value={sort}>
      <option value="unlinked">{$t("sortUnlinked")}</option>
      <option value="added">{$t("sortAdded")}</option>
      <option value="alpha">{$t("sortAlpha")}</option>
    </select>
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
    <span class="spacer"></span>
    <span class="page-sub" style="margin:0">{$t("mdHint")}</span>
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
        onclick={openDeleteBatch}>{$t("delBtn")}</button
      >
      <button class="btn" onclick={() => setSelMode(false)}>{$t("cancel")}</button>
    </div>
  {/if}

  {#if view === "card"}
    <div class="grid">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.folderId))}
        {#if "single" in entry}
          {@render mdCard(entry.single, false)}
        {:else}
          {@const open = openFolders.has(entry.folder)}
          <div
            class="card folder-card"
            title={$t("folderTitle")}
            role="button"
            tabindex="0"
            onclick={() => toggleFolder(entry.folder)}
            onkeydown={(e) =>
              (e.key === "Enter" || e.key === " ") &&
              (e.preventDefault(), toggleFolder(entry.folder))}
          >
            <div
              class="cav"
              style="background:linear-gradient(135deg,var(--violet),var(--blue))"
            >
              {initial(entry.folder)}
            </div>
            <div class="cname">{open ? "▾" : "▸"} {entry.folder}</div>
            <div class="csub">
              {fmt($t("folderCount"), { n: entry.items.length })}
            </div>
          </div>
          {#if open}
            {#each entry.items as a (a.folderId)}
              {@render mdCard(a, true)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {:else}
    <div class="list">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.folderId))}
        {#if "single" in entry}
          {@render mdRow(entry.single, false)}
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
            {#each entry.items as a (a.folderId)}
              {@render mdRow(a, true)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {/if}

  <div class="legend">
    <span><i style="background:var(--green)"></i> <span>{$t("legendLinked")}</span></span>
    <span><i style="background:var(--border)"></i> <span>{$t("legendNormal")}</span></span>
    <span><i style="background:var(--violet)"></i> <span>{$t("legendFolder")}</span></span>
  </div>
</section>

{#snippet assignSelect(a: MdAccount)}
  <select
    class="type-sel"
    title={$t("assignTitle")}
    disabled={running || selMode}
    value={a.steamLogin && byLogin.has(a.steamLogin) ? a.steamLogin : ""}
    onchange={(e) =>
      assignSteam(a, (e.currentTarget as HTMLSelectElement).value)}
  >
    <option value="">{$t("assignNone")}</option>
    {#each $steamAccounts as s (s.accountName)}
      <option value={s.accountName}
        >{s.mostRecent && $steamRunning
          ? `● ${s.personaName}（${s.accountName}）· ${$t("signedIn")}`
          : accountLabel($lang, s.personaName, s.accountName)}</option
      >
    {/each}
  </select>
{/snippet}

{#snippet mdRow(a: MdAccount, child: boolean)}
  {@const named = !!(a.accountName && a.accountName.length)}
  {@const steam = assignedSteam(a)}
  {@const uri = steam ? $avatars[steam.steamId64] : null}
  {@const picked = selected.has(a.folderId)}
  {#if selMode}
    <div
      class="row selectable"
      class:is-active={a.isLinked}
      class:child
      class:selected={picked}
      role="button"
      tabindex="0"
      onclick={() => toggleSel(a.folderId)}
      onkeydown={(e) =>
        (e.key === "Enter" || e.key === " ") &&
        (e.preventDefault(), toggleSel(a.folderId))}
    >
      <span class="selbox">✓</span>
      <div class="av" style="background:{hue(a.folderId)}">
        {#if uri}
          <img src={uri} alt="" />
        {/if}
        {named ? initial(a.accountName) : "#"}
      </div>
      <div class="who">
        <div class="acct">
          <span class="md-name" class:unnamed={!named}
            >{named ? a.accountName : $t("setName")}</span
          >
          {#if !steam}
            <span class="pill warn">{$t("unmatched")}</span>
          {/if}
        </div>
        <div class="meta">
          LocalData\{a.folderId}\0000 · {a.isLinked
            ? $t("metaJunction")
            : a.hasFiles
              ? $t("metaOwn")
              : $t("metaEmpty")}
        </div>
      </div>
    </div>
  {:else}
    <div class="row" class:is-active={a.isLinked} class:child>
      <div class="av" style="background:{hue(a.folderId)}">
        {#if uri}
          <img src={uri} alt="" />
        {/if}
        {named ? initial(a.accountName) : "#"}
      </div>
      <div class="who">
        <div class="acct">
          {#if editingId === a.folderId}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="name-input"
              placeholder={$t("setName")}
              bind:value={editValue}
              autofocus
              onblur={() => commitEdit(a)}
              onkeydown={(e) => {
                if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
                if (e.key === "Escape") {
                  editValue = a.accountName;
                  cancelEdit();
                }
              }}
            />
          {:else}
            <span
              class="md-name"
              class:unnamed={!named}
              role="button"
              tabindex="0"
              onclick={() => startEdit(a)}
              onkeydown={(e) =>
                (e.key === "Enter" || e.key === " ") &&
                (e.preventDefault(), startEdit(a))}
              >{named ? a.accountName : $t("setName")}</span
            >
          {/if}
          {#if !steam}
            <span class="pill warn">{$t("unmatched")}</span>
          {/if}
        </div>
        <div class="meta">
          LocalData\{a.folderId}\0000 · {a.isLinked
            ? $t("metaJunction")
            : a.hasFiles
              ? $t("metaOwn")
              : $t("metaEmpty")}
        </div>
      </div>
      <div class="end">
        {@render assignSelect(a)}
        <label
          class="sw"
          class:disabled={running}
          title={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
        >
          <input
            type="checkbox"
            checked={a.isLinked}
            disabled={running}
            onchange={(e) =>
              toggleLink(a, (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="track"></span><span class="knob"></span>
        </label>
        <button class="btn danger-line" disabled={running} onclick={() => openDelete(a)}
          >{$t("delBtn")}</button
        >
      </div>
    </div>
  {/if}
{/snippet}

{#snippet mdCard(a: MdAccount, child: boolean)}
  {@const named = !!(a.accountName && a.accountName.length)}
  {@const steam = assignedSteam(a)}
  {@const uri = steam ? $avatars[steam.steamId64] : null}
  {@const picked = selected.has(a.folderId)}
  {#if selMode}
    <div
      class="card selectable"
      class:is-linked={a.isLinked}
      class:child-card={child}
      class:selected={picked}
      role="button"
      tabindex="0"
      onclick={() => toggleSel(a.folderId)}
      onkeydown={(e) =>
        (e.key === "Enter" || e.key === " ") &&
        (e.preventDefault(), toggleSel(a.folderId))}
    >
      <span class="selbox">✓</span>
      <div class="cav" style="background:{hue(a.folderId)}">
        {#if uri}
          <img src={uri} alt="" />
        {/if}
        {named ? initial(a.accountName) : "#"}
      </div>
      <div class="cname">{named ? a.accountName : $t("setName")}</div>
      <div class="csub">{a.folderId}</div>
      <div class="cfoot">
        {#if !steam}
          <span class="pill warn">{$t("unmatched")}</span>
        {/if}
      </div>
    </div>
  {:else}
    <div class="card" class:is-linked={a.isLinked} class:child-card={child}>
      <button
        class="more"
        title={$t("delBtn")}
        disabled={running}
        onclick={() => openDelete(a)}>⋯</button
      >
      <div class="cav" style="background:{hue(a.folderId)}">
        {#if uri}
          <img src={uri} alt="" />
        {/if}
        {named ? initial(a.accountName) : "#"}
      </div>
      <div class="cname">{named ? a.accountName : $t("setName")}</div>
      <div class="csub">{a.folderId}</div>
      <div class="cfoot">
        {#if !steam}
          <span class="pill warn">{$t("unmatched")}</span>
        {/if}
        <label
          class="sw"
          class:disabled={running}
          title={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
        >
          <input
            type="checkbox"
            checked={a.isLinked}
            disabled={running}
            onchange={(e) =>
              toggleLink(a, (e.currentTarget as HTMLInputElement).checked)}
          />
          <span class="track"></span><span class="knob"></span>
        </label>
      </div>
    </div>
  {/if}
{/snippet}

{#if delAccount}
  {@const a = delAccount}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeDelete()}
  >
    <div class="modal" role="dialog" aria-modal="true">
      <h3>
        {fmt($t("delTitle"), {
          id: a.folderId,
          name: a.accountName
            ? $lang === "en"
              ? ` (${a.accountName})`
              : `（${a.accountName}）`
            : "",
        })}
      </h3>
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      <p>
        {@html fmt($t("delBody"), {
          id: a.folderId,
          linked: a.isLinked ? $t("delBodyLinked") : "",
        })}
      </p>
      {#if delSteam}
        <label class="opt danger">
          <input type="checkbox" bind:checked={delAlsoSteam} />
          <span>
            <div class="ot">
              {fmt($t("delSteamLabel"), {
                s: accountLabel($lang, delSteam.personaName, delSteam.accountName),
              })}
            </div>
            <div class="od">{$t("delSteamDesc")}</div>
          </span>
        </label>
      {/if}
      <div class="actions">
        <button class="btn ghost" onclick={openExport}>{$t("exportFirst")}</button>
        <span class="spacer"></span>
        <button class="btn" onclick={closeDelete}>{$t("cancel")}</button>
        <button class="btn danger" onclick={confirmDelete}>{$t("del")}</button>
      </div>
    </div>
  </div>
{/if}

{#if delBatch}
  {@const n = delBatch.length}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeDeleteBatch()}
  >
    <div class="modal" role="dialog" aria-modal="true">
      <h3>{fmt($t("delTitleN"), { n })}</h3>
      <p>{$t("delBodyN")}</p>
      {#if delBatchLogins.length}
        <label class="opt danger">
          <input type="checkbox" bind:checked={delBatchAlsoSteam} />
          <span>
            <div class="ot">
              {fmt($t("delSteamLabelN"), { k: delBatchLogins.length })}
            </div>
            <div class="od">{$t("delSteamDesc")}</div>
          </span>
        </label>
      {/if}
      <div class="actions">
        <button class="btn ghost" onclick={openExport}>{$t("exportFirst")}</button>
        <span class="spacer"></span>
        <button class="btn" onclick={closeDeleteBatch}>{$t("cancel")}</button>
        <button class="btn danger" onclick={confirmDeleteBatch}>{$t("del")}</button>
      </div>
    </div>
  </div>
{/if}

{#if createOpen}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeCreate()}
  >
    <div class="modal" role="dialog" aria-modal="true">
      <h3>{$t("createTitle")}</h3>
      <p>{$t("createBody")}</p>
      <label class="opt">
        <input
          type="radio"
          name="create-mode"
          value="seed"
          bind:group={createMode}
          disabled={seedCandidates.length === 0}
        />
        <span>
          <div class="ot">{$t("createSeedT")}</div>
          <div class="od">{$t("createSeedD")}</div>
          {#if seedCandidates.length}
            <select
              class="type-sel"
              style="margin-top:6px"
              bind:value={createSeedId}
            >
              {#each seedCandidates as c (c.folderId)}
                <option value={c.folderId}
                  >{c.accountName || c.folderId} · {gb(c.sizeBytes)}</option
                >
              {/each}
            </select>
          {:else}
            <select class="type-sel" style="margin-top:6px" disabled>
              <option>{$t("createNoSeed")}</option>
            </select>
          {/if}
        </span>
      </label>
      <label class="opt">
        <input
          type="radio"
          name="create-mode"
          value="empty"
          bind:group={createMode}
        />
        <span>
          <div class="ot">{$t("createEmptyT")}</div>
          <div class="od">{$t("createEmptyD")}</div>
        </span>
      </label>
      <div class="actions">
        <span class="spacer"></span>
        <button class="btn" onclick={closeCreate}>{$t("cancel")}</button>
        <button class="btn accent" disabled={running} onclick={confirmCreate}
          >{$t("createGo")}</button
        >
      </div>
    </div>
  </div>
{/if}
