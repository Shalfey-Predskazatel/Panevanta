import * as SwitchPrimitive from "@radix-ui/react-switch";
import { cn } from "../../lib/utils";

export function Switch({ className, ...props }) {
  return (
    <SwitchPrimitive.Root data-slot="switch" data-glass className={cn("pv-switch inline-flex shrink-0 items-center", className)} {...props}>
      <SwitchPrimitive.Thumb data-slot="switch-thumb" className="pv-switch-thumb pointer-events-none block" />
    </SwitchPrimitive.Root>
  );
}
