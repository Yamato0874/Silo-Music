import { useEffect, useState } from "react";
import { api, Playlist, Track } from "./api";
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
export default function TrackList(props: { tracks: Track[]; onRemove?: (t: Track) => void }) {
  const { play, enqueue, cur, onError, ratings } = useApp();
  const [pls, setPls] = useState<Playlist[]>([]);
  useEffect(() => { api.playlists().then(setPls).catch(() => {}); }, [props.tracks]);
  if (!props.tracks.length) return <p className="muted">{t.empty}</p>;
  const err = (x: unknown) => onError(String(x));
  return (
    <ul className="tracks">
      {props.tracks.map((tr, i) => {
        const items: MenuItem[] = [
          { label: t.playNext, onClick: () => enqueue(tr, true) },
          { label: t.addQueue, onClick: () => enqueue(tr, false) },
        ];
        if (tr.source === "jamendo") items.push({ label: tr.downloadAllowed ? t.download : t.noDownload, disabled: !tr.downloadAllowed, onClick: () => api.download(tr).catch(err) });
        if (props.onRemove) items.push({ label: t.removeFromPlaylist, onClick: () => props.onRemove!(tr) });
        if (pls.length) {
          items.push({ label: t.addToPlaylist, header: true });
          pls.forEach((p) => items.push({ label: p.name, onClick: () => api.addToPlaylist(p.id, tr).catch(err) }));
        }
        const rated = ratings[tr.id];
        return (
          <li key={tr.id + i} className={(tr.id === cur?.id ? "active" : "") + (rated ? " rated" : "") + (rated === -2 ? " banned" : "")}>
            <button className="cover" onClick={() => play(props.tracks, i)} aria-label={t.play}>
              {tr.image ? <img src={tr.image} alt="" loading="lazy" decoding="async" /> : <span />}
            </button>
            <div className="meta" onClick={() => play(props.tracks, i)}>
              <b>{tr.name}</b>
              <span><em className={"src " + tr.source}>{srcName(tr.source)}</em>{tr.artist}</span>
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
  );
}
