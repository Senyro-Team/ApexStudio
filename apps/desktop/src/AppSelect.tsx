import { useEffect, useId, useRef, useState, type SelectHTMLAttributes } from "react";
import { createPortal } from "react-dom";

type MenuPosition = { top: number; left: number; width: number; maxHeight: number };

export default function AppSelect({
  children,
  onPointerDown,
  onMouseDown,
  onClick,
  onKeyDown,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement>) {
  const selectRef = useRef<HTMLSelectElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [position, setPosition] = useState<MenuPosition | null>(null);

  function placeMenu() {
    const select = selectRef.current;
    if (!select) return;
    const rect = select.getBoundingClientRect();
    const width = Math.min(Math.max(rect.width, 180), window.innerWidth - 16);
    const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    const wantedHeight = Math.min(288, select.options.length * 40 + 8);
    const below = window.innerHeight - rect.bottom - 8;
    const above = rect.top - 8;
    const showAbove = below < wantedHeight && above > below;
    const maxHeight = Math.max(40, Math.min(wantedHeight, showAbove ? above : below));
    setPosition({
      top: showAbove ? rect.top - maxHeight - 6 : rect.bottom + 6,
      left,
      width,
      maxHeight,
    });
  }

  function showMenu(offset = 0) {
    const select = selectRef.current;
    if (!select || select.disabled || !select.options.length) return;
    setActive(Math.max(0, Math.min(select.options.length - 1, select.selectedIndex + offset)));
    placeMenu();
    setOpen(true);
  }

  function choose(index: number) {
    const select = selectRef.current;
    const option = select?.options[index];
    if (!select || select.disabled || !option || option.disabled) return;
    if (select.value !== option.value) {
      select.value = option.value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }
    setOpen(false);
    select.focus();
  }

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!selectRef.current?.contains(target) && !menuRef.current?.contains(target))
        setOpen(false);
    };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", placeMenu);
    window.addEventListener("scroll", placeMenu, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("resize", placeMenu);
      window.removeEventListener("scroll", placeMenu, true);
    };
  }, [open]);

  useEffect(() => {
    if (open) (menuRef.current?.children[active] as HTMLElement | undefined)?.scrollIntoView?.({ block: "nearest" });
  }, [active, open]);

  const options = open ? Array.from(selectRef.current?.options ?? []) : [];
  return (
    <>
      <select
        {...props}
        ref={selectRef}
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-activedescendant={open ? `${menuId}-option-${active}` : undefined}
        onPointerDown={(event) => {
          onPointerDown?.(event);
          if (event.defaultPrevented || event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          if (open) setOpen(false);
          else showMenu();
        }}
        onMouseDown={(event) => {
          onMouseDown?.(event);
          event.preventDefault();
        }}
        onClick={(event) => {
          onClick?.(event);
          event.preventDefault();
        }}
        onKeyDown={(event) => {
          onKeyDown?.(event);
          if (event.defaultPrevented) return;
          const count = event.currentTarget.options.length;
          if (event.key === "Escape" && open) {
            event.preventDefault();
            setOpen(false);
          } else if (event.key === "Tab") {
            setOpen(false);
          } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            if (!open) showMenu(event.key === "ArrowDown" ? 1 : -1);
            else setActive((index) => Math.max(0, Math.min(count - 1, index + (event.key === "ArrowDown" ? 1 : -1))));
          } else if (event.key === "Home" && open) {
            event.preventDefault();
            setActive(0);
          } else if (event.key === "End" && open) {
            event.preventDefault();
            setActive(Math.max(0, count - 1));
          } else if (event.key === "Enter" || event.key === " " || event.key === "F4") {
            event.preventDefault();
            if (open) choose(active);
            else showMenu();
          } else if (open && event.key.length === 1) {
            const match = options.findIndex((option) =>
              option.text.toLocaleLowerCase().startsWith(event.key.toLocaleLowerCase()),
            );
            if (match >= 0) setActive(match);
          }
        }}
      >
        {children}
      </select>
      {open && position && createPortal(
        <div
          id={menuId}
          ref={menuRef}
          className="app-select-menu"
          role="listbox"
          style={position}
        >
          {options.map((option, index) => (
            <button
              key={`${option.value}-${index}`}
              id={`${menuId}-option-${index}`}
              type="button"
              role="option"
              aria-selected={option.value === selectRef.current?.value}
              className={index === active ? "active" : undefined}
              disabled={option.disabled}
              onPointerDown={(event) => event.preventDefault()}
              onMouseEnter={() => setActive(index)}
              onClick={() => choose(index)}
            >
              {option.text}
            </button>
          ))}
        </div>,
        document.body,
      )}
    </>
  );
}
