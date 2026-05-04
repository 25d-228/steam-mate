# steam-mate dev reference

> One-stop reference for v0.1 implementation.

## What it is

Tiny Tauri 2 + Svelte desktop app. Two features:

1. **Steam account switching** — Watt Toolkit-style: kill Steam → patch `loginusers.vdf` + Steam-internal "registry" → relaunch. Cross-platform design; v0.1 implements Windows.
2. **Master Duel asset linking (Windows-only)** — replace per-account `LocalData\<id>\0000\` with NTFS junctions to a shared `LocalData\DATA\0000\` (~13 GB cache shared across Konami accounts).

Target binary: < 6 MB stripped on Windows.

---

## v0.1 acceptance — must all be checked before shipping

### Steam (Windows)
- [ ] `steam::paths::find_install()` reads `HKCU\Software\Valve\Steam\SteamPath`
- [ ] `steam::vdf::{read,write}_login_users()` round-trip losslessly
- [ ] `steam::registry::set_auto_login_user()` writes 3 keys
- [ ] `steam::process::{kill,start}_steam()`
- [ ] `steam::switch::switch_account()` orchestrator
- [ ] Commands: `list_steam_accounts`, `switch_steam_account`, `kill_steam`, `start_steam`, `clear_steam_login`
- [ ] `/steam` route: account list, click-to-switch, "currently active" indicator

### Master Duel (Windows)
- [ ] `master_duel::paths::find_install()` via `libraryfolders.vdf`
- [ ] `master_duel::csv::{read,write,upsert}_accounts()`
- [ ] `master_duel::link::{link,unlink,is_linked}_account()`
- [ ] `master_duel::process::is_running()`
- [ ] Commands: `md_list_accounts`, `md_link_account`, `md_unlink_account`, `md_is_linked`, `md_save_metadata`, `md_is_running`
- [ ] `/games/master-duel` route: account list, link toggle per row, inline name/type editing
- [ ] Refuses link/unlink with `GameRunning` if `masterduel.exe` is up

### Quality bar
- [ ] Each module has unit tests with fixture VDF/CSV
- [ ] `tests/integration_steam_switch.rs` and `tests/integration_md_link.rs` against `tempdir()`
- [ ] Stripped release binary < 6 MB
- [ ] Manual E2E checklist (end of doc) passes on real Windows 11 install

---

## Module layout

```
src-tauri/src/
├── main.rs                 # Tauri builder + invoke_handler
├── lib.rs                  # mod declarations
├── error.rs                # AppError, AppResult
├── steam/                  # Cross-platform
│   ├── mod.rs
│   ├── commands.rs         # #[tauri::command]s
│   ├── account.rs          # SteamAccount struct
│   ├── paths.rs
│   ├── vdf.rs              # loginusers.vdf parse/write
│   ├── registry.rs         # Win HKCU OR registry.vdf (Linux/Mac)
│   ├── process.rs          # kill/start
│   └── switch.rs           # orchestrator
└── games/
    ├── mod.rs              # SUPPORTED_GAMES registry
    └── master_duel/        # #[cfg(windows)]
        ├── mod.rs
        ├── commands.rs
        ├── paths.rs
        ├── csv.rs
        ├── link.rs
        └── account.rs

src/
├── lib/
│   ├── api/
│   │   ├── steam.ts        # invoke wrappers
│   │   ├── types.ts        # SteamAccount, MdAccount, GameInfo, AppError
│   │   └── games/master-duel.ts
│   └── components/
├── routes/
│   ├── +layout.svelte      # nav: Steam | Games > [discovered]
│   ├── steam/+page.svelte
│   └── games/master-duel/+page.svelte
└── app.html
```

**Settled design decisions** — don't re-litigate:

- **Steam is top-level, not a "game".** Steam is the platform; account switching is a platform feature.
- **No shared trait across games.** Different games will need different surfaces. Promote a helper to a shared module only when two consumers actually need it.
- **Each module owns its `commands.rs`.** Tauri requires static command registration in `main.rs` — per-module files keep it mechanical.
- **No `core/` until needed.** Premature shared modules end up coupled.

**Cross-platform `cfg`:**
- Module-level: `#[cfg(windows)] mod master_duel;` in `games/mod.rs`. Non-Windows builds drop the entire module.
- Function-level: for cross-platform modules with per-OS branches, use sub-files (`steam/registry/{windows,linux,macos}.rs`) re-exported from `mod.rs`.

---

## Bootstrap

```powershell
# Prereqs: Rust 1.80+, Node 20 LTS, pnpm
cargo install create-tauri-app --locked
cargo install tauri-cli --version "^2.0" --locked

# Scaffold
cargo create-tauri-app
#   name:     steam-mate
#   id:       com.<your-handle>.steammate
#   frontend: TypeScript / Svelte
#   pkg mgr:  pnpm
cd steam-mate
pnpm install

# Run
pnpm tauri dev      # HMR + Rust rebuild on save
pnpm tauri build    # release binary + .msi
```

**`Cargo.lock` IS committed** — steam-mate is a binary, not a library.

### `src-tauri/Cargo.toml`

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
keyvalues-parser = "0.2"
keyvalues-serde = "0.2"
sysinfo = "0.32"
dirs = "5"
csv = "1.3"
walkdir = "2"

[target.'cfg(windows)'.dependencies]
winreg = "0.52"
junction = "1.2"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

### Why these crates

| Crate | Rationale |
|---|---|
| `keyvalues-parser` 0.2 | VDF parser; round-trip preserves source |
| `keyvalues-serde` 0.2 | typed deser on top |
| `winreg` 0.52 | HKCU access; pure Rust |
| **`junction` 1.2** | **`std::os::windows::fs::symlink_dir` requires admin / Dev Mode. Junctions don't. Decisive.** |
| `sysinfo` 0.32 | cross-OS process listing |
| `csv` 1.3 | CSV with CJK round-trip |
| `dirs` 5 | per-OS user dirs |

---

## Error model

Single crate-wide enum. All commands return `AppResult<T>`. Tagged for frontend pattern matching.

```rust
#[derive(thiserror::Error, Debug, serde::Serialize)]
#[serde(tag = "kind", content = "msg")]
pub enum AppError {
    #[error("steam not installed")]              SteamNotInstalled,
    #[error("vdf parse failed: {0}")]            VdfParse(String),
    #[error("registry write failed: {0}")]       RegistryWrite(String),
    #[error("io error: {0}")]                    Io(String),
    #[error("game not installed: {0}")]          GameNotInstalled(&'static str),
    #[error("game running: {0}")]                GameRunning(&'static str),
    #[error("junction failed: {0}")]             JunctionFailed(String),
    #[error("account not found: {0}")]           AccountNotFound(String),
    #[error("process kill failed: {0}")]         ProcessKillFailed(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e.to_string()) }
}

pub type AppResult<T> = Result<T, AppError>;
```

Add new variants per-feature; **do not** create per-module error enums.

---

## Tauri command surface

All commands `async fn`, return `AppResult<T>`.

### Steam (cross-platform; v0.1 = Windows)

| Command | Args | Returns | Behavior |
|---|---|---|---|
| `list_steam_accounts` | – | `SteamAccount[]` | parse `loginusers.vdf` |
| `switch_steam_account` | `{ accountName, offlineMode?, personaState? }` | `()` | full switch sequence; `personaState` ignored in v0.1 |
| `clear_steam_login` | – | `()` | `AutoLoginUser = ""` |
| `kill_steam` | – | `()` | SIGTERM, poll 3s, force-kill if needed; idempotent |
| `start_steam` | – | `()` | spawn Steam exe; doesn't wait |

### Master Duel (`#[cfg(windows)]` — absent on other OSes)

| Command | Args | Returns | Behavior |
|---|---|---|---|
| `md_list_accounts` | – | `MdAccount[]` | scan `LocalData\` × `accounts.csv` |
| `md_link_account` | `{ folderId, force? }` | `()` | refuses non-empty `0000\` unless `force=true` |
| `md_unlink_account` | `{ folderId }` | `()` | removes junction, recreates empty 257-dir scaffolding |
| `md_is_linked` | `{ folderId }` | `bool` | wraps `junction::exists`; missing path → `false`, not error |
| `md_save_metadata` | `{ folderId, accountName, accountType }` | `()` | upsert `accounts.csv` |
| `md_is_running` | – | `bool` | `masterduel.exe` in process list |

### Discovery

| Command | Returns | Notes |
|---|---|---|
| `list_supported_games` | `GameInfo[]` | `[]` on macOS/Linux for v0.1 |

### Frontend types (`src/lib/api/types.ts`)

```ts
export interface SteamAccount {
  accountName: string;
  personaName: string;
  steamId64: string;          // STRING — JS can't hold 64-bit ints precisely
  steamId32: number;
  rememberPassword: boolean;
  mostRecent: boolean;
  wantsOfflineMode: boolean;
  skipOfflineModeWarning: boolean;
  allowAutoLogin: boolean;
  timestamp: number;
  avatarUrl?: string;         // v0.2
}

export interface MdAccount {
  folderId: string;
  accountName: string;        // empty if not in accounts.csv
  accountType: "main" | "sub" | "";
  isLinked: boolean;
}

export interface GameInfo {
  id: string;                 // "master_duel"
  displayName: string;        // "Yu-Gi-Oh! Master Duel"
  installed: boolean;
}

export type AppError =
  | { kind: "SteamNotInstalled" | "GameNotInstalled" | "GameRunning"
        | "VdfParse" | "RegistryWrite" | "JunctionFailed"
        | "AccountNotFound" | "ProcessKillFailed" | "Io"; msg: string };
```

**Frontend rule:** components never call `invoke` directly. Only `src/lib/api/*.ts` does.

### Threading

All commands are `async fn` but most do synchronous filesystem IO (fast). The Steam switch sequence (kills Steam, ~500 ms wait) runs under `tokio::task::spawn_blocking` so it doesn't block the runtime. **No global state** — each command re-reads the filesystem.

---

## Steam: switch algorithm

### Config locations

| OS | Steam install | `loginusers.vdf` | "Registry" |
|---|---|---|---|
| Win | `HKCU\Software\Valve\Steam\SteamPath` | `<install>\config\loginusers.vdf` | Win HKCU `Software\Valve\Steam` |
| Linux | `~/.steam/steam/` | `<install>/config/loginusers.vdf` | `~/.steam/registry.vdf` |
| macOS | `~/Library/Application Support/Steam/` | `<install>/config/loginusers.vdf` | `<install>/registry.vdf` |

⚠️ `registry.vdf` is **NOT** the Windows registry — it's a Valve-format text file Steam uses on Linux/macOS as the cross-platform analog. No Windows registry concept on those OSes.

### `loginusers.vdf` schema

```
"users"
{
    "76561198XXXXXXXXX"          // SteamID64 — block key
    {
        "AccountName"            "username"
        "PersonaName"            "Displayed Name"
        "RememberPassword"       "1"
        "WantsOfflineMode"       "0"
        "SkipOfflineModeWarning" "0"
        "AllowAutoLogin"         "1"
        "MostRecent"             "1"     // exactly one user has this set
        "Timestamp"              "1700000000"
    }
    ...
}
```

All values are strings even when semantically int/bool — parse explicitly.

### `switch_account(account_name)` sequence

1. Read `loginusers.vdf` → `HashMap<AccountName, SteamUser>`.
2. Locate target or `Err(AccountNotFound)`.
3. **Kill Steam** — SIGTERM (Win: `Steam.exe`; Linux: `steam`; macOS: `steam_osx` + helpers). Poll up to 3000 ms, force-kill if still running. Idempotent.
4. **Write Steam-internal "registry":**
   - Win (`winreg`):
     - `AutoLoginUser` ← `account_name` (REG_SZ)
     - `AutoLoginUser_steamchina` ← `account_name` (REG_SZ)
     - `RememberPassword` ← `1` (DWORD)
   - Linux/macOS: open `registry.vdf`, traverse `Registry > HKCU > Software > Valve > Steam`, set the same 3 keys, **write atomically** (tempfile + rename).
5. **Update `loginusers.vdf`:**
   - For target: `MostRecent=1`, `RememberPassword=1`, optionally `WantsOfflineMode`/`SkipOfflineModeWarning` from args.
   - For all others: `MostRecent=0`.
   - **Write atomically** (tempfile + rename) to avoid partial-write corruption.
6. **Spawn Steam:**
   - Win: `Command::new(steam_path.join("Steam.exe")).spawn()?`
   - Linux: `Command::new("steam").spawn()?`
   - macOS: `Command::new("open").arg("-a").arg("Steam").spawn()?`

Wrap the full sequence in `tokio::task::spawn_blocking`.

### `ssfn*` files — DO NOT TOUCH

Located at `<install>\ssfn*` (Win/Mac) or `~/.steam/ssfn*` (Linux). These ARE the credentials. Steam manages them.

If the target account has no `ssfn`, the switch lands on the login screen with the username pre-filled. **That's expected, not an error.**

### Edge cases (must handle)

| Condition | Behavior |
|---|---|
| `loginusers.vdf` missing | `SteamNotInstalled` |
| Target name not in file | `AccountNotFound` (we don't add accounts; user must do manual login first) |
| No `ssfn*` for target | Switch succeeds; Steam prompts for password |
| Steam already closed | `kill_steam()` returns Ok, skip wait |
| SIGTERM ignored after 3 s | Force kill, log warning |
| Steam Guard / 2FA prompt | Steam's dialog appears post-relaunch; we don't intercept |
| `MostRecent=1` already on target | Still rewrite (idempotent) |
| Duplicate `AccountName` in VDF | Pick highest `Timestamp` |

### Persona state (v0.2)

8 values, numeric `0..7`: Offline, Online, Busy, Away, Snooze, LookingToTrade, LookingToPlay, Invisible. v0.1 ignores the arg if passed.

### Offline mode (v0.1)

`offlineMode: true` → write `WantsOfflineMode=1` + `SkipOfflineModeWarning=1` on the target user.

---

## Master Duel: link algorithm (Windows-only)

### Install discovery

1. Read `<steam>\steamapps\libraryfolders.vdf` → all library locations.
2. For each: check `<library>\steamapps\common\Yu-Gi-Oh!  Master Duel\masterduel.exe`.
3. First match wins. None → `Err(GameNotInstalled("master_duel"))`.

⚠️ **Two literal spaces** between `Yu-Gi-Oh!` and `Master`. Konami fixed it that way. Don't normalize.

### Folder layout

```
<install>\
├── masterduel.exe
├── LocalData\
│   ├── DATA\0000\         ← shared cache (~13 GB, ~37k files)
│   │   ├── 00..ff\          (256 hex bucket dirs, sharded by hash byte 0)
│   │   └── root\
│   ├── 9b7470c4\0000\     ← per-Konami-account; replace with junction
│   ├── 42a06a1e\0000\
│   └── ...
└── LocalSave\             ← per-account save data — DO NOT TOUCH
```

Each per-account `0000\` is structurally identical to `DATA\0000\`. Empty for fresh accounts (~48 KB, just the 257 dirs). MD doesn't detect or care about the junction.

### Why junctions, not symlinks

| | Junction `/J` | Symlink `/D` |
|---|---|---|
| Admin? | **No** | Yes (or Dev Mode) |
| Same volume only | Yes | No |
| Crate | `junction` 1.2 | n/a |

Same-volume restriction is irrelevant (MD and `DATA\` are co-located). Admin-free creation is decisive.

```rust
junction::create(target_dir, junction_path)?;  // junction_path → target_dir
junction::exists(path)?;                        // true iff path is a junction
```

### `md_link_account(folder_id, force=false)`

```
1. install = paths::find_install()?
2. shared  = install/"LocalData/DATA/0000"
   if !shared.exists(): Err(GameNotInstalled("master_duel: DATA missing"))
3. acct = install/"LocalData"/folder_id/"0000"
   if !acct.parent().exists(): Err(AccountNotFound(folder_id))
4. if junction::exists(&acct)? { return Ok(()) }   // idempotent
5. if acct.exists() {
       if process::is_master_duel_running()?: Err(GameRunning("master_duel"))
       has_files = walkdir(&acct).any(is_file)
       if has_files && !force: Err(JunctionFailed("0000 contains files; use force=true"))
       fs::remove_dir_all(&acct)?
   }
6. junction::create(&shared, &acct)?
```

### `md_unlink_account(folder_id)`

```
1. acct = install/"LocalData"/folder_id/"0000"
2. if !junction::exists(&acct)?: Err(JunctionFailed("not a junction"))
3. if process::is_master_duel_running()?: Err(GameRunning("master_duel"))
4. fs::remove_dir(&acct)?              // REMOVES JUNCTION ONLY — never touches target
5. fs::create_dir(&acct)?
   for i in 0..=255u8: fs::create_dir(acct/format!("{:02x}", i))?
   fs::create_dir(acct/"root")?
```

⚠️ **Use `remove_dir`, NOT `remove_dir_all`.** `remove_dir_all` on a junction walks through and deletes the *target's* contents — i.e. the shared `DATA\0000\`. Catastrophic.

### `is_master_duel_running`

```rust
let mut sys = sysinfo::System::new();
sys.refresh_processes();
sys.processes_by_exact_name("masterduel.exe").next().is_some()
```

### `accounts.csv` schema

Default `<install>\accounts.csv` (configurable). Plain CSV with header:

```
folder_id,account_type,account_name
9b7470c4,main,キラーチューン
42a06a1e,sub,閃刀姫
5abe0b9,sub,HERO              # leading 0 may be stripped — IDs are 7 OR 8 hex chars
ff5f260a,sub,M∀LICE           # special unicode chars
```

| Column | Notes |
|---|---|
| `folder_id` | hex, 7 or 8 chars (leading zero may be stripped) |
| `account_type` | `main` \| `sub` \| empty. UI may warn before linking `main`. |
| `account_name` | UTF-8 free-form; CJK + special chars allowed |

`md_list_accounts`: cross-reference `LocalData\` subdirs (excluding `DATA`) with `accounts.csv`. Folders not in CSV come back with empty fields — frontend prompts for a name.

---

## Critical gotchas (the "don't forget" list)

1. **`tauri::generate_handler!` doesn't accept `#[cfg]` per item.** Workaround: two `Builder::default()` branches with conditional compilation, or a helper macro. Verify Tauri 2 current best practice.
2. **Atomic file writes** for both `loginusers.vdf` and `registry.vdf` — tempfile + rename. Corrupting `loginusers.vdf` loses the user's account list.
3. **`steamId64` is a STRING in TS**, not a number — JS can't hold 64-bit ints precisely.
4. **`force=true` on `md_link_account`** silently destroys 13 GB of asset cache if misused. Default `false`. Frontend must explicitly opt in per click.
5. **`fs::remove_dir` vs `remove_dir_all` on junction unlink** — see warning above. Wrong one nukes the shared cache.
6. **Two spaces** in `Yu-Gi-Oh!  Master Duel` folder name. Don't normalize whitespace.
7. **Don't touch `ssfn*` files** ever — they ARE the credentials.
8. **Don't touch `LocalSave\`** — that's user save data.
9. **`registry.vdf` is not the Windows registry** despite the name.
10. **Never run tests against the user's real Steam install.** Always use `tempfile::tempdir()`.
11. **`md_is_linked` on a missing path returns `false`, not an error.**
12. **VDF values are strings** even when semantically int/bool — parse explicitly.
13. **Duplicate `AccountName` in VDF** (rare/malformed): pick highest `Timestamp`.

---

## Testing

### Layers

1. **Unit** — `#[cfg(test)] mod tests` inline per module, fixtures in `src-tauri/tests/fixtures/{steam,master_duel}/`.
2. **Integration** — `src-tauri/tests/integration_*.rs` against `tempfile::tempdir()`.
3. **Manual E2E** — checklist below per release.

### Fixtures to create

```
src-tauri/tests/fixtures/
├── steam/
│   ├── loginusers.vdf            # 3 accounts, one MostRecent=1
│   ├── registry.vdf              # Linux/macOS sample
│   └── empty_loginusers.vdf      # edge case
└── master_duel/
    ├── accounts.csv              # 9 rows including CJK + special chars
    └── localdata_layout/         # synthesized 0000 dir
```

### v0.1 manual E2E checklist (real Win11 install)

1. Fresh install. Open steam-mate. `/steam` shows real Steam accounts.
2. Click an account. Steam closes (visible). New Steam launches signed in to clicked account.
3. `clear_login`. Click another account. Steam relaunches at login screen with username pre-filled.
4. Open `/games/master-duel`. All Konami account folders show. CSV content visible.
5. Toggle "linked" on a non-main account. Verify in Explorer that `LocalData\<id>\0000` is a junction (Properties → Target points to DATA).
6. Try toggling while MD is running → red error toast "game is running". Close MD → toggle works.
7. Untoggle. Verify folder is back to a normal directory with 256 hex buckets + `root`.

---

## Out of scope (forever)

These overlap with Watt Toolkit but steam-mate explicitly will **not** ship them:

- TOTP / Steam Guard authenticator (security-sensitive; punt to dedicated tools)
- Network accelerator / proxy
- Game store / library browsing
- Trade / market features
- Inventory / screenshot management

The whole point is doing two things well, not twenty things loosely.

---

## v0.2+ at a glance (not v0.1 work)

**v0.2 — polish + cross-platform Steam:**
Linux/macOS path discovery, `registry.vdf` patching (replaces `winreg` on those OSes), persona state on switch, offline-mode toggle, avatar fetching via Steam Web API (cached), per-account remarks, MD filesystem watcher, disk-space estimate, bulk link/unlink, system tray quick-switch, settings page, GH Actions matrix.

**v0.3 — Watt Toolkit parity:**
Family Share manager, desktop-shortcut composition with avatar `.ico`, beyond-Steam platforms (Battle.net/EGS/Riot via JSON config), account import/export, manage `loginusers.vdf` directly (delete remembered user, mark `AllowAutoLogin=0`).

**Stretch:** CLI mode (`steam-mate --switch <account>`), headless tray-only daemon, Tauri auto-updater, FFXIV/Genshin game modules, localized UI (zh-CN, ja, en).
