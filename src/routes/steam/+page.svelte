<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    getInstallPath,
    listAccounts,
    clearLogin,
    switchAccount,
    forgetAccount,
  } from "$lib/api/steam";
  import type { SteamAccount } from "$lib/api/types";
  import { t, fmt, lang, tNow } from "$lib/i18n";
  import { toast, toastLoading } from "$lib/toast";
  import { toastError } from "$lib/errors";
  import { avatars, fetchAvatar } from "$lib/stores/avatars";
  import { steamAccounts } from "$lib/stores/steam";

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

  let installPath = $state<string>("");
  let accounts = $state<SteamAccount[]>([]);
  let offline = $state(false);
  let switching = $state(false);

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
    if (a.mostRecent || switching) return;
    switching = true;
    const off = offline;
    const disp =
      $lang === "en"
        ? `${a.personaName} (${a.accountName})`
        : `${a.personaName}（${a.accountName}）`;
    // Spinner stays up for the whole (multi-second) kill/rewrite/relaunch; the
    // success message is shown only once switchAccount actually resolves, so a
    // failure never flashes a false "Signed in" toast.
    toastLoading(fmt(tNow("toastSwitch1"), { p: a.personaName }));
    try {
      await switchAccount(a.accountName, off);
      await loadAccounts();
      toast(
        "",
        fmt(tNow("toastSwitch2"), {
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

  // ---- Steam delete (hide / forget) dialog ----
  let sdelAccount = $state<SteamAccount | null>(null);
  let sdelMode = $state<"hide" | "forget">("hide");

  function openSDelete(a: SteamAccount) {
    sdelAccount = a;
    sdelMode = "hide";
  }
  function closeSDelete() {
    sdelAccount = null;
  }
  async function confirmSDelete() {
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
        toast("", fmt(tNow("toastForget"), { a: name }));
      } catch (e) {
        toastError(e);
      }
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") closeSDelete();
  }

  function onFocus() {
    relist();
  }

  onMount(() => {
    loadHidden();
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
  });
  onDestroy(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("keydown", onKeydown);
    }
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

  <div class="legend">
    <span><i style="background:var(--green)"></i> <span>{$t("legendActive")}</span></span>
    <span><i style="background:var(--violet)"></i> <span>{$t("legendFolder")}</span></span>
  </div>
</section>

{#snippet steamRow(a: SteamAccount, child: boolean)}
  {@const uri = $avatars[a.steamId64]}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="row"
    class:is-active={a.mostRecent}
    class:child
    class:can-switch={!a.mostRecent}
    title={a.mostRecent ? undefined : $t("rowTitle")}
    role={a.mostRecent ? undefined : "button"}
    tabindex={a.mostRecent ? undefined : 0}
    ondblclick={() => switchTo(a)}
  >
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
      {#if a.mostRecent}
        <span class="pill active">{$t("activePill")}</span>
      {:else}
        <span class="pill muted">{$t("dblPill")}</span>
      {/if}
      <button
        class="btn danger-line"
        onclick={(e) => {
          e.stopPropagation();
          openSDelete(a);
        }}>{$t("delBtn")}</button
      >
    </div>
  </div>
{/snippet}

{#if sdelAccount}
  {@const a = sdelAccount}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeSDelete()}
  >
    <div class="modal" role="dialog" aria-modal="true">
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
      <div class="actions">
        <span class="spacer"></span>
        <button class="btn" onclick={closeSDelete}>{$t("cancel")}</button>
        <button class="btn danger" onclick={confirmSDelete}>{$t("removeBtn")}</button>
      </div>
    </div>
  </div>
{/if}
