import { ReactNode, useEffect, useRef, useState } from "react";
import { api, Track } from "./api";
import { srcName, t } from "./i18n";
import { useApp } from "./ctx";
import { usePs } from "./psStore";
import { Bars, Btn, Cover, Dialog, Empty, Icon, IconName, Menu, MenuItem, MenuPopup, toast } from "./ui";

export const fmtDur = (s: number) => (s ? `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}` : "");
/** Суммарная длительность списка: «12 треков · 47 мин». */
export const listSummary = (tracks: Track[]) => {
  const sec = tracks.reduce((a, x) => a + (x.duration || 0), 0);
  const h = Math.floor(sec / 3600), m = Math.round((sec % 3600) / 60);
  return `${tracks.length} ${t.tracksN}` + (sec ? " · " + (h ? `${h} ${t.hShort} ${m} ${t.mShort}` : `${m} ${t.mShort}`) : "");
};

export function RateButtons({ tr, size = "sm" }: { tr: Track; size?: "sm" | "md" }) {
  const { ratings, rate } = useApp();
  const v = ratings[tr.id] ?? 0;
  return (
    <>
      <Btn icon="like" size={size} label={t.like} active={v === 1} onClick={(e) => { e.stopPropagation(); rate(tr, v === 1 ? 0 : 1); }} />
      <Btn icon="dislike" size={size} label={t.dislike} active={v === -1} className="bad" onClick={(e) => { e.stopPropagation(); rate(tr, v === -1 ? 0 : -1); }} />
      <Btn icon="ban" size={size} label={t.ban} active={v === -2} className="bad" onClick={(e) => { e.stopPropagation(); rate(tr, v === -2 ? 0 : -2); }} />
    </>
  );
}

/** Полоски у играющего трека замирают на паузе; подписка на тик плеера только в одной строке. */
function LiveBars() { const ps = usePs(); return <Bars paused={!ps.playing} />; }

/** Карточка трека: лицензия, источник, жанры, ссылка на страницу. */
export function TrackInfo({ tr, onClose }: { tr: Track; onClose: () => void }) {
  const { openArtist, onError } = useApp();
  const err = (x: unknown) => onError(String(x));
  return (
    <Dialog title={t.about} onClose={onClose} footer={<>
      {tr.url && <Btn icon="external" variant="solid" onClick={() => api.openUrl(tr.url).catch(err)}>{t.openSource}</Btn>}
      <Btn variant="primary" onClick={onClose}>{t.close}</Btn></>}>
      <div className="info-top">
        <span className="cover"><Cover src={tr.image} /></span>
        <div className="grow"><b>{tr.name}</b><span className="muted">{tr.artist}</span></div>
      </div>
      <dl className="dl">
        {tr.artist && <><dt>{t.artist}</dt><dd><a className="artist-link" onClick={() => { onClose(); openArtist(tr.artist); }}>{tr.artist}</a></dd></>}
        {tr.album && <><dt>{t.album}</dt><dd>{tr.album}</dd></>}
        <dt>{t.source}</dt><dd><em className={"src " + tr.source}>{srcName(tr.source)}</em></dd>
        {tr.duration > 0 && <><dt>{t.duration}</dt><dd>{fmtDur(tr.duration)}</dd></>}
        {tr.licenseUrl && <><dt>{t.license}</dt><dd><a className="artist-link" onClick={() => api.openUrl(tr.licenseUrl).catch(err)}>{tr.licenseUrl.replace(/^https?:\/\//, "")}</a></dd></>}
        {tr.tags.length > 0 && <><dt>{t.tags}</dt><dd>{tr.tags.slice(0, 12).join(", ")}</dd></>}
      </dl>
    </Dialog>
  );
}

/** Список треков: обложка, название, источник, длительность, оценка и меню действий (кнопкой или правой кнопкой мыши). */
export default function TrackList(props: { tracks: Track[]; onRemove?: (t: Track) => void; onMove?: (from: number, to: number) => void; numbered?: boolean; empty?: ReactNode; noFilter?: boolean }) {
  const { play, enqueue, cur, onError, ratings, rate, radio, openArtist, playlists: pls, refreshPlaylists } = useApp();
  const [f, setF] = useState("");
  const [info, setInfo] = useState<Track | null>(null);
  const [limit, setLimit] = useState(100);
  const [ctx, setCtx] = useState<{ x: number; y: number; tr: Track } | null>(null);
  const drag = useRef(-1);
  const over = useRef<HTMLElement | null>(null);
  const more = useRef<HTMLLIElement>(null);
  const markOver = (el: HTMLElement | null) => { over.current?.classList.remove("drag-over"); over.current = el; el?.classList.add("drag-over"); };
  // догрузка порциями по 100 при прокрутке до конца списка
  useEffect(() => {
    const el = more.current; if (!el) return;
    const io = new IntersectionObserver((e) => { if (e[0].isIntersecting) setLimit((l) => l + 100); }, { rootMargin: "400px" });
    io.observe(el); return () => io.disconnect();
  });
  if (!props.tracks.length) return <>{props.empty ?? <Empty title={t.empty} />}</>;
  const err = (x: unknown) => onError(String(x));
  const low = f.trim().toLowerCase();
  const all = props.tracks.map((tr, i) => [tr, i] as const).filter(([tr]) => !low || (tr.name + " " + tr.artist + " " + tr.album).toLowerCase().includes(low));
  const shown = all.slice(0, limit);
  const menuFor = (tr: Track): MenuItem[] => {
    const v = ratings[tr.id] ?? 0;
    const items: MenuItem[] = [
      { label: t.playNext, icon: "playNext", onClick: () => { enqueue(tr, true); toast(t.addedNext, "ok"); } },
      { label: t.addQueue, icon: "addQueue", onClick: () => { enqueue(tr, false); toast(t.addedQueue, "ok"); } },
      { label: t.trackRadio, icon: "radio", onClick: () => radio(tr) },
      { sep: true, label: "" },
      { label: v === 1 ? t.unlike : t.like, icon: "like", onClick: () => rate(tr, v === 1 ? 0 : 1) },
    ];
    if (tr.artist) items.push({ label: t.artistPage, icon: "user", onClick: () => openArtist(tr.artist) });
    items.push({ label: t.about, icon: "info", onClick: () => setInfo(tr) });
    items.push({ label: t.copyName, icon: "copy", onClick: () => navigator.clipboard.writeText((tr.artist ? tr.artist + " - " : "") + tr.name).then(() => toast(t.copiedName, "ok")).catch(err) });
    if (["jamendo", "ccmixter", "archive"].includes(tr.source)) items.push({ label: tr.downloadAllowed ? t.download : t.noDownload, icon: "download" as IconName, disabled: !tr.downloadAllowed, onClick: () => { toast(t.downloading + ": " + tr.name, "info"); api.download(tr).then(() => toast(t.downloaded + ": " + tr.name, "ok")).catch(err); } });
    if (props.onRemove) items.push({ sep: true, label: "" }, { label: t.removeFromPlaylist, icon: "trash", danger: true, onClick: () => props.onRemove!(tr) });
    if (pls.length) {
      items.push({ sep: true, label: "" }, { label: t.addToPlaylist, header: true });
      pls.forEach((p) => items.push({ label: p.name, icon: "playlist", onClick: () => api.addToPlaylist(p.id, tr).then(() => { refreshPlaylists(); toast(t.addedTo + " «" + p.name + "»", "ok"); }).catch(err) }));
    }
    return items;
  };
  return (
    <>
    {props.tracks.length > 15 && !props.noFilter && (
      <div className="list-tools">
        <label className="field"><Icon name="search" size={16} /><input value={f} onChange={(e) => { setF(e.target.value); setLimit(100); }} placeholder={t.filter} aria-label={t.filter} onKeyDown={(e) => { if (e.key === "Escape") setF(""); }} />
          {f && <Btn icon="close" size="sm" className="field-x" label={t.clear} onClick={() => setF("")} />}</label>
        {low && <span className="list-count">{all.length} / {props.tracks.length}</span>}
      </div>
    )}
    {!all.length ? <Empty icon="search" title={t.noResults} text={`«${f}»`} /> : (
    <ul className="tracks">
      {shown.map(([tr, i], k) => {
        const rated = ratings[tr.id];
        const active = tr.id === cur?.id;
        return (
          <li key={tr.id + i} tabIndex={0} draggable={!!props.onMove && !low}
            onDragStart={(e) => { drag.current = i; e.dataTransfer.effectAllowed = "move"; e.currentTarget.classList.add("dragging"); }}
            onDragEnd={(e) => { e.currentTarget.classList.remove("dragging"); markOver(null); }}
            onDragOver={(e) => { if (props.onMove && drag.current >= 0) { e.preventDefault(); markOver(e.currentTarget); } }}
            onDrop={() => { if (props.onMove && drag.current >= 0 && drag.current !== i) props.onMove(drag.current, i); drag.current = -1; markOver(null); }}
            onContextMenu={(e) => { e.preventDefault(); setCtx({ x: e.clientX, y: e.clientY, tr }); }}
            onKeyDown={(e) => {
              if (e.target !== e.currentTarget) return;
              if (e.key === "Enter") play(props.tracks, i);
              else if (e.key === "ArrowDown") { e.preventDefault(); (e.currentTarget.nextElementSibling as HTMLElement | null)?.focus(); }
              else if (e.key === "ArrowUp") { e.preventDefault(); (e.currentTarget.previousElementSibling as HTMLElement | null)?.focus(); }
              else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) { e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); setCtx({ x: r.left + 60, y: r.bottom - 8, tr }); }
              else if (e.altKey && e.key === "ArrowUp" && props.onMove && i > 0) props.onMove(i, i - 1);
              else if (e.altKey && e.key === "ArrowDown" && props.onMove && i < props.tracks.length - 1) props.onMove(i, i + 1);
            }}
            className={(active ? "active" : "") + (rated ? " rated" : "") + (rated === -2 ? " banned" : "")}>
            {props.numbered && <span className="tr-n">{k + 1}</span>}
            <button className="cover" tabIndex={-1} onClick={() => play(props.tracks, i)} aria-label={t.play + ": " + tr.name}>
              <Cover src={tr.image} />
              <span className="cover-play">{active ? <LiveBars /> : null}<Icon name="play" size={18} /></span>
            </button>
            <div className="meta" onClick={() => play(props.tracks, i)}>
              <b title={tr.name}>{tr.name}</b>
              <span><em className={"src " + tr.source}>{srcName(tr.source)}</em>{tr.artist && <a className="artist-link" onClick={(e) => { e.stopPropagation(); openArtist(tr.artist); }}>{tr.artist}</a>}</span>
            </div>
            <div className="acts">
              <RateButtons tr={tr} />
              <Menu icon="more" size="sm" label={t.more} items={() => menuFor(tr)} />
            </div>
            <span className="dur">{fmtDur(tr.duration)}</span>
          </li>
        );
      })}
      {all.length > limit && <li ref={more} aria-hidden="true" style={{ justifyContent: "center" }}><Btn variant="solid" size="sm" onClick={() => setLimit(limit + 100)}>{t.showMore} ({all.length - limit})</Btn></li>}
    </ul>)}
    {ctx && <MenuPopup at={{ x: ctx.x, y: ctx.y }} items={menuFor(ctx.tr)} onClose={() => setCtx(null)} />}
    {info && <TrackInfo tr={info} onClose={() => setInfo(null)} />}
    </>
  );
}
