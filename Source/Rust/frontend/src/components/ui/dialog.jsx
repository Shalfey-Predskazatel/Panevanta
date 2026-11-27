import React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { Button } from "./button";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;

export function DialogContent({ title, description, children }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="pv-dialog-scrim" />
      <DialogPrimitive.Content className="pv-dialog" data-glass>
        <div className="pv-dialog-head">
          <div><DialogPrimitive.Title>{title}</DialogPrimitive.Title><DialogPrimitive.Description>{description}</DialogPrimitive.Description></div>
          <DialogPrimitive.Close asChild><Button size="icon" variant="ghost" aria-label="Закрыть настройки" title="Закрыть настройки"><X size={18} /></Button></DialogPrimitive.Close>
        </div>
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
