# steam-mate

A small Windows and macOS desktop app for switching between remembered Steam
accounts. On Windows, it can also share Yu-Gi-Oh! Master Duel's ~13 GB asset
cache across every account on the machine.

Built with Tauri 2 (Rust) and SvelteKit. The setup file is under 2 MB and the
app binary under 5 MB. The interface ships in English, 简体中文, 繁體中文, and
日本語, with seven color presets, each in light and dark.

| Feature | Windows | macOS |
|---|:---:|:---:|
| Steam account management | ✓ | ✓ |
| Yu-Gi-Oh! Master Duel tools | ✓ | — |
| Rich tray popup | ✓ | — |

## Steam

- One list of every remembered account, with real profile avatars and the
  active account marked. Accounts that share a display name fold into one group.
- **Double-click an account to switch**: the app closes Steam, points
  auto-login at the target, rewrites `loginusers.vdf` atomically, and
  relaunches signed in. An offline-mode checkbox rides along with the switch.
- Refresh re-reads the list (also when the window regains focus). Clear
  auto-login makes the next launch stop at the login screen.
- Delete offers two depths: hide the row in steam-mate only (reversible), or
  forget the login on this disk — the same thing Steam's own "Forget" does.

## Yu-Gi-Oh! Master Duel (Windows only)

- Each game profile folder is listed with its name and a **link toggle**:
  linked profiles read the shared cache (`LocalData\DATA\0000`) through an
  NTFS folder link — the kind `mklink /J` makes — instead of holding their own
  ~13 GB copy.
- **Link all / Unlink all** act on every profile at once; folders that still
  hold their own files are skipped, never forced.
- Every profile can be **assigned the Steam account it belongs to**; avatars
  and the delete dialog follow that assignment, and profiles with no current
  owner show a "no Steam account" badge. Names that match exactly one Steam
  account assign themselves.
- Delete removes a profile's data, its saves, and its bookkeeping row —
  irreversibly, with export offered first — and can also forget the assigned
  Steam login.
- Export writes the account list as JSON. Sort by non-linked first, newest, or
  name. Every control locks while the game is running.

## Where things live

No database — plain files only:

| Data | File |
|---|---|
| Steam's remembered logins | Windows: `<steam>\config\loginusers.vdf`; macOS: `~/Library/Application Support/Steam/config/loginusers.vdf` (Steam's own file) |
| Steam auto-login state | Windows Registry; macOS: `~/Library/Application Support/Steam/registry.vdf` |
| Profile names + Steam assignment (Windows) | `<game>\accounts.csv` — `folder_id,account_name,steam_login`; older files migrate in place |
| Avatar cache | The platform application-data directory under `steam-mate/avatars/` |
| Preferences (language, theme, hidden rows) | the app's local storage |

## Build from source

Prerequisites: Rust 1.80+, Node 20+, pnpm.

```
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # native application bundle or installer
cargo test --manifest-path src-tauri/Cargo.toml
```

## Planned

- An in-app updater, once releases are hosted.

## License

MIT.
