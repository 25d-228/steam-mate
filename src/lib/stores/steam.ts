// Shared Steam-accounts store.
//
// The Master Duel page matches an account label to a Steam persona to show the
// same avatar and to offer "also forget the Steam account". Rather than have the
// MD page reimplement the Steam fetch, both pages read/refresh this store.

import { writable, get } from "svelte/store";
import { listAccounts } from "../api/steam";
import type { SteamAccount } from "../api/types";

export const steamAccounts = writable<SteamAccount[]>([]);

/** Fetch the Steam account list into the store (best effort). */
export async function refreshSteamAccounts(): Promise<SteamAccount[]> {
  const list = await listAccounts();
  steamAccounts.set(list);
  return list;
}

/** Ensure the store is populated at least once. */
export async function ensureSteamAccounts(): Promise<SteamAccount[]> {
  const cur = get(steamAccounts);
  if (cur.length) return cur;
  try {
    return await refreshSteamAccounts();
  } catch {
    return [];
  }
}

/**
 * First Steam account whose persona matches `name`, or undefined.
 *
 * An empty `name` never matches: an unnamed Master Duel account ("") must not
 * resolve to a Steam login whose PersonaName happens to be empty, or the delete
 * dialog would offer to forget an unrelated real account.
 */
export function steamByPersona(
  list: SteamAccount[],
  name: string,
): SteamAccount | undefined {
  if (!name) return undefined;
  return list.find((s) => s.personaName === name);
}

/**
 * The Steam account whose accountName (login) equals `login`, or undefined.
 *
 * Used by the Master Duel page to resolve a profile's stored `steamLogin` to a
 * real current account — for its avatar, its selector value, and the delete
 * dialog's "also forget the Steam account" option. An empty `login` (unassigned)
 * never matches.
 */
export function steamByLogin(
  list: SteamAccount[],
  login: string,
): SteamAccount | undefined {
  if (!login) return undefined;
  return list.find((s) => s.accountName === login);
}
