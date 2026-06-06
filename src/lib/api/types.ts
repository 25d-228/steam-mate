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
}

export interface GameInfo {
  id: string;
  displayName: string;
  installed: boolean;
}

export interface MdAccount {
  folderId: string;
  accountName: string;
  isLinked: boolean;
  steamLogin: string;
}

export type AppErrorKind =
  | "SteamNotInstalled"
  | "VdfParse"
  | "RegistryWrite"
  | "Io"
  | "GameNotInstalled"
  | "GameRunning"
  | "JunctionFailed"
  | "AccountNotFound"
  | "ProcessKillFailed";

export type AppError = { kind: AppErrorKind; msg?: string };

/** Narrow an unknown caught value to AppError shape when possible. */
export function asAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e) {
    return e as AppError;
  }
  return { kind: "Io", msg: typeof e === "string" ? e : String(e) };
}
