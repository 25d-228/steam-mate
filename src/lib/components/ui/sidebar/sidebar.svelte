<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
	bind:this={ref}
	data-sidebar="sidebar"
	data-slot="sidebar"
	class={cn(
		"flex h-full w-(--sidebar-width) shrink-0 flex-col bg-sidebar text-sidebar-foreground",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
