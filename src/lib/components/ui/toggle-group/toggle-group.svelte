<script lang="ts" module>
	import { getContext, setContext } from "svelte";
	import type { ToggleVariants } from "./toggle-group-variants.js";

	interface ToggleGroupContext extends ToggleVariants {
		spacing?: number;
		orientation?: "horizontal" | "vertical";
	}

	function setToggleGroupCtx(props: ToggleGroupContext) {
		setContext("toggleGroup", props);
	}

	export function getToggleGroupCtx() {
		return getContext<Required<ToggleGroupContext>>("toggleGroup");
	}
</script>

<script lang="ts">
	import { ToggleGroup as ToggleGroupPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		value = $bindable(),
		class: className,
		size = "default",
		spacing = 0,
		orientation = "horizontal",
		variant = "default",
		required = false,
		onValueChange,
		...restProps
	}: ToggleGroupPrimitive.RootProps &
		ToggleVariants & {
			spacing?: number;
			orientation?: "horizontal" | "vertical";
			required?: boolean;
		} = $props();

	let committedValue = value;

	$effect(() => {
		if (typeof value === "string" ? value.length > 0 : (value?.length ?? 0) > 0) {
			committedValue = value;
		}
	});

	function handleValueChange(next: string | string[]) {
		if (required && next.length === 0) {
			value = committedValue as never;
			return;
		}

		value = next as never;
		const notify = onValueChange as ((newValue: string | string[]) => void) | undefined;
		notify?.(next);
	}

	setToggleGroupCtx({
		get variant() {
			return variant;
		},
		get size() {
			return size;
		},
		get spacing() {
			return spacing;
		},
		get orientation() {
			return orientation;
		},
	});
</script>

<!--
Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<ToggleGroupPrimitive.Root
	bind:value={value as never}
	onValueChange={handleValueChange as never}
	bind:ref
	{orientation}
	data-slot="toggle-group"
	data-variant={variant}
	data-size={size}
	data-spacing={spacing}
	style={`--gap: ${spacing}`}
	class={cn(
		"rounded-lg data-[size=sm]:rounded-[min(var(--radius-md),10px)] group/toggle-group flex w-fit flex-row items-center gap-[--spacing(var(--gap))] data-vertical:flex-col data-vertical:items-stretch",
		className
	)}
	{...restProps}
/>
