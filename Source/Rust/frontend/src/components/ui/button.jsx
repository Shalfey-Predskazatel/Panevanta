import { Slot } from "@radix-ui/react-slot";
import { cva } from "class-variance-authority";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "pv-button inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap disabled:pointer-events-none disabled:opacity-55 [&_svg]:pointer-events-none",
  {
    variants: {
      variant: { default: "pv-primary", glass: "pv-glass", ghost: "pv-ghost", danger: "pv-danger" },
      size: { default: "h-10 px-4", icon: "size-9", launch: "h-12 w-full px-5" },
    },
    defaultVariants: { variant: "glass", size: "default" },
  },
);

export function Button({ className, variant, size, asChild = false, ...props }) {
  const Component = asChild ? Slot : "button";
  return <Component data-slot="button" data-glass className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
