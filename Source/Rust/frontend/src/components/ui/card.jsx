import { cn } from "../../lib/utils";

export function Card({ className, ...props }) {
  return <section data-slot="card" data-glass className={cn("pv-card min-w-0", className)} {...props} />;
}
