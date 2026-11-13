import { ButtonHTMLAttributes, ReactNode, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";

/** Набор линейных SVG-иконок 24×24 (stroke = currentColor). Один стиль на всё приложение. */
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
  dislike: "M17 14V3h4v11zM17 14l-4 8a3 3 0 0 1-3-3v-4H4.400a2 2 0 0 1-2-2.3l1.4-8a2 2 0 0 1 2-1.7H17",
  download: "M12 3v12M7 10l5 5 5-5M5 21h14",
  close: "M6 6l12 12M18 6 6 18",
  volume: "M4 9v6h4l5 4V5L8 9zM16.5 8.5a5 5 0 0 1 0 7",
  volumeLow: "M4 9v6h4l5 4V5L8 9z",
  mute: "M4 9v6h4l5 4V5L8 9zM17 9l5 6M22 9l-5 6",
  back: "M15 18l-6-6 6-6",
  chevR: "M9 6l6 6-6 6",
  chevD: "M6 9l6 6 6-6",
  playNext: "M5 4v7a4 4 0 0 0 4 4h11M15 10l5 5-5 5",
  addQueue: "M3 6h13M3 12h13M3 18h8M18 14v8M14 18h8",
  save: "M6 3h12v18l-6-4-6 4z",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14",
  home: "M3 11l9-8 9 8M5 10v10h5v-6h4v6h5V10",
  search: "M11 19a8 8 0 1 1 0-16a8 8 0 0 1 0 16zM21 21l-4.3-4.3",
  browse: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  echo: "M2 12h.01M6 8v8M10 4v16M14 7v10M18 9v6M22 12h.01",
  history: "M3 12a9 9 0 1 0 3-6.7M3 4v5h5M12 7v5l3 2",
  playlist: "M3 6h14M3 12h10M3 18h8M18 10v8.5a2.3 2.3 0 1 1-1.5-2.2M18 10l3-1",
  import: "M4 14v6h16v-6M12 15V3M8 7l4-4 4 4",
  settings: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h.01M16 4v4M10 10v4M18 16v4",
  heart: "M12 20.5S3.5 15.5 3.5 9.2A4.7 4.7 0 0 1 12 6.6a4.7 4.7 0 0 1 8.500 2.6c0 6.3-8.5 11.3-8.5 11.3z",
  lyrics: "M5 5h14v10H9l-4 4zM9 9h6M9 12h4",
  mini: "M3 5h18v14H3zM12 12h7v5h-7z",
  expand: "M4 14v6h6M20 10V4h-6M4 20l7-7M20 4l-7 7",
  speed: "M4.5 18a9 9 0 1 1 15 0M12 13l4-5",
  check: "M5 12.5l4.5 4.5L19 7.5",
  info: "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM12 11v6M12 7.5h.01",
  alert: "M12 3l10 18H2zM12 10v5M12 18h.01",
  plus: "M12 5v14M5 12h14",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
  copy: "M9 9h11v11H9zM5 15H4V4h11v1",
  refresh: "M20 11a8 8 0 0 0-14.3-4.3L4 8M4 4v4h4M4 13a8 8 0 0 0 14.3 4.3L20 16M20 20v-4h-4",
  folder: "M3 6h6l2 2h10v11H3z",
  grip: "M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01",
  radio: "M12 13a1 1 0 1 0 0-2a1 1 0 0 0 0 2zM7.8 16.2a6 6 0 0 1 0-8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 19.1a10 10 0 0 1 0-14.2M19.1 4.9a10 10 0 0 1 0 14.2",
  user: "M12 12a4 4 0 1 0 0-8a4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0",
  video: "M3 6h13v12H3zM16 10l5-3v10l-5-3",
  wifiOff: "M2 2l20 20M12 20h.01M8.5 16a5 5 0 0 1 7 0M5 12.500a10 10 0 0 1 3-2M2 8.500a15 15 0 0 1 4-2.7M10 5.1A15 15 0 0 1 22 8.500",
  music: "M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0a3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0a3 3 0 0 1 6 0z",
  eq: "M6 4v16M12 4v16M18 4v16M4 9h4M10 15h4M16 8h4",
  sun: "M12 17a5 5 0 1 0 0-10a5 5 0 0 0 0 10zM12 1v3M12 20v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M1 12h3M20 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1",
  help: "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM9.500 9.500a2.5 2.5 0 1 1 3.500 2.300c-.7.4-1 1-1 1.700M12 17h.01",
  database: "M4 6c0-1.700 3.600-3 8-3s8 1.300 8 3-3.600 3-8 3-8-1.300-8-3zM4 6v12c0 1.700 3.600 3 8 3s8-1.300 8-3V6M4 12c0 1.700 3.600 3 8 3s8-1.300 8-3",
  key: "M15 15a5 5 0 1 0-4.600-3L3 19.500V22h3v-2h2v-2h2l1.900-1.900A5 5 0 0 0 15 15zM16.500 8.500h.01",
  moon: "M20 14.500A8.500 8.500 0 0 1 9.500 4a8.500 8.500 0 1 0 10.500 10.500z",
  clock: "M12 21a9 9 0 1 0 0-18a9 9 0 0 0 0 18zM12 7v5l3 2",
  external: "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
  pin: "M12 17v5M8 3h8l-1 6 3 4H6l3-4z",
} as const;
export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, fill }: { name: IconName; size?: number; fill?: boolean }) {
  const filled = fill || name === "play" || name === "pause" || name === "next" || name === "prev";
  const dots = name === "more" || name === "grip";
  return (
    <svg className="ico" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"
      fill={filled ? "currentColor" : "none"} stroke="currentColor"
      strokeWidth={dots ? 2.8 : 1.7} strokeLinecap="round" strokeLinejoin="round">
      <path d={PATHS[name]} />
    </svg>
  );
}

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: IconName; label?: string; active?: boolean; busy?: boolean;
  /** ghost — прозрачная, solid — заливка панели, primary — акцент, chip — фильтр-«таблетка», nav — пункт меню, danger — опасное действие */
  variant?: "ghost" | "solid" | "primary" | "chip" | "nav" | "danger";
  size?: "sm" | "md" | "lg";
};

/** Единая кнопка приложения: hover, нажатие, видимый фокус с клавиатуры, состояние «занято». */
export function Btn({ icon, label, active, busy, variant = "ghost", size = "md", className = "", children, disabled, ...rest }: BtnProps) {
  const iconOnly = !!icon && !children;
  return (
    <button type="button" {...rest} disabled={disabled || busy} aria-label={label} title={label} aria-pressed={active} aria-busy={busy || undefined}
      className={`btn ${variant} ${size}${iconOnly ? " icon" : ""}${active ? " on" : ""}${busy ? " busy" : ""} ${className}`}>
      {busy ? <Spinner size={size === "lg" ? 20 : 16} /> : icon && <Icon name={icon} fill={active && (icon === "like" || icon === "dislike" || icon === "heart")} size={size === "lg" ? 22 : size === "sm" ? 16 : 18} />}
      {children && <span>{children}</span>}
    </button>
  );
}

export const Spinner = ({ size = 18 }: { size?: number }) => (
  <svg className="spin" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" role="status" aria-label="…"><path d="M12 3a9 9 0 1 0 9 9" /></svg>
);

export type MenuItem = { label: string; onClick?: () => void; disabled?: boolean; header?: boolean; checked?: boolean; icon?: IconName; danger?: boolean; sep?: boolean };

/** Всплывающее меню в портале: не обрезается списками и панелями, само выбирает сторону, управляется стрелками. */
export function MenuPopup({ at, items, onClose, anchor }: { at: { x: number; y: number; up?: boolean; right?: boolean }; items: MenuItem[]; onClose: () => void; anchor?: HTMLElement | null }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const el = ref.current!; const r = el.getBoundingClientRect(); const W = window.innerWidth, H = window.innerHeight;
    let left = at.right ? at.x - r.width : at.x, top = at.up ? at.y - r.height : at.y;
    if (left + r.width > W - 8) left = W - 8 - r.width; if (left < 8) left = 8;
    if (top + r.height > H - 8) top = Math.max(8, (at.up ? at.y : at.y - 36) - r.height); if (top < 40) top = 40;
    setPos({ left, top });
    (el.querySelector(".menu-i:not(:disabled)") as HTMLElement | null)?.focus({ preventScroll: true });
  }, []);
  useEffect(() => {
    const close = (e: Event) => { if (!ref.current?.contains(e.target as Node) && !anchor?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "Tab") { e.preventDefault(); onClose(); anchor?.focus(); return; }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
      e.preventDefault(); e.stopPropagation();
      const list = [...(ref.current?.querySelectorAll<HTMLElement>(".menu-i:not(:disabled)") ?? [])]; if (!list.length) return;
      const i = list.indexOf(document.activeElement as HTMLElement);
      const n = e.key === "Home" ? 0 : e.key === "End" ? list.length - 1 : (i + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length;
      list[n].focus();
    };
    const scroll = (e: Event) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    document.addEventListener("mousedown", close); document.addEventListener("keydown", key, true);
    window.addEventListener("resize", onClose); document.addEventListener("scroll", scroll, true); window.addEventListener("blur", onClose);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", key, true); window.removeEventListener("resize", onClose); document.removeEventListener("scroll", scroll, true); window.removeEventListener("blur", onClose); };
  }, [onClose, anchor]);
  return createPortal(
    <div ref={ref} className="menu" role="menu" style={pos ? { left: pos.left, top: pos.top } : { left: 0, top: 0, visibility: "hidden" }} onKeyDown={(e) => e.stopPropagation()} onContextMenu={(e) => e.preventDefault()}>
      {items.map((it, i) => it.sep ? <div key={i} className="menu-sep" role="separator" />
        : it.header ? <div key={i} className="menu-h">{it.label}</div>
        : <button key={i} role="menuitem" className={"menu-i" + (it.checked ? " on" : "") + (it.danger ? " danger" : "")} disabled={it.disabled}
            onClick={() => { onClose(); it.onClick?.(); }}>
            {it.icon ? <Icon name={it.icon} size={16} /> : it.checked !== undefined ? <span className="menu-ck">{it.checked && <Icon name="check" size={16} />}</span> : null}
            <span>{it.label}</span>
          </button>)}
    </div>, document.body);
}

/** Кнопка с выпадающим меню. up — открывать вверх (для панели плеера). */
export function Menu({ icon, label, items, up, active, size, children, variant }: { icon?: IconName; label: string; items: MenuItem[] | (() => MenuItem[]); up?: boolean; active?: boolean; size?: "sm" | "md"; children?: ReactNode; variant?: BtnProps["variant"] }) {
  const [at, setAt] = useState<{ x: number; y: number; up?: boolean; right?: boolean } | null>(null);
  const ref = useRef<HTMLSpanElement>(null);
  const open = () => { const r = ref.current!.getBoundingClientRect(); setAt({ x: r.right, y: up ? r.top - 6 : r.bottom + 6, up, right: true }); };
  return (
    <span className="menu-wrap" ref={ref}>
      <Btn icon={icon} label={label} size={size} variant={variant} active={active || !!at} aria-haspopup="menu" aria-expanded={!!at} onClick={(e) => { e.stopPropagation(); at ? setAt(null) : open(); }}>{children}</Btn>
      {at && <MenuPopup at={at} items={typeof items === "function" ? items() : items} onClose={() => setAt(null)} anchor={ref.current} />}
    </span>
  );
}

export const Row = ({ children }: { children: ReactNode }) => <div className="row">{children}</div>;

/** Шапка страницы: заголовок, пояснение и действия справа. Одна на каждый экран. */
export function PageHead({ title, sub, children, onBack, backLabel }: { title: ReactNode; sub?: ReactNode; children?: ReactNode; onBack?: () => void; backLabel?: string }) {
  return (
    <header className="page-h">
      {onBack && <Btn icon="back" label={backLabel} onClick={onBack} />}
      <div className="page-t"><h2>{title}</h2>{sub && <p>{sub}</p>}</div>
      {children && <div className="page-a">{children}</div>}
    </header>
  );
}

/** Заголовок секции внутри страницы. */
export function SectionHead({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return <div className="sec-h"><h3>{title}</h3>{children && <div className="sec-a">{children}</div>}</div>;
}

/** Пустое состояние: иконка, заголовок, пояснение и действие. */
export function Empty({ icon = "music", title, text, children }: { icon?: IconName; title: string; text?: string; children?: ReactNode }) {
  return (
    <div className="empty" role="status">
      <span className="empty-ico"><Icon name={icon} size={26} /></span>
      <b>{title}</b>
      {text && <p>{text}</p>}
      {children && <div className="empty-a">{children}</div>}
    </div>
  );
}

/** Заглушки строк, пока список грузится. */
export function SkeletonRows({ n = 6 }: { n?: number }) {
  return <ul className="tracks skel" aria-busy="true" aria-label="…">{Array.from({ length: n }, (_, i) => <li key={i}><span className="sk sk-cover" /><span className="sk-meta"><span className="sk" style={{ width: 30 + ((i * 17) % 40) + "%" }} /><span className="sk" style={{ width: 18 + ((i * 11) % 20) + "%" }} /></span></li>)}</ul>;
}
export function SkeletonCards({ n = 7 }: { n?: number }) {
  return <div className="shelf skel" aria-busy="true">{Array.from({ length: n }, (_, i) => <span key={i} className="card"><span className="sk card-cover" /><span className="sk" style={{ width: "80%" }} /><span className="sk" style={{ width: "50%" }} /></span>)}</div>;
}

/** Переключатель «вкл/выкл» с подписью и пояснением (строка настроек). */
export function Switch({ checked, onChange, label, hint, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string; disabled?: boolean }) {
  return (
    <label className={"set-row" + (disabled ? " off" : "")}>
      <span className="set-l"><b>{label}</b>{hint && <small>{hint}</small>}</span>
      <input type="checkbox" role="switch" className="switch" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}
/** Строка настроек с произвольным элементом управления справа. */
export function SetRow({ label, hint, children, stack }: { label: string; hint?: string; children: ReactNode; stack?: boolean }) {
  return <div className={"set-row" + (stack ? " stack" : "")}><span className="set-l"><b>{label}</b>{hint && <small>{hint}</small>}</span><span className="set-c">{children}</span></div>;
}

/** Ползунок с закрашенной частью слева (громкость, перемотка, кроссфейд). */
export function Slider({ value, min = 0, max = 100, step, onChange, className = "", ...rest }: { value: number; min?: number; max?: number; step?: number; onChange: (v: number) => void; className?: string } & Omit<React.InputHTMLAttributes<HTMLInputElement>, "onChange" | "value" | "min" | "max" | "step">) {
  const p = max > min ? ((value - min) / (max - min)) * 100 : 0;
  return <input type="range" className={"slider " + className} min={min} max={max} step={step} value={value} style={{ "--p": p + "%" } as React.CSSProperties} onChange={(e) => onChange(Number(e.target.value))} {...rest} />;
}

/* ---------- Диалоги: общий каркас с ловушкой фокуса ---------- */
export function Dialog({ title, children, onClose, footer, wide }: { title: string; children: ReactNode; onClose: () => void; footer?: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const el = ref.current!;
    if (!el.contains(document.activeElement)) (el.querySelector<HTMLElement>("[autofocus], input, textarea, select, .dialog-foot .btn.primary, .dialog-foot .btn") ?? el).focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.stopPropagation(); onClose(); return; }
      if (e.key !== "Tab") return;
      const f = [...el.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), select, textarea, a[href], [tabindex]:not([tabindex='-1'])")];
      if (!f.length) return;
      const first = f[0], last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    };
    document.addEventListener("keydown", key, true);
    return () => { document.removeEventListener("keydown", key, true); prev?.focus?.(); };
  }, []);
  return createPortal(
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div ref={ref} className={"dialog" + (wide ? " wide" : "")} role="dialog" aria-modal="true" aria-label={title} tabIndex={-1} onKeyDown={(e) => e.stopPropagation()}>
        <div className="dialog-h"><h3>{title}</h3><Btn icon="close" size="sm" label="Esc" onClick={onClose} /></div>
        <div className="dialog-b">{children}</div>
        {footer && <div className="dialog-foot">{footer}</div>}
      </div>
    </div>, document.body);
}

/** Окно подтверждения. children — дополнительные пункты (например, флажок). */
export function Confirm({ title, body, confirmLabel, cancelLabel, onConfirm, onCancel, children, danger }: {
  title: string; body: string; confirmLabel: string; cancelLabel: string; onConfirm: () => void; onCancel: () => void; children?: ReactNode; danger?: boolean;
}) {
  return (
    <Dialog title={title} onClose={onCancel} footer={<><Btn variant="solid" onClick={onCancel}>{cancelLabel}</Btn><Btn variant={danger ? "danger" : "primary"} onClick={onConfirm} autoFocus>{confirmLabel}</Btn></>}>
      <p className="muted">{body}</p>
      {children}
    </Dialog>
  );
}

/* ---------- Вопросы из кода: await ask.confirm(...) / ask.prompt(...) вместо системных окон ---------- */
type Ask = { kind: "confirm" | "prompt"; title: string; body?: string; value?: string; ok: string; cancel: string; danger?: boolean; placeholder?: string; done: (v: string | boolean | null) => void };
let asks: Ask[] = [];
const askSubs = new Set<() => void>();
const askEmit = () => askSubs.forEach((f) => f());
export const ask = {
  confirm: (o: { title: string; body?: string; ok: string; cancel: string; danger?: boolean }) => new Promise<boolean>((res) => { asks = [...asks, { kind: "confirm", ...o, done: (v) => res(!!v) }]; askEmit(); }),
  prompt: (o: { title: string; body?: string; value?: string; ok: string; cancel: string; placeholder?: string }) => new Promise<string | null>((res) => { asks = [...asks, { kind: "prompt", ...o, done: (v) => res(typeof v === "string" ? v : null) }]; askEmit(); }),
};
export function AskHost() {
  const list = useSyncExternalStore((cb) => { askSubs.add(cb); return () => { askSubs.delete(cb); }; }, () => asks);
  const a = list[0];
  const [v, setV] = useState("");
  useEffect(() => { setV(a?.value ?? ""); }, [a]);
  if (!a) return null;
  const close = (r: string | boolean | null) => { asks = asks.slice(1); askEmit(); a.done(r); };
  if (a.kind === "confirm") return <Confirm title={a.title} body={a.body ?? ""} confirmLabel={a.ok} cancelLabel={a.cancel} danger={a.danger} onCancel={() => close(false)} onConfirm={() => close(true)} />;
  return (
    <Dialog title={a.title} onClose={() => close(null)} footer={<><Btn variant="solid" onClick={() => close(null)}>{a.cancel}</Btn><Btn variant="primary" disabled={!v.trim()} onClick={() => close(v.trim())}>{a.ok}</Btn></>}>
      {a.body && <p className="muted">{a.body}</p>}
      <input className="dialog-input" value={v} placeholder={a.placeholder} autoFocus onFocus={(e) => e.target.select()} onChange={(e) => setV(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && v.trim()) close(v.trim()); }} />
    </Dialog>
  );
}

/* ---------- Тосты: короткие сообщения поверх интерфейса (успех, ошибка, справка) ---------- */
export type ToastKind = "ok" | "error" | "info";
type Toast = { id: number; kind: ToastKind; text: string; action?: { label: string; run: () => void } };
let toasts: Toast[] = [];
let toastId = 0;
const toastSubs = new Set<() => void>();
const toastEmit = () => toastSubs.forEach((f) => f());
const dropToast = (id: number) => { toasts = toasts.filter((x) => x.id !== id); toastEmit(); };
export function toast(text: string, kind: ToastKind = "info", action?: Toast["action"]) {
  if (!text) return;
  const same = toasts.find((x) => x.text === text);
  if (same) return; // одно и то же сообщение не дублируем
  const id = ++toastId;
  toasts = [...toasts.slice(-3), { id, kind, text, action }];
  toastEmit();
  window.setTimeout(() => dropToast(id), kind === "error" ? 8000 : action ? 7000 : 3500);
}
export function Toaster() {
  const list = useSyncExternalStore((cb) => { toastSubs.add(cb); return () => { toastSubs.delete(cb); }; }, () => toasts);
  return createPortal(
    <div className="toasts" role="region" aria-live="polite">
      {list.map((x) => (
        <div key={x.id} className={"toast " + x.kind} role={x.kind === "error" ? "alert" : "status"}>
          <Icon name={x.kind === "ok" ? "check" : x.kind === "error" ? "alert" : "info"} size={18} />
          <span>{x.text}</span>
          {x.action && <button className="toast-act" onClick={() => { x.action!.run(); dropToast(x.id); }}>{x.action.label}</button>}
          <button className="toast-x" aria-label="×" onClick={() => dropToast(x.id)}><Icon name="close" size={14} /></button>
        </div>
      ))}
    </div>, document.body);
}

/** Анимированные полоски «сейчас играет». */
export const Bars = ({ paused }: { paused?: boolean }) => <span className={"bars" + (paused ? " paused" : "")} aria-hidden="true"><i /><i /><i /></span>;

/** Обложка с запасным видом (нота), если картинки нет или она не загрузилась. */
export function Cover({ src, size }: { src?: string; size?: number }) {
  const [bad, setBad] = useState(false);
  useEffect(() => setBad(false), [src]);
  return src && !bad
    ? <img src={src} alt="" loading="lazy" decoding="async" draggable={false} onError={() => setBad(true)} style={size ? { width: size, height: size } : undefined} />
    : <span className="cover-ph" style={size ? { width: size, height: size } : undefined}><Icon name="music" size={size ? Math.round(size * 0.42) : 18} /></span>;
}
