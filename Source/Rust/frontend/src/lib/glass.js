import { useEffect } from "react";

export const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

export function useGlass(enabled) {
  useEffect(() => {
    if (!enabled || !window.matchMedia("(any-pointer: fine)").matches) return;
    let target = null;
    let pointer = null;
    let frame = 0;
    let rect = null;

    function clear() {
      if (target) {
        target.removeAttribute("data-lit");
        target = null;
      }
      pointer = null;
      rect = null;
    }

    function paint() {
      frame = 0;
      if (!pointer) return;
      const { element, x, y } = pointer;
      if (target !== element) {
        target?.removeAttribute("data-lit");
        target = element;
        rect = null;
      }
      rect ??= target.getBoundingClientRect();
      if (!rect.width || !rect.height) return;
      const dx = x - rect.left;
      const dy = y - rect.top;
      target.style.setProperty("--spot-x", `${Math.round(dx)}px`);
      target.style.setProperty("--spot-y", `${Math.round(dy)}px`);
      if (!target.hasAttribute("data-lit")) target.setAttribute("data-lit", "");
    }

    function track(event) {
      if (event.pointerType === "touch") return;
      const element = event.target instanceof Element && event.target.closest("[data-glass]");
      if (!element || element.matches(":disabled,[data-disabled]")) { clear(); return; }
      pointer = { element, x: event.clientX, y: event.clientY };
      if (!frame) frame = requestAnimationFrame(paint);
    }

    function invalidate() { rect = null; }

    document.addEventListener("pointermove", track, { passive: true });
    document.documentElement.addEventListener("pointerleave", clear);
    document.addEventListener("scroll", invalidate, { capture: true, passive: true });
    window.addEventListener("resize", invalidate);
    window.addEventListener("blur", clear);
    return () => {
      clear();
      cancelAnimationFrame(frame);
      document.removeEventListener("pointermove", track);
      document.documentElement.removeEventListener("pointerleave", clear);
      document.removeEventListener("scroll", invalidate, true);
      window.removeEventListener("resize", invalidate);
      window.removeEventListener("blur", clear);
    };
  }, [enabled]);
}
