import { useState } from "react";
import { api, Track } from "./api";
import { srcName, t } from "./i18n";
import { useApp } from "./ctx";
import { Btn, Menu, MenuItem } from "./ui";

const fmt = (s: number) => (s ? `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}` : "");

export function RateButtons({ tr, size = "sm" }: { tr: Track; size?: "sm" | "md" }) {
  const { ratings, rate } = useApp();
  const v = ratings[tr.id] ?? 0;
  return (
    <>
      <Btn icon="like" size={size} label={t.like} active={v === 1} onClick={() => rate(tr, v === 1 ? 0 : 1)} />
      <Btn icon="dislike" size={size} label={t.dislike} active={v === -1} className="bad" onClick={() => rate(tr, v === -1 ? 0 : -1)} />
      <Btn icon="ban" size={size} label={t.ban} active={v === -2} className="bad" onClick={() => rate(tr, v === -2 ? 0 : -2)} />
    </>
  );
}

/** Строка трека: обложка, название, источник, длительность, оценка и одно меню действий. */
export default function TrackList(props: { tracks: Track[]; onRemove?: (t: Track) => void; onMove?: (from: number, to: number) => void }) {
  const { play, enqueue, cur, onError, ratings, radio, openArtist, playlists: pls } = useApp();
  const [f, setF] = useState("");
  const [info, setInfo] = useState<Track | null>(null);
  const [drag, setDrag] = useState(-1);
  const [limit, setLimit] = useState(100);
  if (!props.tracks.length) return <p className="muted">{t.empty}</p>;
  const err = (x: unknown) => onError(String(x));
  // быстрый фильтр и порции по 100: длинные списки не рисуются целиком
  const low = f.trim().toLowerCase();
  const all = props.tracks.map((tr, i) => [tr, i] as const).filter(([tr]) => !low || (tr.name + " " + tr.artist).toLowerCase().includes(low));
  const shown = all.slice(0, limit);
  return (
    <>
    {props.tracks.length > 15 && <input className="list-filter" value={f} onChange={(e) => { setF(e.target.value); setLimit(100); }} placeholder={t.filter} />}
    <ul className="tracks">
      {shown.map(([tr, i]) => {
        const items: MenuItem[] = [
          { label: t.playNext, onClick: () => enqueue(tr, true) },
          { label: t.addQueue, onClick: () => enqueue(tr, false) },
          { label: t.trackRadio, onClick: () => radio(tr) },
          { label: t.about, onClick: () => setInfo(tr) },
        ];
        if (tr.artist) items.push({ label: t.artistPage, onClick: () => openArtist(tr.artist) });
        if (["jamendo", "ccmixter", "archive"].includes(tr.source)) items.push({ label: tr.downloadAllowed ? t.download : t.noDownload, disabled: !tr.downloadAllowed, onClick: () => api.download(tr).catch(err) });
        if (props.onRemove) items.push({ label: t.removeFromPlaylist, onClick: () => props.onRemove!(tr) });
        if (pls.length) {
          items.push({ label: t.addToPlaylist, header: true });
          pls.forEach((p) => items.push({ label: p.name, onClick: () => api.addToPlaylist(p.id, tr).catch(err) }));
        }
        const rated = ratings[tr.id];
        return (
          <li key={tr.id + i} draggable={!!props.onMove && !low} onDragStart={() => setDrag(i)} onDragOver={(e) => props.onMove && e.preventDefault()} onDrop={() => { if (props.onMove && drag >= 0 && drag !== i) props.onMove(drag, i); setDrag(-1); }} className={(tr.id === cur?.id ? "active" : "") + (rated ? " rated" : "") + (rated === -2 ? " banned" : "")}>
            <button className="cover" onClick={() => play(props.tracks, i)} aria-label={t.play}>
              {tr.image ? <img src={tr.image} alt="" loading="lazy" decoding="async" /> : <span />}
            </button>
            <div className="meta" onClick={() => play(props.tracks, i)}>
              <b>{tr.name}</b>
              <span><em className={"src " + tr.source}>{srcName(tr.source)}</em><a className="artist-link" onClick={(e) => { e.stopPropagation(); if (tr.artist) openArtist(tr.artist); }}>{tr.artist}</a></span>
            </div>
            <span className="dur">{fmt(tr.duration)}</span>
            <div className="acts">
              <RateButtons tr={tr} />
              <Menu icon="more" size="sm" label={t.more} items={items} />
            </div>
          </li>
        );
      })}
    </ul>
    {info && (
      <div className="scrim" onClick={() => setInfo(null)}>
        <div className="dialog" onClick={(e) => e.stopPropagation()}>
          <h3>{info.name}</h3>
          <p className="muted">{info.artist}{info.album ? " · " + info.album : ""}</p>
          <p>{t.source}: {srcName(info.source)}{info.duration ? " · " + fmt(info.duration) : ""}</p>
          {info.licenseUrl && <p>{t.license}: <a className="artist-link" onClick={() => api.openUrl(info.licenseUrl).catch(err)}>{info.licenseUrl.replace(/^https?:\/\//, "")}</a></p>}
          {info.tags.length > 0 && <p>{t.tags}: {info.tags.slice(0, 12).join(", ")}</p>}
          <div className="row" style={{ gap: 8 }}>
            {info.url && <Btn size="sm" onClick={() => api.openUrl(info.url).catch(err)}>{t.openSource}</Btn>}
            <Btn size="sm" variant="solid" onClick={() => setInfo(null)}>{t.close}</Btn>
          </div>
        </div>
      </div>
    )}
    {all.length > limit && <button className="more-btn" onClick={() => setLimit(limit + 100)}>{t.showMore} ({all.length - limit})</button>}
    </>
  );
}
