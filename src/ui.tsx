import { ButtonHTMLAttributes, ReactNode, useEffect, useRef, useState } from "react";

/** Набор линейных SVG-иконок 24×24 (stroke = currentColor). */
const PATHS = {
  play: "M7 4.5v15l12-7.5z",
  pause: "M7 4h3.5v16H7zM13.5 4H17v16h-3.5z",
  prev: "M18 5.5v13L9 12zM6 5v14",
  next: "M6 5.5v13l9-6.5zM18 5v14",
  shuffle: "M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5",
  repeat: "M17 2l4 4-4 4M3 11V10a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v1a4 4 0 0 1-4 4H3",
  repeatOne: "M17 2l4 4-4 4M3 11V10a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v1a4 4 0 0 1-4 4H3M11 10h1.5v5",
  queue: "M3 6h18M3 12h18M3 18h11",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  like: "M7 10v11H3V10zM7 10l4-8a3 3 0 0 1 3 3v4h5.6a2 2 0 0 1 2 2.3l-1.4 8a2 2 0 0 1-2 1.7H7",
  ban: "M21 12a9 9 0 1 1-18 0a9 9 0 0 1 18 0zM5.6 5.6l12.8 12.8",
  dislike: "M17 14V3h4v11zM17 14l-4 8a3 3 0 0 1-3-3v-4H4.4a2 2 0 0 1-2-2.3l1.4-8a2 2 0 0 1 2-1.7H17",
  download: "M12 3v12M7 10l5 5 5-5M5 21h14",
  close: "M6 6l12 12M18 6 6 18",
  volume: "M4 9v6h4l5 4V5L8 9zM16.5 8.5a5 5 0 0 1 0 7",
  mute: "M4 9v6h4l5 4V5L8 9zM17 9l5 6M22 9l-5 6",
  back: "M15 18l-6-6 6-6",
  playNext: "M5 4v7a4 4 0 0 0 4 4h11M15 10l5 5-5 5",
  addQueue: "M3 6h13M3 12h13M3 18h8M18 14v8M14 18h8",
  save: "M6 3h12v18l-6-4-6 4z",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14",
} as const;
export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, fill }: { name: IconName; size?: number; fill?: boolean }) {
  const filled = fill || name === "play" || name === "pause" || name === "next" || name === "prev";
  return (
    <svg className="ico" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"
      fill={filled ? "currentColor" : "none"} stroke="currentColor"
      strokeWidth={name === "more" ? 3.2 : 1.8} strokeLinecap="round" strokeLinejoin="round">
      <path d={PATHS[name]} />
    </svg>
  );
}

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: IconName; label?: string; active?: boolean;
  /** ghost — прозрачная, solid — заливка панели, primary — акцент, chip — фильтр-«таблетка», nav — пункт меню */
  variant?: "ghost" | "solid" | "primary" | "chip" | "nav";
  size?: "sm" | "md" | "lg";
};

/** Единая кнопка приложения: hover, нажатие (лёгкое «вдавливание»), видимый фокус с клавиатуры. */
export function Btn({ icon, label, active, variant = "ghost", size = "md", className = "", children, ...rest }: BtnProps) {
  const iconOnly = !!icon && !children;
  return (
    <button type="button" {...rest} aria-label={label} title={label} aria-pressed={active}
      className={`btn ${variant} ${size}${iconOnly ? " icon" : ""}${active ? " on" : ""} ${className}`}>
      {icon && <Icon name={icon} fill={active && (icon === "like" || icon === "dislike")} size={size === "lg" ? 22 : size === "sm" ? 16 : 18} />}
      {children && <span>{children}</span>}
    </button>
  );
}

export type MenuItem = { label: string; onClick?: () => void; disabled?: boolean; header?: boolean; checked?: boolean };

/** Выпадающее меню за кнопкой. up — открывать вверх (для панели плеера). */
export function Menu({ icon, label, items, up, active, size }: { icon: IconName; label: string; items: MenuItem[]; up?: boolean; active?: boolean; size?: "sm" | "md" }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", close); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", esc); };
  }, [open]);
  return (
    <div className="menu-wrap" ref={ref}>
      <Btn icon={icon} label={label} size={size} active={active || open} onClick={() => setOpen(!open)} />
      {open && (
        <div className={"menu" + (up ? " up" : "")} role="menu">
          {items.map((it, i) => it.header
            ? <div key={i} className="menu-h">{it.label}</div>
            : <button key={i} role="menuitem" className={"menu-i" + (it.checked ? " on" : "")} disabled={it.disabled}
                onClick={() => { it.onClick?.(); setOpen(false); }}>{it.label}</button>)}
        </div>
      )}
    </div>
  );
}

export const Row = ({ children }: { children: ReactNode }) => <div className="row">{children}</div>;

/** Окно подтверждения. children — дополнительные пункты (например, флажок). */
export function Confirm({ title, body, confirmLabel, cancelLabel, onConfirm, onCancel, children }: {
  title: string; body: string; confirmLabel: string; cancelLabel: string; onConfirm: () => void; onCancel: () => void; children?: ReactNode;
}) {
  useEffect(() => {
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") onCancel(); };
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, [onCancel]);
  return (
    <div className="scrim" onMouseDown={onCancel}>
      <div className="dialog" role="alertdialog" aria-label={title} onMouseDown={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        <p className="muted">{body}</p>
        {children}
        <div className="dialog-foot">
          <Btn variant="solid" onClick={onCancel}>{cancelLabel}</Btn>
          <Btn variant="primary" onClick={onConfirm} autoFocus>{confirmLabel}</Btn>
        </div>
      </div>
    </div>
  );
}
