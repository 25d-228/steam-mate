// Copy text to the clipboard.
//
// navigator.clipboard needs a secure context and can be absent or rejected in
// webviews, so a hidden-textarea execCommand("copy") fallback covers the rest.
// Resolves true only when a copy path reported success — callers decide how to
// toast, so success and failure feedback stays consistent across pages.
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to the legacy path */
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}
