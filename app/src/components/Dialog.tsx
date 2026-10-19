import { useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  role?: "dialog" | "alertdialog";
  width?: number;
  /** hide the caption close button (message boxes) */
  noClose?: boolean;
}

/** Modal XP dialog: Luna caption, beige client area, Tab stays inside, Escape closes. */
export function Dialog({ title, onClose, children, role = "dialog", width = 440, noClose }: Props) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>("[data-autofocus]") ?? ref.current?.querySelector<HTMLElement>("button, input, a[href]");
    first?.focus();
    return () => prev?.focus?.();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { e.stopPropagation(); onClose(); return; }
    if (e.key !== "Tab") return;
    const f = Array.from(ref.current?.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), a[href], select, [tabindex='0']") ?? []);
    if (!f.length) return;
    const a = document.activeElement;
    if (e.shiftKey && a === f[0]) { e.preventDefault(); f[f.length - 1].focus(); }
    else if (!e.shiftKey && a === f[f.length - 1]) { e.preventDefault(); f[0].focus(); }
  };

  return (
    <div className="dialog-layer" onKeyDown={onKeyDown}>
      <div className="dialog" role={role} aria-modal="true" aria-labelledby={titleId} ref={ref} style={{ width }}>
        <div className="titlebar dialog-title">
          <div className="titlebar-text" id={titleId}>{title}</div>
          {!noClose && (
            <div className="titlebar-controls">
              <button type="button" className="cap close" aria-label={t("window.close")} title={t("window.close")} onClick={onClose} tabIndex={-1}>
                <svg width="9" height="9" viewBox="0 0 9 9" aria-hidden="true"><path d="M1 1l7 7M8 1 1 8" stroke="#fff" strokeWidth="2" /></svg>
              </button>
            </div>
          )}
        </div>
        <div className="dialog-body">{children}</div>
      </div>
    </div>
  );
}
