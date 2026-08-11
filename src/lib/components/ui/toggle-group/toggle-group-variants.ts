import { type VariantProps, tv } from "tailwind-variants";

export const toggleVariants = tv({
	base: "inline-flex items-center justify-center gap-1 whitespace-nowrap rounded-lg text-sm font-medium transition-all outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
	variants: {
		variant: {
			default: "bg-transparent data-[state=on]:bg-muted",
			outline: "border border-input bg-transparent data-[state=on]:bg-primary data-[state=on]:text-primary-foreground",
		},
		size: {
			default: "h-8 min-w-8 px-2.5",
			sm: "h-7 min-w-7 px-2.5 text-[0.8rem]",
			lg: "h-9 min-w-9 px-2.5",
		},
	},
	defaultVariants: {
		variant: "default",
		size: "default",
	},
});

export type ToggleVariants = VariantProps<typeof toggleVariants>;
