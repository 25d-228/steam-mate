// Global two-stage toast API, presented by the shared Sonner toaster.
//
// Mirrors the prototype's `toast(loading, done, isErr)`:
//   - normal: shows `loading` with a spinner, then after ~900ms swaps to `done`
//     (spinner hidden) and auto-hides ~1700ms later.
//   - error: shows `done` immediately in the error style, auto-hides ~2600ms.
// A loading-stage with an empty string just shows `done` directly (no spinner
// delay) — matching the prototype calls like toast('', t('toastHide')).

import { toast as sonner } from "svelte-sonner";

let timer: ReturnType<typeof setTimeout> | undefined;
const TOAST_ID = "steam-mate-global";

function clear() {
  if (timer) {
    clearTimeout(timer);
    timer = undefined;
  }
}

/**
 * Show a toast.
 * @param loading message for the loading stage (empty ⇒ skip straight to done)
 * @param done    final message
 * @param isErr   show in the error style immediately
 */
export function toast(loading: string, done: string, isErr = false) {
  clear();
  if (isErr) {
    sonner.error(done, { id: TOAST_ID, duration: 2600 });
    return;
  }
  if (!loading) {
    sonner.success(done, { id: TOAST_ID, duration: 1700 });
    return;
  }
  sonner.loading(loading, { id: TOAST_ID, duration: Infinity });
  timer = setTimeout(() => {
    sonner.success(done, { id: TOAST_ID, duration: 1700 });
    timer = undefined;
  }, 900);
}

/**
 * Show a spinner toast that stays up until the next `toast(...)`/`toastLoading`
 * call replaces it. Use for async work whose success/failure isn't known yet —
 * unlike {@link toast}, it never auto-swaps to a "done" message on a timer, so
 * the caller can show the real outcome only after the work resolves.
 */
export function toastLoading(loading: string) {
  clear();
  sonner.loading(loading, { id: TOAST_ID, duration: Infinity });
}
