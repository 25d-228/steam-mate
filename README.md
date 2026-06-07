# steam-mate

A small Windows desktop app that does two things well: switch between your
remembered Steam accounts, and share Yu-Gi-Oh! Master Duel's ~13 GB asset
cache across every account on the machine.

Built with Tauri 2 (Rust) and SvelteKit. The setup file is under 2 MB and the
app binary under 5 MB. The interface ships in English, 简体中文, 繁體中文, and
日本語, with four color presets, each in light and dark.

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

## Yu-Gi-Oh! Master Duel

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
| Steam's remembered logins | `<steam>\config\loginusers.vdf` (Steam's own file) |
| Profile names + Steam assignment | `<game>\accounts.csv` — `folder_id,account_name,steam_login`; older files migrate in place |
| Avatar cache | `%APPDATA%\steam-mate\avatars\` |
| Preferences (language, theme, hidden rows) | the app's local storage |

## Build from source

Prerequisites: Rust 1.80+, Node 20+, pnpm.

```
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # installer (.msi + setup.exe)
cargo test --manifest-path src-tauri/Cargo.toml
```

## Planned

- **Batch delete** — select several accounts in either list and delete them in
  one action.
- **Create the shared cache** — today the app expects `LocalData\DATA\0000` to
  exist (set up by hand). The app will build it itself, seeding from an
  existing profile's data, so linking works on a fresh machine.
- **Show the signed-in account in the assignment menu** — each Master Duel
  dropdown marks the Steam account that is currently logged in.
- **Card / list view** — both panels can switch between the current list and a
  card grid (large avatars, status at a glance); the choice is remembered.
- **Reveal in folder** — a button on the shared-cache box opens
  `LocalData\DATA\0000` in File Explorer.
- **Copy the install path** — a button next to each "Installed at" line puts
  the exact path on the clipboard. Selecting the label by hand drops one of
  the two spaces in `Yu-Gi-Oh!  Master Duel` and yields a path that does not
  exist.
- A tray menu for switching without opening the window, and a "signed in as"
  line in the header.
- An in-app updater, once releases are hosted.

## License

MIT.
