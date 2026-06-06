<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { forgetAccount } from "$lib/api/steam";
  import * as md from "$lib/api/games/master-duel";
  import type { MdAccount, SteamAccount } from "$lib/api/types";
  import { asAppError } from "$lib/api/types";
  import { save } from "@tauri-apps/plugin-dialog";
  import { t, fmt, lang, tNow } from "$lib/i18n";
  import { toast } from "$lib/toast";
  import { toastError } from "$lib/errors";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import {
    steamAccounts,
    ensureSteamAccounts,
    refreshSteamAccounts,
    steamByLogin,
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

  let installPath = $state<string>("");
  let accounts = $state<MdAccount[]>([]);
  let order = $state<string[]>([]); // disk order, for "recently added"
  let sort = $state<Sort>("unlinked");
  let cacheBytes = $state<number | null>(null);
  let running = $state(false);
  let openFolders = $state(new Set<string>());

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

  async function loadAccounts() {
    accounts = await md.listAccounts();
    // disk order only grows so "recently added" stays stable across re-lists
    for (const a of accounts)
      if (!order.includes(a.folderId)) order = [...order, a.folderId];
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
    if (running) return;
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

  // ---- delete dialog ----
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
        // refresh the shared Steam store so the persona/avatar map updates
        await refreshSteamAccounts().catch(() => {});
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
    if (e.key === "Escape") closeDelete();
  }

  onMount(() => {
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
  </div>

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
  </div>

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
    <span class="spacer"></span>
    <span class="page-sub" style="margin:0">{$t("mdHint")}</span>
  </div>

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

  <div class="legend">
    <span><i style="background:var(--green)"></i> <span>{$t("legendLinked")}</span></span>
    <span><i style="background:var(--border)"></i> <span>{$t("legendNormal")}</span></span>
    <span><i style="background:var(--violet)"></i> <span>{$t("legendFolder")}</span></span>
  </div>
</section>

{#snippet mdRow(a: MdAccount, child: boolean)}
  {@const named = !!(a.accountName && a.accountName.length)}
  {@const steam = assignedSteam(a)}
  {@const uri = steam ? $avatars[steam.steamId64] : null}
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
          : $t("metaEmpty")}
      </div>
    </div>
    <div class="end">
      <select
        class="type-sel"
        title={$t("assignTitle")}
        disabled={running}
        value={a.steamLogin && byLogin.has(a.steamLogin) ? a.steamLogin : ""}
        onchange={(e) =>
          assignSteam(a, (e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">{$t("assignNone")}</option>
        {#each $steamAccounts as s (s.accountName)}
          <option value={s.accountName}
            >{$lang === "en"
              ? `${s.personaName} (${s.accountName})`
              : `${s.personaName}（${s.accountName}）`}</option
          >
        {/each}
      </select>
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
                s:
                  $lang === "en"
                    ? `${delSteam.personaName} (${delSteam.accountName})`
                    : `${delSteam.personaName}（${delSteam.accountName}）`,
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
