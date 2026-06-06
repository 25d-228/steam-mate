<script lang="ts">
  // Global toast — two-stage loading→done, plus the error variant.
  // Ported from the prototype's #toast element + toast() function; state lives
  // in the shared toast store so any module can trigger it.
  import { toastState } from "$lib/toast";

  let s = $derived($toastState);
</script>

<div
  class="toast"
  class:show={s.show}
  class:ok={!s.loading}
  class:err={s.err}
  role="status"
  aria-live="polite"
>
  <span class="sp"></span><span class="msg">{s.msg}</span>
</div>
