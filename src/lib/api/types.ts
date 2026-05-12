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

export type AppError =
  | { kind: "SteamNotInstalled" | "VdfParse" | "RegistryWrite" | "Io"
        | "GameNotInstalled" | "GameRunning" | "JunctionFailed"
        | "AccountNotFound" | "ProcessKillFailed"; msg?: string };
