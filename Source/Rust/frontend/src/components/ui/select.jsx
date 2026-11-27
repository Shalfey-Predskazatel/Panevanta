import React from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown, ChevronUp } from "lucide-react";

export function Select({ id, value, onValueChange, disabled, placeholder, children }) {
  return (
    <SelectPrimitive.Root value={String(value)} onValueChange={onValueChange} disabled={disabled}>
      <SelectPrimitive.Trigger id={id} data-glass className="pv-select">
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon asChild><ChevronDown size={15} aria-hidden="true" /></SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content className="pv-select-menu" position="popper" sideOffset={5} collisionPadding={10}>
          <SelectPrimitive.ScrollUpButton className="pv-select-scroll"><ChevronUp size={14} /></SelectPrimitive.ScrollUpButton>
          <SelectPrimitive.Viewport className="pv-select-viewport">{children}</SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton className="pv-select-scroll"><ChevronDown size={14} /></SelectPrimitive.ScrollDownButton>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

export function SelectGroup({ label, children }) {
  return <SelectPrimitive.Group><SelectPrimitive.Label className="pv-select-label">{label}</SelectPrimitive.Label>{children}</SelectPrimitive.Group>;
}

export function SelectItem({ value, children }) {
  return (
    <SelectPrimitive.Item className="pv-select-item" data-glass value={String(value)}>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
      <SelectPrimitive.ItemIndicator className="pv-select-check"><Check size={15} aria-hidden="true" /></SelectPrimitive.ItemIndicator>
    </SelectPrimitive.Item>
  );
}
