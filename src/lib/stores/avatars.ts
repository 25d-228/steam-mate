// Shared avatar cache.
//
// Both the Steam and Master Duel pages show real avatars. The Steam page fetches
// one per account (by steamId64) after the list renders; the Master Duel page
// matches an account by persona name to a Steam account and reuses that fetch.
// Avatars are cached here (steamId64 → data URI | null) so a switch between pages
// doesn't refetch, and a reactive store lets rows update when a URI arrives.

import { writable, get } from "svelte/store";
import { getAvatar } from "../api/steam";

/** steamId64 → data URI (or null when none / failed). */
export const avatars = writable<Record<string, string | null>>({});

const inFlight = new Set<string>();

/** Fetch an avatar once per steamId64; updates the store when it arrives. */
export async function fetchAvatar(steamId64: string): Promise<void> {
  if (!steamId64) return;
  const cache = get(avatars);
  if (steamId64 in cache || inFlight.has(steamId64)) return;
  inFlight.add(steamId64);
  try {
    const uri = await getAvatar(steamId64);
    avatars.update((m) => ({ ...m, [steamId64]: uri }));
  } catch {
    avatars.update((m) => ({ ...m, [steamId64]: null }));
  } finally {
    inFlight.delete(steamId64);
  }
}
