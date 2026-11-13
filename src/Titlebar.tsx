import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Своя тонкая шапка окна вместо системной рамки Windows: перетаскивание, свернуть, развернуть, закрыть. */
export default function Titlebar({ now }: { now?: string }) {
  const [max, setMax] = useState(false);
  useEffect(() => {
    const w = getCurrentWindow();
    const sync = () => w.isMaximized().then(setMax).catch(() => {});
    sync();
    let off: (() => void) | undefined;
    w.onResized(sync).then((f) => (off = f)).catch(() => {});
    return () => off?.();
  }, []);
  const w = () => getCurrentWindow();
  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="tb-title" data-tauri-drag-region>
        <svg className="logo" width="16" height="16" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round"><path d="M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4" /></svg>
        Silo{now ? <span className="tb-now">{now}</span> : null}
      </span>
      <div className="tb-btns">
        <button aria-label="Свернуть" onClick={() => w().minimize()}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6h8" stroke="currentColor" strokeWidth="1.2" /></svg>
        </button>
        <button aria-label={max ? "Восстановить" : "Развернуть"} onClick={() => w().toggleMaximize()}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.2">
            {max ? <><rect x="2.6" y="3.4" width="6" height="6" /><path d="M4 3.4V2h6v6H8.6" /></> : <rect x="2" y="2" width="8" height="8" />}
          </svg>
        </button>
        <button className="close" aria-label="Закрыть" onClick={() => w().close()}>
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" strokeWidth="1.2" /></svg>
        </button>
      </div>
    </div>
  );
}
