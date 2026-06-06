// Map an AppError to a localized toast message and show it.
//
// Known kinds get a dedicated string (GameRunning → errRunning, etc.); unknown
// kinds fall back to the kind text plus any backend message.

import { asAppError } from "./api/types";
import { tNow, fmt } from "./i18n";
import { toast } from "./toast";

/** Localized message for an AppError-shaped value. */
export function errorMessage(e: unknown): string {
  const err = asAppError(e);
  switch (err.kind) {
    case "GameRunning":
      return tNow("errRunning");
    case "SteamNotInstalled":
      return tNow("errSteamNotInstalled");
    case "GameNotInstalled":
      return fmt(tNow("errGameNotInstalled"), { game: err.msg ?? "" });
    default:
      // Unknown kinds show the kind text (plus backend detail when present).
      return err.msg ? `${err.kind} — ${err.msg}` : err.kind;
  }
}

/** Show an error toast for a caught value. */
export function toastError(e: unknown) {
  toast("", errorMessage(e), true);
}
