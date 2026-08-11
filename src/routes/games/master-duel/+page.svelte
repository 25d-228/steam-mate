<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { copyText } from "$lib/clipboard";
  import {
    forgetAccount,
    forgetAccounts,
    listSupportedGames,
  } from "$lib/api/steam";
  import * as md from "$lib/api/games/master-duel";
  import type { MdAccount, SteamAccount, SeedCandidate } from "$lib/api/types";
  import { asAppError } from "$lib/api/types";
  import { save } from "@tauri-apps/plugin-dialog";
  import { t, fmt, lang, tNow, accountLabel } from "$lib/i18n";
  import { hue, initial } from "$lib/avatar";
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
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as Avatar from "$lib/components/ui/avatar/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as NativeSelect from "$lib/components/ui/native-select/index.js";
  import * as RadioGroup from "$lib/components/ui/radio-group/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import * as ToggleGroup from "$lib/components/ui/toggle-group/index.js";
  import { cn } from "$lib/utils.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import DatabaseIcon from "@lucide/svelte/icons/database";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import LayoutGridIcon from "@lucide/svelte/icons/layout-grid";
  import ListIcon from "@lucide/svelte/icons/list";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import UploadIcon from "@lucide/svelte/icons/upload";

  type Sort = "unlinked" | "added" | "alpha";
  type View = "list" | "card";

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
  // How often to re-probe whether Master Duel is running.
  const RUNNING_POLL_MS = 5000;

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

  const cacheSizeLabel = $derived(
    cacheBytes == null ? "—" : formatGb(cacheBytes),
  );

  function orderIndex(a: MdAccount): number {
    const i = order.indexOf(a.folderId);
    return i < 0 ? 0 : i;
  }

  function comparator(a: MdAccount, b: MdAccount): number {
    if (sort === "added") return orderIndex(b) - orderIndex(a);
    if (sort === "alpha")
      return (a.accountName || "").localeCompare(b.accountName || "") || orderIndex(a) - orderIndex(b);
    // unlinked first (default)
    return (
      ((a.isLinked ? 1 : 0) - (b.isLinked ? 1 : 0)) || orderIndex(a) - orderIndex(b)
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
      cacheExists = await md.cacheExists();
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

  async function reloadAccountsQuietly() {
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

  // Re-probe whether the game is running and, if it is, show the "can't while
  // running" toast. Returns true when the caller should bail — every mutating
  // action guards on this so nothing touches the game's folders while it runs.
  async function refuseIfRunning(): Promise<boolean> {
    if (await checkRunning()) {
      toast("", tNow("errRunning"), true);
      return true;
    }
    return false;
  }

  // ---- inline rename ----
  let editingId = $state<string | null>(null);
  let editValue = $state("");

  async function startEdit(a: MdAccount) {
    if (running || selMode) return;
    if (await refuseIfRunning()) return;
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

  // ---- link toggle + forced-link confirmation ----
  let forceLinkOpen = $state(false);
  let forceLinkAccount = $state<MdAccount | null>(null);

  const forceLinkMessage = $derived(
    forceLinkAccount
      ? fmt($t("confirmForce"), { id: forceLinkAccount.folderId })
      : "",
  );
  const forceLinkTitle = $derived(forceLinkMessage.split("\n\n")[0] ?? "");
  const forceLinkDescription = $derived(
    forceLinkMessage.split("\n\n").slice(1).join("\n\n"),
  );

  async function toggleLink(a: MdAccount, wantLinked: boolean) {
    if (await refuseIfRunning()) return;
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
          forceLinkAccount = a;
          forceLinkOpen = true;
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

  function onForceLinkOpenChange(open: boolean) {
    if (open) return;
    forceLinkOpen = false;
    if (!forceLinkAccount) return;
    forceLinkAccount = null;
    loadAccounts().catch(toastError);
  }

  async function confirmForceLink() {
    const a = forceLinkAccount;
    if (!a) return;
    // Clear first so explicitly closing the controlled Alert Dialog is not
    // treated as cancellation. Cancellation alone reloads without retrying.
    forceLinkAccount = null;
    forceLinkOpen = false;
    if (await refuseIfRunning()) return;
    try {
      await md.linkAccount(a.folderId, true);
      await afterLink(a, true);
    } catch (e) {
      toastError(e);
      await loadAccounts();
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
    if (await refuseIfRunning()) return;
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
    if (await refuseIfRunning()) return;
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
  let delBatch = $state<MdAccount[] | null>(null);
  let delBatchLogins = $state<string[]>([]);
  let delBatchAlsoSteam = $state(false);
  let deleteOpen = $state(false);
  let deleteReturnTarget: HTMLElement | null = null;
  let focusToolbarOnDeleteClose = false;
  let toolbarFocusTarget = $state<HTMLButtonElement | null>(null);

  async function openDelete(a: MdAccount, trigger: HTMLElement) {
    if (await refuseIfRunning()) return;
    focusToolbarOnDeleteClose = false;
    deleteReturnTarget = trigger;
    delAccount = a;
    delBatch = null;
    delBatchLogins = [];
    delAlsoSteam = false;
    // Only offer "also forget" when the assignment resolves to a real current
    // Steam account; an unassigned or stale login shows no checkbox.
    delSteam = steamByLogin($steamAccounts, a.steamLogin) ?? null;
    deleteOpen = true;
  }
  function closeDeleteDialog() {
    deleteOpen = false;
    delAccount = null;
    delSteam = null;
    delBatch = null;
    delBatchLogins = [];
  }
  function onDeleteOpenChange(open: boolean) {
    if (!open) closeDeleteDialog();
  }
  function onDeleteCloseAutoFocus(event: Event) {
    event.preventDefault();
    const target = focusToolbarOnDeleteClose
      ? toolbarFocusTarget
      : deleteReturnTarget;
    focusToolbarOnDeleteClose = false;
    deleteReturnTarget = null;
    if (target && document.contains(target)) target.focus();
    else toolbarFocusTarget?.focus();
  }
  async function confirmDelete() {
    const a = delAccount;
    if (!a) return;
    if (await refuseIfRunning()) return;
    const n = a.accountName || a.folderId;
    const both = delAlsoSteam && delSteam;
    const steamName = delSteam?.accountName;
    focusToolbarOnDeleteClose = true;
    closeDeleteDialog();
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
  async function openDeleteBatch(trigger: HTMLElement) {
    if (await refuseIfRunning()) return;
    if (selected.size === 0) return;
    focusToolbarOnDeleteClose = false;
    deleteReturnTarget = trigger;
    delBatch = selectedAccounts;
    delBatchLogins = selectedLogins;
    delBatchAlsoSteam = false;
    delAccount = null;
    delSteam = null;
    deleteOpen = true;
  }
  async function confirmDeleteBatch() {
    const items = delBatch;
    if (!items || !items.length) return;
    if (await refuseIfRunning()) return;
    const logins = delBatchAlsoSteam ? delBatchLogins.slice() : [];
    focusToolbarOnDeleteClose = true;
    closeDeleteDialog();
    try {
      // Steam-forget runs FIRST (one invoke for all distinct logins), then the
      // irreversible per-profile MD delete loop — the same ordering the single
      // delete uses, so a forget failure never wipes a save folder.
      if (logins.length) {
        await forgetAccounts(logins);
        // The batch forget killed Steam without relaunching it — refresh both
        // the account map and the running probe.
        await refreshSteamAccounts().catch(() => {});
        refreshSteamRunning();
      }
      // One backend call for the whole batch: one running check, one install
      // resolve, and a profile that fails is skipped and reported instead of
      // stranding the rest half-deleted.
      const res = await md.deleteAccounts(items.map((a) => a.folderId));
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
  let createReturnTarget: HTMLElement | null = null;
  let focusToolbarOnCreateClose = false;

  function formatGb(bytes: number): string {
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  }

  async function openCreate(trigger: HTMLElement) {
    if (await refuseIfRunning()) return;
    focusToolbarOnCreateClose = false;
    createReturnTarget = trigger;
    try {
      const candidates = await md.seedCandidates();
      // largest-first; defensive even if the backend already sorts
      seedCandidates = candidates
        .slice()
        .sort((a, b) => b.sizeBytes - a.sizeBytes);
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
  function onCreateOpenChange(open: boolean) {
    if (!open) closeCreate();
  }
  function onCreateCloseAutoFocus(event: Event) {
    event.preventDefault();
    const target = focusToolbarOnCreateClose
      ? toolbarFocusTarget
      : createReturnTarget;
    focusToolbarOnCreateClose = false;
    createReturnTarget = null;
    if (target && document.contains(target)) target.focus();
    else toolbarFocusTarget?.focus();
  }
  async function confirmCreate() {
    if (await refuseIfRunning()) return;
    const seed = createMode === "seed" ? createSeedId || null : null;
    const seedName =
      seed != null
        ? (() => {
            const c = seedCandidates.find((x) => x.folderId === seed);
            return c ? c.accountName || c.folderId : seed;
          })()
        : "";
    focusToolbarOnCreateClose = true;
    closeCreate();
    try {
      await md.createCache(seed);
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
      await md.revealCache();
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

  onMount(() => {
    let destroyed = false;

    (async () => {
      // Direct navigation can reach this route even when navigation omits it.
      // Check the backend's compile-time support list before invoking any
      // Windows-only Master Duel command.
      try {
        const supported = await listSupportedGames();
        if (destroyed) return;
        if (!supported.some((game) => game.id === "master_duel")) {
          await goto("/steam", { replaceState: true });
          return;
        }
      } catch {
        if (destroyed) return;
        await goto("/steam", { replaceState: true });
        return;
      }

      if (typeof localStorage !== "undefined") {
        const v = localStorage.getItem("sm-view-md");
        if (v === "card" || v === "list") view = v;
      }

      try {
        // The game can live in any Steam library, so ask the backend for the
        // real install dir rather than assuming the primary Steam root.
        const path = await md.installPath();
        if (destroyed) return;
        installPath = path;
      } catch {
        if (destroyed) return;
        // pathline just stays empty
      }
      try {
        const bytes = await md.cacheSize();
        if (destroyed) return;
        cacheBytes = bytes;
      } catch (e) {
        if (destroyed) return;
        toastError(e);
      }
      if (destroyed) return;
      checkCacheExists();
      ensureSteamAccounts();
      checkRunning();
      reloadAccountsQuietly();
      runningTimer = setInterval(checkRunning, RUNNING_POLL_MS);
    })();

    return () => {
      destroyed = true;
      if (runningTimer) clearInterval(runningTimer);
    };
  });
</script>

<section class="page">
  <h2 class="page-title">
    <span>{$t("mdTitle")}</span>
    <span class="count">· {accounts.length}</span>
  </h2>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  <p class="page-sub">{@html $t("mdSub")}</p>

  <Card.Root
    size="sm"
    class="mb-4 inline-flex max-w-full flex-row items-center gap-2 rounded-lg border border-border bg-muted px-2.5 py-1.5 font-mono text-[11.5px] text-muted-foreground shadow-inner"
  >
    <span class="shrink-0">{$t("installedAt")}</span>
    <b class="truncate font-semibold text-foreground" title={installPath}>{installPath}</b>
    <Button
      variant="outline"
      size="xs"
      class="ml-1 shrink-0 font-mono"
      disabled={!installPath}
      onclick={copyInstallPath}
    >
      <CopyIcon data-icon="inline-start" aria-hidden="true" />
      <span>{$t("copyBtn")}</span>
    </Button>
  </Card.Root>

  {#if cacheExists === false}
    <Card.Root
      size="sm"
      class="mb-4 flex-row items-center gap-3 overflow-visible border border-dashed border-primary/45 bg-muted px-3.5 py-3"
    >
      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary opacity-60">
        <DatabaseIcon class="size-5" aria-hidden="true" />
      </span>
      <div class="min-w-0 flex-1">
        <b class="text-[13px] text-foreground">{$t("cacheNoneTitle")}</b>
        <div class="mt-0.5 text-[11.5px] text-muted-foreground">
          {$t("cacheNoneDesc")}
        </div>
      </div>
      <Button
        disabled={running}
        onclick={(event) => openCreate(event.currentTarget)}
      >
        {$t("createBtn")}
      </Button>
    </Card.Root>
  {:else}
    <Card.Root
      size="sm"
      class="mb-4 flex-row items-center gap-3 overflow-visible border border-dashed border-[rgba(42,161,152,0.45)] bg-[rgba(42,161,152,0.06)] px-3.5 py-3"
    >
      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-[rgba(42,161,152,0.16)] text-[var(--cyan)] ring-1 ring-[rgba(42,161,152,0.25)]">
        <DatabaseIcon class="size-5" aria-hidden="true" />
      </span>
      <div class="min-w-0 flex-1">
        <b class="text-[13px] text-foreground">{$t("cacheTitle")}</b>
        <div class="mt-0.5 text-[11.5px] text-muted-foreground">
          {$t("cacheDesc")}
        </div>
      </div>
      <span class="font-mono text-sm font-bold text-[var(--cyan)]">{cacheSizeLabel}</span>
      <Button variant="ghost" onclick={revealCache}>
        <FolderOpenIcon data-icon="inline-start" aria-hidden="true" />
        {$t("revealBtn")}
      </Button>
    </Card.Root>
  {/if}

  {#if running}
    <Card.Root
      size="sm"
      class="mb-4 flex-row items-start gap-3 overflow-visible border border-destructive/40 bg-destructive/10 px-3.5 py-3 text-[12.5px] leading-relaxed"
    >
      <TriangleAlertIcon class="mt-0.5 size-[18px] shrink-0 text-destructive" aria-hidden="true" />
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      <div>{@html $t("guardHtml")}</div>
    </Card.Root>
  {/if}

  <div class="mb-4 flex flex-wrap items-center gap-2">
    <Button bind:ref={toolbarFocusTarget} variant="outline" onclick={refresh}>
      <RefreshCwIcon data-icon="inline-start" aria-hidden="true" />
      <span>{$t("refreshBtn")}</span>
    </Button>
    <Button variant="outline" onclick={openExport}>
      <UploadIcon data-icon="inline-start" aria-hidden="true" />
      <span>{$t("exportBtn")}</span>
    </Button>
    <Button variant="outline" disabled={running} onclick={linkAll}>
      <span>{$t("linkAll")}</span>
    </Button>
    <Button variant="outline" disabled={running} onclick={unlinkAll}>
      <span>{$t("unlinkAll")}</span>
    </Button>
    <NativeSelect.Root aria-label="Sort" bind:value={sort}>
      <NativeSelect.Option value="unlinked">{$t("sortUnlinked")}</NativeSelect.Option>
      <NativeSelect.Option value="added">{$t("sortAdded")}</NativeSelect.Option>
      <NativeSelect.Option value="alpha">{$t("sortAlpha")}</NativeSelect.Option>
    </NativeSelect.Root>
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
    >
      {$t("select")}
    </Button>
    <span class="flex-1"></span>
    <span class="text-[12.5px] text-muted-foreground">{$t("mdHint")}</span>
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
      <Button
        variant="destructive"
        disabled={selected.size === 0}
        onclick={(event) => openDeleteBatch(event.currentTarget)}
      >
        <Trash2Icon data-icon="inline-start" aria-hidden="true" />
        {$t("delBtn")}
      </Button>
      <Button variant="outline" onclick={() => setSelMode(false)}>{$t("cancel")}</Button>
    </Card.Root>
  {/if}

  {#if view === "card"}
    <div class="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-2.5">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.folderId))}
        {#if "single" in entry}
          {@render mdCard(entry.single, false)}
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
            <div class="font-mono text-[10.5px] text-muted-foreground">
              {fmt($t("folderCount"), { n: entry.items.length })}
            </div>
          </Card.Root>
          {#if open}
            {#each entry.items as a (a.folderId)}
              {@render mdCard(a, true, col)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {:else}
    <div class="flex flex-col gap-2">
      {#each entries as entry (("folder" in entry ? "f:" + entry.folder : "s:" + entry.single.folderId))}
        {#if "single" in entry}
          {@render mdRow(entry.single, false)}
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
            {#each entry.items as a (a.folderId)}
              {@render mdRow(a, true)}
            {/each}
          {/if}
        {/if}
      {/each}
    </div>
  {/if}

  <div class="mt-4 flex flex-wrap gap-4 text-[11.5px] text-muted-foreground">
    <span class="inline-flex items-center gap-1.5">
      <i class="block size-2.5 rounded-[3px] bg-[var(--green)]"></i>
      <span>{$t("legendLinked")}</span>
    </span>
    <span class="inline-flex items-center gap-1.5">
      <i class="block size-2.5 rounded-[3px] bg-border"></i>
      <span>{$t("legendNormal")}</span>
    </span>
    <span class="inline-flex items-center gap-1.5">
      <i class="block size-2.5 rounded-[3px] bg-[var(--violet)]"></i>
      <span>{$t("legendFolder")}</span>
    </span>
  </div>
</section>

{#snippet assignSelect(a: MdAccount)}
  <NativeSelect.Root
    size="sm"
    class="max-w-[230px]"
    title={$t("assignTitle")}
    aria-label={$t("assignTitle")}
    disabled={running || selMode}
    value={a.steamLogin && byLogin.has(a.steamLogin) ? a.steamLogin : ""}
    onchange={(e) =>
      assignSteam(a, (e.currentTarget as HTMLSelectElement).value)}
  >
    <NativeSelect.Option value="">{$t("assignNone")}</NativeSelect.Option>
    {#each $steamAccounts as s (s.accountName)}
      <NativeSelect.Option value={s.accountName}
        >{s.mostRecent && $steamRunning
          ? `● ${s.personaName}（${s.accountName}）· ${$t("signedIn")}`
          : accountLabel($lang, s.personaName, s.accountName)}</NativeSelect.Option
      >
    {/each}
  </NativeSelect.Root>
{/snippet}

{#snippet mdRow(a: MdAccount, child: boolean)}
  {@const named = !!(a.accountName && a.accountName.length)}
  {@const steam = assignedSteam(a)}
  {@const uri = steam ? $avatars[steam.steamId64] : null}
  {@const picked = selected.has(a.folderId)}
  <Card.Root
    class={cn(
      "relative flex-row items-center gap-3 overflow-visible border border-border px-3.5 py-3 shadow-sm transition hover:-translate-y-px hover:border-primary/45 hover:shadow-md",
      a.isLinked &&
        "border-[rgba(133,153,0,0.5)] bg-[linear-gradient(90deg,rgba(133,153,0,0.1),rgba(133,153,0,0.03)_40%),var(--win)] before:absolute before:inset-y-2 before:left-0 before:w-[3px] before:rounded-r-sm before:bg-[var(--green)]",
      child &&
        "ml-[34px] after:absolute after:-left-[19px] after:top-1/2 after:h-px after:w-[13px] after:bg-border",
      selMode && picked && "border-primary ring-2 ring-primary/25",
    )}
  >
    {#if selMode}
      <Checkbox
        class="cursor-pointer"
        checked={picked}
        aria-label={a.accountName || a.folderId}
        onCheckedChange={(checked) => checked !== picked && toggleSel(a.folderId)}
      />
    {/if}
    <Avatar.Root class="size-10 rounded-xl shadow-md">
      {#if uri}
        <Avatar.Image class="rounded-xl" src={uri} alt="" />
      {/if}
      <Avatar.Fallback
        class="rounded-xl font-bold text-white"
        style={`background: ${hue(a.folderId)}`}
      >{named ? initial(a.accountName) : "#"}</Avatar.Fallback>
    </Avatar.Root>
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        {#if !selMode && editingId === a.folderId}
          <Input
            class="h-7 w-[170px] font-semibold"
            placeholder={$t("setName")}
            bind:value={editValue}
            autofocus
            onblur={() => commitEdit(a)}
            onkeydown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") {
                editValue = a.accountName;
                cancelEdit();
              }
            }}
          />
        {:else if selMode}
          <span
            class={cn(
              "truncate text-sm font-bold text-foreground",
              !named && "font-medium text-muted-foreground italic",
            )}
          >{named ? a.accountName : $t("setName")}</span>
        {:else}
          <Button
            variant="link"
            size="sm"
            class={cn(
              "h-auto max-w-full justify-start truncate p-0 text-sm font-bold",
              !named && "font-medium text-muted-foreground italic",
            )}
            disabled={running}
            onclick={() => startEdit(a)}
          >{named ? a.accountName : $t("setName")}</Button>
        {/if}
        {#if !steam}
          <Badge variant="destructive">{$t("unmatched")}</Badge>
        {/if}
      </div>
      <div class="mt-1 truncate font-mono text-[11px] text-muted-foreground">
        LocalData\{a.folderId}\0000 · {a.isLinked
          ? $t("metaJunction")
          : a.hasFiles
            ? $t("metaOwn")
            : $t("metaEmpty")}
      </div>
    </div>
    {#if !selMode}
      <div class="ml-auto flex items-center gap-2.5">
        {@render assignSelect(a)}
        <Switch
          checked={a.isLinked}
          disabled={running}
          title={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
          aria-label={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
          onCheckedChange={(checked) => toggleLink(a, checked)}
        />
        <Button
          variant="destructive"
          disabled={running}
          onclick={(event) => openDelete(a, event.currentTarget)}
        >{$t("delBtn")}</Button>
      </div>
    {/if}
  </Card.Root>
{/snippet}

{#snippet mdCard(a: MdAccount, child: boolean, fc?: string)}
  {@const named = !!(a.accountName && a.accountName.length)}
  {@const steam = assignedSteam(a)}
  {@const uri = steam ? $avatars[steam.steamId64] : null}
  {@const picked = selected.has(a.folderId)}
  <Card.Root
    class={cn(
      "relative items-center gap-1.5 overflow-visible border border-border px-2.5 py-4 text-center shadow-sm transition hover:-translate-y-px hover:border-primary/45 hover:shadow-md",
      a.isLinked && "border-t-[3px] border-t-[var(--green)]",
      child &&
        "border-[color:color-mix(in_srgb,var(--folder-color)_30%,transparent)] bg-[color:color-mix(in_srgb,var(--folder-color)_10%,var(--win))]",
      selMode && picked && "border-primary ring-2 ring-primary/25",
    )}
    style={fc ? `--folder-color: ${fc}` : undefined}
  >
    {#if selMode}
      <Checkbox
        class="absolute top-2 left-2 cursor-pointer"
        checked={picked}
        aria-label={a.accountName || a.folderId}
        onCheckedChange={(checked) => checked !== picked && toggleSel(a.folderId)}
      />
    {:else}
      <Button
        variant="ghost"
        size="icon-xs"
        class="absolute top-1.5 right-1.5 text-muted-foreground"
        title={$t("delBtn")}
        aria-label={$t("delBtn")}
        disabled={running}
        onclick={(event) => openDelete(a, event.currentTarget)}
      >
        <EllipsisIcon aria-hidden="true" />
      </Button>
    {/if}
    <Avatar.Root class="size-14 rounded-xl shadow-md">
      {#if uri}
        <Avatar.Image class="rounded-xl" src={uri} alt="" />
      {/if}
      <Avatar.Fallback
        class="rounded-xl text-xl font-bold text-white"
        style={`background: ${hue(a.folderId)}`}
      >{named ? initial(a.accountName) : "#"}</Avatar.Fallback>
    </Avatar.Root>
    <div
      class={cn(
        "break-all text-[13px] leading-tight font-bold text-foreground",
        !named && "font-medium text-muted-foreground italic",
      )}
    >{named ? a.accountName : $t("setName")}</div>
    <div class="break-all font-mono text-[10.5px] text-muted-foreground">
      {a.folderId}
    </div>
    <div class="mt-0.5 flex min-h-5 items-center gap-2">
      {#if !steam}
        <Badge variant="destructive">{$t("unmatched")}</Badge>
      {/if}
      {#if !selMode}
        <Switch
          size="sm"
          checked={a.isLinked}
          disabled={running}
          title={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
          aria-label={a.isLinked ? $t("linkedTitle") : $t("linkTitle")}
          onCheckedChange={(checked) => toggleLink(a, checked)}
        />
      {/if}
    </div>
  </Card.Root>
{/snippet}

<Dialog.Root bind:open={deleteOpen} onOpenChange={onDeleteOpenChange}>
  {#if delAccount || delBatch}
    <Dialog.Content
      class="max-w-[min(500px,calc(100%-2rem))] gap-3 p-5"
      onCloseAutoFocus={onDeleteCloseAutoFocus}
    >
      <Dialog.Header>
        <Dialog.Title>
          {#if delBatch}
            {fmt($t("delTitleN"), { n: delBatch.length })}
          {:else if delAccount}
            {fmt($t("delTitle"), {
              id: delAccount.folderId,
              name: delAccount.accountName
                ? $lang === "en"
                  ? ` (${delAccount.accountName})`
                  : `（${delAccount.accountName}）`
                : "",
            })}
          {/if}
        </Dialog.Title>
        <Dialog.Description class="text-left text-[12.5px] leading-relaxed">
          {#if delBatch}
            {$t("delBodyN")}
          {:else if delAccount}
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html fmt($t("delBody"), {
              id: delAccount.folderId,
              linked: delAccount.isLinked ? $t("delBodyLinked") : "",
            })}
          {/if}
        </Dialog.Description>
      </Dialog.Header>

      {#if delBatch && delBatchLogins.length}
        <Label
          for="md-delete-steam-batch"
          class="items-start rounded-lg border border-border p-3 transition-colors hover:border-destructive/45 hover:bg-destructive/5"
        >
          <Checkbox
            id="md-delete-steam-batch"
            class="mt-0.5"
            bind:checked={delBatchAlsoSteam}
          />
          <span>
            <span class="block text-[12.5px] font-semibold text-foreground">
              {fmt($t("delSteamLabelN"), { k: delBatchLogins.length })}
            </span>
            <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
              {$t("delSteamDesc")}
            </span>
          </span>
        </Label>
      {:else if delAccount && delSteam}
        <Label
          for="md-delete-steam-single"
          class="items-start rounded-lg border border-border p-3 transition-colors hover:border-destructive/45 hover:bg-destructive/5"
        >
          <Checkbox
            id="md-delete-steam-single"
            class="mt-0.5"
            bind:checked={delAlsoSteam}
          />
          <span>
            <span class="block text-[12.5px] font-semibold text-foreground">
              {fmt($t("delSteamLabel"), {
                s: accountLabel($lang, delSteam.personaName, delSteam.accountName),
              })}
            </span>
            <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
              {$t("delSteamDesc")}
            </span>
          </span>
        </Label>
      {/if}

      <Dialog.Footer class="-mx-5 -mb-5 mt-1 p-4">
        <Button variant="ghost" class="mr-auto" onclick={openExport}>
          <UploadIcon data-icon="inline-start" aria-hidden="true" />
          {$t("exportFirst")}
        </Button>
        <Dialog.Close>
          {#snippet child({ props })}
            <Button {...props} variant="outline">{$t("cancel")}</Button>
          {/snippet}
        </Dialog.Close>
        <Button
          variant="destructive"
          onclick={delBatch ? confirmDeleteBatch : confirmDelete}
        >
          <Trash2Icon data-icon="inline-start" aria-hidden="true" />
          {$t("del")}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  {/if}
</Dialog.Root>

<Dialog.Root bind:open={createOpen} onOpenChange={onCreateOpenChange}>
  <Dialog.Content
    class="max-w-[min(500px,calc(100%-2rem))] gap-3 p-5"
    onCloseAutoFocus={onCreateCloseAutoFocus}
  >
    <Dialog.Header>
      <Dialog.Title>{$t("createTitle")}</Dialog.Title>
      <Dialog.Description class="text-left text-[12.5px] leading-relaxed">
        {$t("createBody")}
      </Dialog.Description>
    </Dialog.Header>

    <RadioGroup.Root
      name="create-mode"
      value={createMode}
      onValueChange={(value) => {
        if (value === "seed" || value === "empty") createMode = value;
      }}
    >
      <div class="flex items-start gap-3 rounded-lg border border-border p-3 transition-colors hover:border-primary/50 hover:bg-accent">
        <RadioGroup.Item
          id="md-create-seed"
          value="seed"
          class="mt-0.5"
          disabled={seedCandidates.length === 0}
        />
        <div class="min-w-0 flex-1">
          <Label for="md-create-seed" class="block cursor-pointer">
            <span class="block text-[12.5px] font-semibold text-foreground">
              {$t("createSeedT")}
            </span>
            <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
              {$t("createSeedD")}
            </span>
          </Label>
          {#if seedCandidates.length}
            <NativeSelect.Root class="mt-2 w-full" bind:value={createSeedId}>
              {#each seedCandidates as candidate (candidate.folderId)}
                <NativeSelect.Option value={candidate.folderId}>
                  {candidate.accountName || candidate.folderId} · {formatGb(candidate.sizeBytes)}
                </NativeSelect.Option>
              {/each}
            </NativeSelect.Root>
          {:else}
            <NativeSelect.Root class="mt-2 w-full" disabled>
              <NativeSelect.Option>{$t("createNoSeed")}</NativeSelect.Option>
            </NativeSelect.Root>
          {/if}
        </div>
      </div>
      <div class="flex items-start gap-3 rounded-lg border border-border p-3 transition-colors hover:border-primary/50 hover:bg-accent">
        <RadioGroup.Item id="md-create-empty" value="empty" class="mt-0.5" />
        <Label for="md-create-empty" class="block cursor-pointer">
          <span class="block text-[12.5px] font-semibold text-foreground">
            {$t("createEmptyT")}
          </span>
          <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
            {$t("createEmptyD")}
          </span>
        </Label>
      </div>
    </RadioGroup.Root>

    <Dialog.Footer class="-mx-5 -mb-5 mt-1 p-4">
      <Dialog.Close>
        {#snippet child({ props })}
          <Button {...props} variant="outline">{$t("cancel")}</Button>
        {/snippet}
      </Dialog.Close>
      <Button disabled={running} onclick={confirmCreate}>{$t("createGo")}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<AlertDialog.Root bind:open={forceLinkOpen} onOpenChange={onForceLinkOpenChange}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Media>
        <TriangleAlertIcon class="text-destructive" aria-hidden="true" />
      </AlertDialog.Media>
      <AlertDialog.Title>{forceLinkTitle}</AlertDialog.Title>
      <AlertDialog.Description class="whitespace-pre-line">
        {forceLinkDescription}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{$t("cancel")}</AlertDialog.Cancel>
      <AlertDialog.Action variant="destructive" onclick={confirmForceLink}>
        {$t("linkTitle")}
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
