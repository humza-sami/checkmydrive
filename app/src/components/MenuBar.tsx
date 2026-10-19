import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

export interface MenuItemDef {
  id: string;
  label?: string;
  separator?: boolean;
  disabled?: boolean;
  checked?: boolean;
  onSelect?: () => void;
  submenu?: MenuItemDef[];
}
export interface MenuDef {
  id: string;
  label: string;
  items: MenuItemDef[];
}

/**
 * Classic menu bar. Mouse: click to open, hover to switch while open. Keyboard: Alt or F10 focuses the
 * bar; Left/Right move between menus (mirrored in RTL), Up/Down move inside, Right opens a submenu,
 * Enter/Space activates, Escape closes one level.
 */
export function MenuBar({ menus, label }: { menus: MenuDef[]; label: string }) {
  const [open, setOpen] = useState<number | null>(null);
  const [sub, setSub] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const focusReq = useRef<"first" | "none">("none");
  const rtl = () => document.documentElement.dir === "rtl";

  const topButtons = () => Array.from(root.current?.querySelectorAll<HTMLButtonElement>(":scope > button.menu-top") ?? []);
  const close = useCallback((refocus: boolean, idx?: number | null) => {
    setOpen(null);
    setSub(null);
    if (refocus && idx != null) topButtons()[idx]?.focus();
  }, []);

  // Move focus into a freshly opened menu / submenu when it was opened from the keyboard.
  useLayoutEffect(() => {
    if (focusReq.current !== "first") return;
    focusReq.current = "none";
    const menus_ = root.current?.querySelectorAll<HTMLElement>("[role=menu]");
    const target = menus_ && menus_[menus_.length - 1];
    target?.querySelector<HTMLButtonElement>(":scope > li > button:not([disabled])")?.focus();
  }, [open, sub]);

  // Alt / F10 toggle focus on the bar; click elsewhere closes.
  useEffect(() => {
    let altAlone = false;
    const down = (e: KeyboardEvent) => {
      altAlone = e.key === "Alt" && !e.ctrlKey && !e.shiftKey && !e.metaKey;
      if (e.key === "F10" && !e.shiftKey) { e.preventDefault(); topButtons()[0]?.focus(); }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Alt" && altAlone) {
        e.preventDefault();
        const b = topButtons();
        if (root.current?.contains(document.activeElement)) { close(false); (document.activeElement as HTMLElement).blur(); }
        else b[0]?.focus();
      }
      altAlone = false;
    };
    const click = (e: MouseEvent) => { if (!root.current?.contains(e.target as Node)) close(false); };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("mousedown", click);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("mousedown", click);
    };
  }, [close]);

  const topIndexOf = (el: Element | null) => topButtons().indexOf(el as HTMLButtonElement);

  const onKeyDown = (e: React.KeyboardEvent) => {
    const target = e.target as HTMLElement;
    const menuEl = target.closest<HTMLElement>("[role=menu]");
    const fwd = rtl() ? "ArrowLeft" : "ArrowRight";
    const back = rtl() ? "ArrowRight" : "ArrowLeft";
    const tops = topButtons();

    if (!menuEl) {
      const i = topIndexOf(target);
      if (i < 0) return;
      if (e.key === fwd || e.key === back) {
        e.preventDefault();
        const n = tops[(i + (e.key === fwd ? 1 : -1) + tops.length) % tops.length];
        n.focus();
        if (open != null) { setSub(null); setOpen(topIndexOf(n)); focusReq.current = "first"; }
      } else if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        setSub(null); setOpen(i); focusReq.current = "first";
      } else if (e.key === "Escape") close(false);
      return;
    }

    const items = Array.from(menuEl.querySelectorAll<HTMLButtonElement>(":scope > li > button:not([disabled])"));
    const idx = items.indexOf(target as HTMLButtonElement);
    const isSub = menuEl.dataset.level === "1";
    const move = (to: number) => { e.preventDefault(); items[(to + items.length) % items.length]?.focus(); };
    switch (e.key) {
      case "ArrowDown": move(idx + 1); break;
      case "ArrowUp": move(idx - 1); break;
      case "Home": move(0); break;
      case "End": move(items.length - 1); break;
      case "Escape":
        e.preventDefault();
        if (isSub) { const parent = sub; setSub(null); root.current?.querySelector<HTMLButtonElement>(`[data-item="${parent}"]`)?.focus(); }
        else close(true, open);
        break;
      case "Tab": close(false); break;
      default:
        if (e.key === fwd) {
          e.preventDefault();
          if (target.getAttribute("aria-haspopup") === "menu") { setSub(target.dataset.item ?? null); focusReq.current = "first"; }
          else if (!isSub && open != null) { const n = (open + 1) % tops.length; setSub(null); setOpen(n); tops[n].focus(); focusReq.current = "first"; }
        } else if (e.key === back) {
          e.preventDefault();
          if (isSub) { const parent = sub; setSub(null); root.current?.querySelector<HTMLButtonElement>(`[data-item="${parent}"]`)?.focus(); }
          else if (open != null) { const n = (open - 1 + tops.length) % tops.length; setSub(null); setOpen(n); tops[n].focus(); focusReq.current = "first"; }
        }
    }
  };

  const renderItems = (items: MenuItemDef[], level: 0 | 1) => (
    <ul className="menu-pop" role="menu" data-level={level}>
      {items.map((it) =>
        it.separator ? (
          <li key={it.id} role="separator" className="menu-sep" />
        ) : (
          <li key={it.id} role="none" className="menu-li">
            <button
              type="button"
              role={it.submenu ? "menuitem" : it.checked !== undefined ? "menuitemradio" : "menuitem"}
              aria-checked={it.checked}
              aria-haspopup={it.submenu ? "menu" : undefined}
              aria-expanded={it.submenu ? sub === it.id : undefined}
              data-item={it.id}
              disabled={it.disabled}
              className="menu-item"
              onMouseEnter={(e) => {
                if (it.submenu) setSub(it.id);
                else if (level === 0) setSub(null);
                if (!it.disabled) (e.currentTarget as HTMLElement).focus({ preventScroll: true });
              }}
              onClick={() => {
                if (it.submenu) { setSub(it.id); focusReq.current = "first"; return; }
                const fn = it.onSelect;
                close(true, open);
                fn?.();
              }}
              lang={level === 1 ? it.id.replace(/^lang:/, "") : undefined}
            >
              <span className="menu-check" aria-hidden="true">{it.checked ? "•" : ""}</span>
              <span className="menu-label">{it.label}</span>
              {it.submenu && <span className="menu-arrow" aria-hidden="true" />}
            </button>
            {it.submenu && sub === it.id && renderItems(it.submenu, 1)}
          </li>
        ),
      )}
    </ul>
  );

  return (
    <div className="menubar" role="menubar" aria-label={label} ref={root} onKeyDown={onKeyDown}>
      {menus.map((m, i) => (
        <button
          key={m.id}
          type="button"
          role="menuitem"
          className={`menu-top ${open === i ? "open" : ""}`}
          aria-haspopup="menu"
          aria-expanded={open === i}
          onClick={() => { setSub(null); setOpen(open === i ? null : i); }}
          onMouseEnter={() => { if (open != null && open !== i) { setSub(null); setOpen(i); } }}
        >
          {m.label}
        </button>
      ))}
      {open != null && (
        <div className="menu-anchor" style={{ insetInlineStart: menuOffset(root.current, open) }}>
          {renderItems(menus[open].items, 0)}
        </div>
      )}
    </div>
  );
}

/** Left (or right, in RTL) offset of the open menu's button inside the bar. */
function menuOffset(root: HTMLElement | null, idx: number): number {
  const b = root?.querySelectorAll<HTMLElement>(":scope > button.menu-top")[idx];
  if (!b || !root) return 0;
  return document.documentElement.dir === "rtl"
    ? root.clientWidth - (b.offsetLeft + b.offsetWidth)
    : b.offsetLeft;
}
