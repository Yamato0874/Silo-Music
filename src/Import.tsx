import { useRef, useState } from "react";
import { api, Track } from "./api";
import { srcName, t } from "./i18n";
import { useApp } from "./ctx";
import { Btn } from "./ui";

interface Wanted { artist: string; title: string; raw: string }
interface Row { w: Wanted; hit?: Track }

const SEP = /\s+[-–—]\s+/;
const clean = (s: string) => s.replace(/\s+/g, " ").trim();
const fromLine = (line: string): Wanted | null => {
  const s = clean(line);
  if (!s) return null;
  const p = s.split(SEP);
  return p.length >= 2 ? { artist: p[0], title: p.slice(1).join(" - "), raw: s } : { artist: "", title: s, raw: s };
};

/** Разбор CSV с кавычками (запятая или точка с запятой). */
function csv(text: string): string[][] {
  const first = text.split(/\r?\n/, 1)[0] ?? "";
  const d = (first.match(/;/g)?.length ?? 0) > (first.match(/,/g)?.length ?? 0) ? ";" : ",";
  const rows: string[][] = []; let row: string[] = [], cell = "", q = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (q) { if (c === '"') { if (text[i + 1] === '"') { cell += '"'; i++; } else q = false; } else cell += c; }
    else if (c === '"') q = true;
    else if (c === d) { row.push(cell); cell = ""; }
    else if (c === "\n" || c === "\r") { if (c === "\r" && text[i + 1] === "\n") i++; row.push(cell); rows.push(row); row = []; cell = ""; }
    else cell += c;
  }
  if (cell || row.length) { row.push(cell); rows.push(row); }
  return rows.filter((r) => r.some((x) => x.trim()));
}

function walkJson(x: unknown, out: Wanted[]) {
  if (Array.isArray(x)) { x.forEach((y) => walkJson(y, out)); return; }
  if (typeof x === "string") { const w = fromLine(x); if (w) out.push(w); return; }
  if (!x || typeof x !== "object") return;
  const o = x as Record<string, any>;
  const title = o.title ?? o.trackName ?? o.track_name ?? o.name ?? (typeof o.track === "string" ? o.track : undefined);
  let artist = o.artist ?? o.artistName ?? o.artist_name ?? o.artists ?? o.author;
  if (Array.isArray(artist)) artist = artist.map((a) => (typeof a === "string" ? a : a?.name)).filter(Boolean).join(", ");
  if (artist && typeof artist === "object") artist = artist.name;
  if (typeof title === "string" && (typeof artist === "string" || o.duration !== undefined || o.album !== undefined)) { out.push({ artist: clean(String(artist ?? "")), title: clean(title), raw: clean(`${artist ?? ""} - ${title}`) }); return; }
  Object.values(o).forEach((v) => walkJson(v, out)); // Spotify: items[].track.{name,artists[]}, Яндекс/VK и т.п.
}

/** Разбор файла/текста в список «исполнитель — название» и список ссылок на плейлисты. */
export function parseInput(name: string, text: string): { wanted: Wanted[]; urls: string[] } {
  const ext = name.toLowerCase().split(".").pop() ?? "";
  const wanted: Wanted[] = []; const urls: string[] = [];
  const body = text.replace(/^﻿/, "");
  const addUrl = (s: string) => { if (/^https?:\/\//i.test(s.trim())) { urls.push(s.trim()); return true; } return false; };
  if (ext === "json" || /^\s*[\[{]/.test(body)) {
    try { walkJson(JSON.parse(body), wanted); return { wanted, urls }; } catch { /* не JSON: разбираем как текст */ }
  }
  if (ext === "csv" || (ext === "txt" && body.split(/\r?\n/, 1)[0].includes(",") && /(artist|исполнитель|track|title)/i.test(body.split(/\r?\n/, 1)[0]))) {
    const rows = csv(body); if (!rows.length) return { wanted, urls };
    const head = rows[0].map((h) => h.trim().toLowerCase());
    const find = (names: string[]) => head.findIndex((h) => names.some((n) => h === n || h.startsWith(n)));
    let ti = find(["track name", "track", "title", "song", "name", "название", "трек", "песня"]);
    let ai = find(["artist name", "artist", "artists", "исполнитель", "автор"]);
    const hasHead = ti >= 0 || ai >= 0;
    if (!hasHead) { ai = 0; ti = rows[0].length > 1 ? 1 : 0; }
    for (const r of hasHead ? rows.slice(1) : rows) {
      const title = clean(r[ti] ?? ""), artist = ai >= 0 && ai !== ti ? clean((r[ai] ?? "").split(/[;,]/)[0]) : "";
      if (title) wanted.push({ artist, title, raw: clean(`${artist} - ${title}`) });
    }
    return { wanted, urls };
  }
  const lines = body.split(/\r?\n/);
  if (ext === "pls") {
    lines.forEach((l) => { const m = l.match(/^Title\d+=(.+)$/i); if (m) { const w = fromLine(m[1]); if (w) wanted.push(w); } });
    return { wanted, urls };
  }
  let pending: string | null = null; // M3U: #EXTINF:секунды,Исполнитель - Название
  for (const l of lines) {
    const s = l.trim();
    if (!s) continue;
    if (s.startsWith("#EXTINF")) { pending = s.slice(s.indexOf(",") + 1); continue; }
    if (s.startsWith("#")) continue;
    if (addUrl(s) && !pending) continue;
    const w = fromLine(pending ?? decodeURIComponent((s.split(/[\\/]/).pop() ?? s).replace(/\.[a-z0-9]{2,4}$/i, "").replace(/^\d+[\s._-]+/, "")));
    pending = null;
    if (w) wanted.push(w);
  }
  return { wanted, urls };
}

const norm = (s: string) => s.toLowerCase().replace(/\(.*?\)|\[.*?\]/g, " ").replace(/\b(feat|ft|featuring|official|video|audio|lyrics|remastered)\b.*$/i, " ").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
const toks = (s: string) => new Set(norm(s).split(" ").filter(Boolean));
const overlap = (a: Set<string>, b: Set<string>) => (a.size ? [...a].filter((x) => b.has(x)).length / a.size : 0);
/** Насколько найденный трек похож на желаемый (0..1). На YouTube «исполнитель» — канал, поэтому смотрим и в название. */
export function score(w: Wanted, tr: Track): number {
  const tt = toks(w.title), at = toks(w.artist);
  const trTitle = toks(tr.name), trAll = toks(`${tr.name} ${tr.artist}`);
  const title = overlap(tt, trTitle);
  const artist = at.size ? Math.max(overlap(at, toks(tr.artist)), overlap(at, trAll)) : 0.6;
  return 0.65 * title + 0.35 * artist;
}

export default function Import() {
  const { play, onError } = useApp();
  const [text, setText] = useState("");
  const [name, setName] = useState("");
  const [rows, setRows] = useState<Row[]>([]);
  const [fromUrls, setFromUrls] = useState<Track[]>([]);
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState("");
  const [saved, setSaved] = useState(false);
  const stop = useRef(false);
  const file = useRef<HTMLInputElement>(null);

  const run = async (fileName: string, content: string) => {
    const { wanted, urls } = parseInput(fileName, content);
    if (!wanted.length && !urls.length) { onError(t.importNothing); return; }
    stop.current = false; setBusy(true); setSaved(false); setRows([]); setFromUrls([]);
    const urlTracks: Track[] = [];
    for (const u of urls) {
      try { urlTracks.push(...(await api.openPlaylist(await api.resolvePlaylist(u)))); }
      catch (e) { onError(`${u}: ${String(e)}`); }
    }
    setFromUrls(urlTracks);
    const out: Row[] = []; let yt = 0;
    // проход 0: то, что уже слушали и лайкали (бесплатно, без сети)
    const known: Track[] = [];
    try { known.push(...(await api.history()), ...(await api.liked())); } catch { /* без истории тоже работаем */ }
    const artistCache = new Map<string, Track[]>();
    for (let i = 0; i < wanted.length && !stop.current; i++) {
      const w = wanted[i];
      setProg(`${i + 1} / ${wanted.length}`);
      const q = clean(`${w.artist} ${w.title}`);
      let best: Track | undefined, bs = 0;
      const pick = (list: Track[]) => list.forEach((tr) => { const s = score(w, tr); if (s > bs) { bs = s; best = tr; } });
      try {
        // сначала свободные источники и скачанное (бесплатно), YouTube только если не нашлось (стоит квоту: не больше 15 раз за импорт)
        pick(known);
        if (bs < 0.6) pick((await api.searchAll({ query: q, tags: [], offset: 0, limit: 6, sources: ["local", "jamendo", "soundcloud"], langStrict: false })).tracks);
        // проход 2: топ исполнителя на YouTube — один поиск на исполнителя (кэш 30 дней, общий с волной)
        if (bs < 0.6 && w.artist.trim()) {
          const ak = w.artist.trim().toLowerCase();
          if (!artistCache.has(ak) && yt < 15) { const [v, searched] = await api.artistTracks(w.artist); if (searched) yt++; artistCache.set(ak, v); }
          pick(artistCache.get(ak) ?? []);
        }
        // проход 3: точный поиск трека, только для ненайденного
        if (bs < 0.6 && yt < 15) { yt++; pick((await api.searchAll({ query: q, tags: [], offset: 0, limit: 5, sources: ["youtube"], langStrict: false })).tracks); }
      } catch { /* ошибка одного поиска не обрывает импорт */ }
      out.push({ w, hit: bs >= 0.6 ? best : undefined });
      if (i % 3 === 2) setRows([...out]);
    }
    setRows(out); setBusy(false); setProg("");
  };

  const found = [...fromUrls, ...rows.filter((r) => r.hit).map((r) => r.hit!)];
  const missed = rows.filter((r) => !r.hit);
  return (
    <>
      <div className="row head"><div className="title"><b>{t.importT}</b><span className="muted">{t.importHint}</span></div></div>
      <textarea className="imp-text" value={text} onChange={(e) => setText(e.target.value)} rows={7} placeholder={t.importPlaceholder} disabled={busy} />
      <div className="row">
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder={t.importName} disabled={busy} />
        <input ref={file} type="file" accept=".m3u,.m3u8,.pls,.csv,.json,.txt" hidden
          onChange={async (e) => { const f = e.target.files?.[0]; e.target.value = ""; if (!f) return; if (!name) setName(f.name.replace(/\.[^.]+$/, "")); run(f.name, await f.text()); }} />
        <Btn variant="solid" disabled={busy} onClick={() => file.current?.click()}>{t.importFile}</Btn>
        <Btn variant="primary" disabled={busy || !text.trim()} onClick={() => run("paste.txt", text)}>{t.importFind}</Btn>
        {busy && <Btn onClick={() => { stop.current = true; }}>{t.importStop}</Btn>}
      </div>
      <p className="muted">{t.importFormats}</p>
      {(busy || rows.length > 0 || fromUrls.length > 0) && (
        <div className="imp-sum">
          <b>{busy ? `${t.importSearching} ${prog}` : `${t.importFound}: ${found.length} · ${t.importMissed}: ${missed.length}`}</b>
          {!busy && found.length > 0 && (
            <span className="row" style={{ margin: 0 }}>
              <Btn variant="primary" icon="play" onClick={() => play(found, 0)}>{t.playAll}</Btn>
              <Btn variant="solid" icon="save" disabled={saved} onClick={() => api.savePlaylist(name.trim() || t.importDefaultName, found).then(() => setSaved(true)).catch((e) => onError(String(e)))}>{saved ? t.saved : t.importSave}</Btn>
            </span>
          )}
        </div>
      )}
      <ul className="imp-list">
        {rows.map((r, i) => (
          <li key={i} className={r.hit ? "ok" : "miss"}>
            <span className="mark">{r.hit ? "✓" : "✗"}</span>
            <span className="want">{r.w.raw}</span>
            <span className="got">{r.hit ? <><em className={"src " + r.hit.source}>{srcName(r.hit.source)}</em>{r.hit.artist} — {r.hit.name}</> : t.importNoMatch}</span>
          </li>
        ))}
      </ul>
    </>
  );
}
