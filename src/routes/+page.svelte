<script lang="ts">
  import { getInstallPath, listAccounts, clearLogin } from "$lib/api/steam";
  import type { SteamAccount, AppError } from "$lib/api/types";

  let steamPath = $state<string | null>(null);
  let accounts = $state<SteamAccount[]>([]);
  let cleared = $state(false);
  let error = $state<AppError | null>(null);
  let loadingPath = $state(false);
  let loadingAccounts = $state(false);
  let loadingClear = $state(false);

  async function fetchSteamPath() {
    loadingPath = true;
    error = null;
    steamPath = null;
    try {
      steamPath = await getInstallPath();
    } catch (e) {
      error = e as AppError;
    } finally {
      loadingPath = false;
    }
  }

  async function fetchAccounts() {
    loadingAccounts = true;
    error = null;
    accounts = [];
    try {
      accounts = await listAccounts();
    } catch (e) {
      error = e as AppError;
    } finally {
      loadingAccounts = false;
    }
  }

  async function clearAutoLogin() {
    loadingClear = true;
    error = null;
    cleared = false;
    try {
      await clearLogin();
      cleared = true;
    } catch (e) {
      error = e as AppError;
    } finally {
      loadingClear = false;
    }
  }
</script>

<main class="container">
  <h1>Welcome to Tauri + Svelte</h1>

  <div class="row">
    <a href="https://vite.dev" target="_blank">
      <img src="/vite.svg" class="logo vite" alt="Vite Logo" />
    </a>
    <a href="https://tauri.app" target="_blank">
      <img src="/tauri.svg" class="logo tauri" alt="Tauri Logo" />
    </a>
    <a href="https://svelte.dev" target="_blank">
      <img src="/svelte.svg" class="logo svelte-kit" alt="SvelteKit Logo" />
    </a>
  </div>
  <p>Click on the Tauri, Vite, and SvelteKit logos to learn more.</p>

  <div class="actions">
    <button onclick={fetchSteamPath} disabled={loadingPath}>
      {loadingPath ? "Looking up..." : "Find Steam install path"}
    </button>
    <button onclick={fetchAccounts} disabled={loadingAccounts}>
      {loadingAccounts ? "Loading..." : "List Steam accounts"}
    </button>
    <button onclick={clearAutoLogin} disabled={loadingClear}>
      {loadingClear ? "Clearing..." : "Clear Steam auto-login"}
    </button>
  </div>

  {#if steamPath}
    <p>Steam is installed at: <code>{steamPath}</code></p>
  {/if}

  {#if cleared}
    <p><em>Steam auto-login cleared. Next Steam launch will show the login screen.</em></p>
  {/if}

  {#if accounts.length > 0}
    <ul class="accounts">
      {#each accounts as a}
        <li>
          <strong>{a.accountName}</strong>
          <span class="persona">({a.personaName})</span>
          {#if a.mostRecent}<span class="badge">most recent</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if error}
    <p>Error: <strong>{error.kind}</strong>{error.msg ? ` — ${error.msg}` : ""}</p>
  {/if}
</main>

<style>
.logo.vite:hover { filter: drop-shadow(0 0 2em #747bff); }
.logo.svelte-kit:hover { filter: drop-shadow(0 0 2em #ff3e00); }

:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;
  color: #0f0f0f;
  background-color: #f6f6f6;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

.logo { height: 6em; padding: 1.5em; will-change: filter; transition: 0.75s; }
.logo.tauri:hover { filter: drop-shadow(0 0 2em #24c8db); }

.row { display: flex; justify-content: center; }
.actions { display: flex; gap: 0.5em; justify-content: center; margin: 1em 0; }

.accounts {
  list-style: none;
  padding: 0;
  margin: 1em auto;
  max-width: 32em;
  text-align: left;
}
.accounts li {
  padding: 0.4em 0.6em;
  border: 1px solid #ddd;
  border-radius: 6px;
  margin-bottom: 0.4em;
  display: flex;
  align-items: center;
  gap: 0.6em;
}
.persona { color: #666; font-size: 0.9em; }
.badge {
  margin-left: auto;
  background: #396cd8;
  color: white;
  padding: 0.1em 0.5em;
  border-radius: 999px;
  font-size: 0.75em;
}

a { font-weight: 500; color: #646cff; text-decoration: inherit; }
a:hover { color: #535bf2; }

h1 { text-align: center; }

button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
  cursor: pointer;
  outline: none;
}
button:hover { border-color: #396cd8; }
button:active { border-color: #396cd8; background-color: #e8e8e8; }

@media (prefers-color-scheme: dark) {
  :root { color: #f6f6f6; background-color: #2f2f2f; }
  a:hover { color: #24c8db; }
  button { color: #ffffff; background-color: #0f0f0f98; }
  button:active { background-color: #0f0f0f69; }
  .accounts li { border-color: #444; }
  .persona { color: #aaa; }
}
</style>
