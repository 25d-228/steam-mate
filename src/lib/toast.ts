// Global two-stage toast store.
//
// Mirrors the prototype's `toast(loading, done, isErr)`:
//   - normal: shows `loading` with a spinner, then after ~900ms swaps to `done`
//     (spinner hidden) and auto-hides ~1700ms later.
//   - error: shows `done` immediately in the error style, auto-hides ~2600ms.
// A loading-stage with an empty string just shows `done` directly (no spinner
// delay) — matching the prototype calls like toast('', t('toastHide')).

import { writable } from "svelte/store";

export interface ToastState {
  show: boolean;
  /** done (final / immediate) message */
  msg: string;
  /** true while the spinner / loading stage is active */
  loading: boolean;
  err: boolean;
}

export const toastState = writable<ToastState>({
  show: false,
  msg: "",
  loading: false,
  err: false,
});

let timer: ReturnType<typeof setTimeout> | undefined;

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
    toastState.set({ show: true, msg: done, loading: false, err: true });
    timer = setTimeout(
      () => toastState.update((s) => ({ ...s, show: false })),
      2600,
    );
    return;
  }
  if (!loading) {
    toastState.set({ show: true, msg: done, loading: false, err: false });
    timer = setTimeout(
      () => toastState.update((s) => ({ ...s, show: false })),
      1700,
    );
    return;
  }
  toastState.set({ show: true, msg: loading, loading: true, err: false });
  timer = setTimeout(() => {
    toastState.set({ show: true, msg: done, loading: false, err: false });
    timer = setTimeout(
      () => toastState.update((s) => ({ ...s, show: false })),
      1700,
    );
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
  toastState.set({ show: true, msg: loading, loading: true, err: false });
}
