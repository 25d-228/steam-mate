<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { Snippet } from "svelte";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		child,
		isActive = false,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLButtonElement>, HTMLButtonElement> & {
		isActive?: boolean;
		child?: Snippet<[{ props: Record<string, unknown> }]>;
	} = $props();

	const buttonProps = $derived({
		class: cn(
			"peer/menu-button flex h-8 w-full items-center gap-2 overflow-hidden rounded-md p-2 text-left text-sm outline-hidden transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring active:bg-sidebar-accent active:text-sidebar-accent-foreground data-active:bg-sidebar-accent data-active:font-medium data-active:text-sidebar-accent-foreground disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0 [&>span:last-child]:truncate",
			className
		),
		"data-slot": "sidebar-menu-button",
		"data-sidebar": "menu-button",
		"data-active": isActive,
		...restProps,
	});
</script>

{#if child}
	{@render child({ props: buttonProps })}
{:else}
	<button bind:this={ref} {...buttonProps}>
		{@render children?.()}
	</button>
{/if}
