import { useRef, useState, type PointerEvent } from "react";
export interface DragItem {
  id: string;
  folder: boolean;
  name: string;
}
export default function useAssetDrag(
  locked: boolean,
  canDrop: (item: DragItem, target: string | null) => boolean,
  onDrop: (item: DragItem, target: string | null, copy: boolean) => void,
) {
  const gesture = useRef<{
    item: DragItem;
    x: number;
    y: number;
    active: boolean;
  } | null>(null);
  const suppressClick = useRef(false);
  const [preview, setPreview] = useState<{
    name: string;
    x: number;
    y: number;
    copy: boolean;
  } | null>(null);
  const [target, setTarget] = useState<string | null | undefined>(undefined);
  function destination(e: PointerEvent) {
    const element = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest<HTMLElement>("[data-drop-folder]");
    if (!element) return undefined;
    return element.dataset.dropFolder || null;
  }
  function reset() {
    gesture.current = null;
    setPreview(null);
    setTarget(undefined);
  }
  return {
    preview,
    target,
    start(e: PointerEvent, item: DragItem) {
      if (locked || e.button !== 0) return;
      gesture.current = { item, x: e.clientX, y: e.clientY, active: false };
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    move(e: PointerEvent) {
      const g = gesture.current;
      if (!g || locked) return;
      if (!g.active && Math.hypot(e.clientX - g.x, e.clientY - g.y) < 8) return;
      g.active = true;
      e.preventDefault();
      const t = destination(e);
      setTarget(t !== undefined && canDrop(g.item, t) ? t : undefined);
      setPreview({
        name: g.item.name,
        x: e.clientX + 14,
        y: e.clientY + 14,
        copy: e.ctrlKey,
      });
    },
    end(e: PointerEvent) {
      const g = gesture.current;
      if (!g) return;
      const t = g.active ? destination(e) : undefined;
      if (g.active) {
        suppressClick.current = true;
        setTimeout(() => {
          suppressClick.current = false;
        }, 0);
        e.preventDefault();
        if (!locked && t !== undefined && canDrop(g.item, t))
          onDrop(g.item, t, e.ctrlKey);
      }
      reset();
    },
    cancel: reset,
    click(e: React.MouseEvent) {
      if (suppressClick.current) {
        e.preventDefault();
        e.stopPropagation();
        suppressClick.current = false;
      }
    },
  };
}
