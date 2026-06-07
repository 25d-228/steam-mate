// Colored-initial fallback tiles for account avatars.
//
// Every surface that renders an avatar (Steam page, Master Duel page, sidebar
// chip, tray popup) shows the same fallback while the real image loads or when
// none exists: a tile colored stably from the account name, holding the
// display name's first letter. One home for that mapping keeps the colors
// identical across surfaces — the folder-group colors in card view reuse
// hue() too, so a group and its members always agree.

const AV_COLORS = [
  "#268bd2",
  "#2aa198",
  "#6c71c4",
  "#b58900",
  "#d33682",
  "#cb4b16",
];

/** First letter of a display name, uppercased; "?" when empty. */
export function initial(s: string): string {
  return (s.trim()[0] || "?").toUpperCase();
}

/** A stable palette color derived from a name. */
export function hue(s: string): string {
  let h = 0;
  for (const c of s) h = (h * 31 + (c.codePointAt(0) ?? 0)) % AV_COLORS.length;
  return AV_COLORS[h];
}
