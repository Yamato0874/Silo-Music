import { useEffect, useState } from "react";
import { api, Playlist, PlaylistRef, Track } from "./api";
import { GENRES, LANGS, genreName, srcName, t } from "./i18n";
import { useApp } from "./ctx";
import { EQ_FREQS, PRESETS, THEMES, qualityFormat } from "./prefs";
import TrackList from "./TrackList";
import { eqFor, seedFor, tasteById } from "./tastes";
import { ArtistPicker, TastePicker } from "./Onboarding";
import { Btn, Confirm } from "./ui";
import Keys from "./Keys";

const toggle = <T,>(v: T[], x: T) => (v.includes(x) ? v.filter((y) => y !== x) : [...v, x]);

export function Search() {
  const { onError, prefs, play } = useApp();
  const [q, setQ] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [lang, setLang] = useState("");
  const [showGenres, setShowGenres] = useState(false);
  const [res, setRes] = useState<Track[]>([]);
  const [warn, setWarn] = useState<string[]>([]);
  const [searched, setSearched] = useState(false);
  const [seq] = useState({ n: 0 });
  // поиск при наборе: через 0,7 с бесплатные источники, через 1,5 с все
  useEffect(() => {
    const s = q.trim();
    if (s.length < 3) return;
    const h = setTimeout(() => go(undefined, true), 700);
    const full = setTimeout(() => go(), 1500); // затем везде, включая YouTube (ответы кэшируются на 3 дня)
    return () => { clearTimeout(h); clearTimeout(full); };
  }, [q]);
  const go = (o?: { tags: string[]; lang?: string }, auto = false, my = ++seq.n) =>
    !o &&/^https?:\/\/(www\.|m\.|on\.|music\.)?(soundcloud\.com|youtube\.com|youtu\.be)\//i.test(q.trim())
      ? api.linkTrack(q.trim()).then((tr) => { setRes([tr]); setWarn([]); setSearched(true); play([tr], 0); }).catch((e) => onError(String(e)))
      : api.searchAll({ query: o ? "" : q, tags: o ? o.tags : tags, lang: (o ? o.lang : lang) || undefined, offset: 0, limit: 20,
      sources: o ? ["jamendo"] : auto ? ["local", "jamendo"] : [], audioFormat: qualityFormat(prefs.quality), langStrict: prefs.langStrict })
      .then((r) => { if (my !== seq.n) return; setRes(r.tracks); setWarn(r.errors); setSearched(true); }).catch((e) => onError(String(e)));
  return (
    <>
      <form onSubmit={(e) => { e.preventDefault(); go(); }} className="row">
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.searchPlaceholder} />
        <Btn variant="ghost" active={showGenres || tags.length > 0 || !!lang} onClick={() => setShowGenres(!showGenres)}>
          {t.filters}{tags.length + (lang ? 1 : 0) ? ` · ${tags.length + (lang ? 1 : 0)}` : ""}
        </Btn>
        <Btn type="submit" variant="primary">{t.find}</Btn>
      </form>
      {showGenres && (
        <div className="chips">
          <select value={lang} onChange={(e) => setLang(e.target.value)} aria-label={t.language}>
            <option value="">{t.language}: {t.anyLang}</option>
            {LANGS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}
          </select>
          {GENRES.map((g) => <Btn key={g[0]} variant="chip" size="sm" active={tags.includes(g[0])} onClick={() => setTags(toggle(tags, g[0]))}>{genreName(g)}</Btn>)}
        </div>
      )}
      {!res.length && prefs.tastes.length > 0 && (
        <div className="foryou">
          <h4>{t.forYou}</h4>
          <div className="chips">
            {prefs.tastes.map((id) => { const g = tasteById(id); return g && (
              <Btn key={id} variant="chip" onClick={() => go({ tags: g.tags.slice(0, 1), lang: g.lang })}>{prefs.lang === "en" ? g.en : g.ru}</Btn>
            ); })}
          </div>
        </div>
      )}
      {warn.map((w, i) => <div key={i} className="err">{w}</div>)}
      {searched && <TrackList tracks={res} />}
    </>
  );
}

/** Готовые плейлисты Jamendo / YouTube / SoundCloud: найти, открыть, проиграть, сохранить к себе. */
export function Browse() {
  const { onError, play, prefs } = useApp();
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<PlaylistRef[]>([]);
  const [warn, setWarn] = useState<string[]>([]);
  const [open, setOpen] = useState<PlaylistRef | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [saved, setSaved] = useState(false);
  if (open) {
    return (
      <>
        <div className="row head">
          <Btn icon="back" label={t.back} onClick={() => setOpen(null)} />
          <div className="title"><b>{open.title}</b><span className="muted">{srcName(open.source)} · {open.owner}</span></div>
          <Btn icon="play" variant="primary" disabled={!tracks.length} onClick={() => play(tracks, 0)}>{t.playAll}</Btn>
          <Btn icon="save" variant="solid" active={saved} disabled={!tracks.length || saved}
            onClick={() => api.savePlaylist(open.title, tracks).then(() => setSaved(true)).catch((e) => onError(String(e)))}>{saved ? t.saved : t.saveToMine}</Btn>
        </div>
        <TrackList tracks={tracks} />
      </>
    );
  }
  return (
    <>
      <form className="row" onSubmit={(e) => { e.preventDefault(); api.searchPlaylists(q, []).then((r) => { setHits(r.playlists); setWarn(r.errors); }).catch((x) => onError(String(x))); }}>
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.playlistSearch} />
        <Btn type="submit" variant="primary">{t.find}</Btn>
      </form>
      {warn.map((w, i) => <div key={i} className="err">{w}</div>)}
      <ul className="tracks">
        {hits.map((p) => (
          <li key={p.id} className="clickable" tabIndex={0}
            onClick={() => { setOpen(p); setSaved(false); setTracks([]); api.openPlaylist(p.id, prefs.langStrict ? seedFor(prefs.tastes).lang : undefined).then(setTracks).catch((x) => onError(String(x))); }}>
            <span className="cover">{p.image ? <img src={p.image} alt="" /> : <span />}</span>
            <div className="meta"><b>{p.title}</b><span><em className={"src " + p.source}>{srcName(p.source)}</em>{p.owner}</span></div>
            <span className="dur">{p.count ? `${p.count} ${t.tracksN}` : ""}</span>
          </li>
        ))}
      </ul>
    </>
  );
}

/** Источники без ключа (Jamendo, YouTube, SoundCloud). null — ещё грузится. */
function useMissingKeys() {
  const [m, setM] = useState<string[] | null>(null);
  useEffect(() => {
    Promise.all(["jamendo_client_id", "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret"].map((k) => api.getSetting(k).catch(() => "")))
      .then(([j, y, i, s]) => setM([!j.trim() && "jamendo", !y.trim() && "youtube", !(i.trim() && s.trim()) && "soundcloud"].filter(Boolean) as string[]));
  }, []);
  return m;
}

export function Echo() {
  const { wave, startWave, stopWave, upcoming, cur } = useApp();
  const missing = useMissingKeys();
  return (
    <>
      <div className="hero">
        <button className={"hero-play" + (wave ? " on" : "")} onClick={() => (wave ? stopWave() : startWave())} aria-label={wave ? t.stopEcho : t.startEcho}>
          <svg width="34" height="34" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">{wave ? <path d="M7 4h3.5v16H7zM13.5 4H17v16h-3.5z" /> : <path d="M7 4.5v15l12-7.5z" />}</svg>
        </button>
        <div className="hero-text">
          <h2>{wave ? t.echoPlaying : t.echo}</h2>
          <p className="muted">{wave && cur ? `${cur.name} · ${cur.artist}` : t.echoPitch}</p>
          {missing && missing.length > 0 && <p className="muted">{t.echoNoKeys}: {missing.map((x) => srcName(x)).join(", ")}. {t.echoNoKeysHint}</p>}
        </div>
        {wave && <Btn variant="solid" onClick={stopWave}>{t.stopEcho}</Btn>}
      </div>
      {wave && upcoming.length > 0 && (
        <>
          <h4 className="shelf-t">{t.upNext}</h4>
          <ul className="mini">
            {upcoming.slice(0, 20).map((tr, i) => <li key={tr.id + i}><b>{tr.name}</b><span><em className={"src " + tr.source}>{srcName(tr.source)}</em>{tr.artist}</span></li>)}
          </ul>
        </>
      )}
    </>
  );
}

function Shelf({ title, tracks }: { title: string; tracks: Track[] }) {
  const { play } = useApp();
  if (!tracks.length) return null;
  return (
    <section>
      <h4 className="shelf-t">{title}</h4>
      <div className="shelf">
        {tracks.map((tr, i) => (
          <button key={tr.id + i} className="card" onClick={() => play(tracks, i)} title={`${tr.name} · ${tr.artist}`}>
            <span className="card-cover">{tr.image ? <img src={tr.image} alt="" loading="lazy" decoding="async" /> : <span className="ph" />}</span>
            <b>{tr.name}</b><small>{tr.artist}</small>
          </button>
        ))}
      </div>
    </section>
  );
}

/** Главная: запуск Эхо, недавнее, понравившееся, плейлисты, волны по жанрам из вкусов. */
export function Home() {
  const { prefs, wave, startWave, stopWave, go } = useApp();
  const missing = useMissingKeys();
  const [hist, setHist] = useState<Track[] | null>(null);
  const [liked, setLiked] = useState<Track[]>([]);
  const [pls, setPls] = useState<Playlist[]>([]);
  useEffect(() => {
    api.history().then((h) => setHist(h.slice(0, 14))).catch(() => setHist([]));
    api.liked().then((l) => setLiked(l.slice(0, 14))).catch(() => {});
    api.playlists().then(setPls).catch(() => {});
  }, []);
  const noKeys = missing !== null && missing.length === 3;
  const empty = hist !== null && !hist.length && !liked.length && !pls.length;
  return (
    <>
      <div className="hero">
        <button className={"hero-play" + (wave ? " on" : "")} onClick={() => (wave ? stopWave() : startWave())} aria-label={wave ? t.stopEcho : t.startEcho}>
          <svg width="34" height="34" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">{wave ? <path d="M7 4h3.5v16H7zM13.5 4H17v16h-3.5z" /> : <path d="M7 4.5v15l12-7.5z" />}</svg>
        </button>
        <div className="hero-text">
          <h2>{wave ? t.echoPlaying : t.startEcho}</h2>
          <p className="muted">{t.echoPitch}</p>
        </div>
      </div>
      {noKeys && (
        <div className="notice">
          <span>{t.addKeysHint}</span>
          <Btn variant="solid" size="sm" onClick={() => go("settings")}>{t.openSettings}</Btn>
        </div>
      )}
      {prefs.tastes.length > 0 && (
        <section>
          <h4 className="shelf-t">{t.byGenre}</h4>
          <div className="chips">
            {prefs.tastes.slice(0, 12).map((id) => { const g = tasteById(id); return g && (
              <Btn key={id} variant="chip" onClick={() => startWave([id])}>{prefs.lang === "en" ? g.en : g.ru}</Btn>
            ); })}
          </div>
        </section>
      )}
      <Shelf title={t.recent} tracks={hist ?? []} />
      <Shelf title={t.likedShelf} tracks={liked} />
      {pls.length > 0 && (
        <section>
          <h4 className="shelf-t">{t.myPls}</h4>
          <div className="chips">{pls.map((p) => <Btn key={p.id} variant="chip" onClick={() => go("playlists")}>{p.name} · {p.count}</Btn>)}</div>
        </section>
      )}
      {empty && (
        <div className="empty">
          <b>{t.homeEmptyT}</b>
          <p className="muted">{t.homeEmpty}</p>
          <div className="row" style={{ justifyContent: "center" }}>
            <Btn variant="primary" onClick={() => go("search")}>{t.search}</Btn>
            <Btn variant="solid" onClick={() => go("browse")}>{t.browse}</Btn>
          </div>
        </div>
      )}
    </>
  );
}

export function Liked() {
  const { onError, ratings, play } = useApp();
  const [d, setD] = useState<Track[]>([]);
  useEffect(() => { api.liked().then(setD).catch((e) => onError(String(e))); }, [ratings]);
  return (
    <>
      <div className="row head">
        <div className="title"><b>{t.liked}</b><span className="muted">{t.likedHint}</span></div>
        <Btn icon="play" variant="primary" disabled={!d.length} onClick={() => play(d, 0)}>{t.playAll}</Btn>
      </div>
      <TrackList tracks={d} />
    </>
  );
}

/** Трек, отправленный в бан, не попадает в «Эхо». Жанры и исполнители не банятся. */
export function Banned() {
  const { onError, ratings, rate } = useApp();
  const [d, setD] = useState<Track[]>([]);
  useEffect(() => { api.banned().then(setD).catch((e) => onError(String(e))); }, [ratings]);
  return (
    <>
      <div className="row head"><div className="title"><b>{t.banned}</b><span className="muted">{t.bannedHint}</span></div></div>
      {!d.length ? <p className="muted">{t.empty}</p> : (
        <ul className="tracks">
          {d.map((tr) => (
            <li key={tr.id}>
              <span className="cover">{tr.image ? <img src={tr.image} alt="" /> : <span />}</span>
              <div className="meta"><b>{tr.name}</b><span><em className={"src " + tr.source}>{srcName(tr.source)}</em>{tr.artist}</span></div>
              <Btn variant="solid" size="sm" onClick={() => rate(tr, 0)}>{t.restore}</Btn>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

export function History() {
  const { onError } = useApp();
  const [d, setD] = useState<Track[]>([]);
  const load = () => api.history().then(setD).catch((e) => onError(String(e)));
  useEffect(() => { load(); }, []);
  return (
    <>
      <div className="row head">
        <div className="title"><b>{t.history}</b></div>
        <Btn variant="solid" disabled={!d.length} onClick={() => api.clearHistory().then(load)}>{t.clearHistory}</Btn>
      </div>
      <TrackList tracks={d} />
    </>
  );
}

export function Playlists() {
  const [pls, setPls] = useState<Playlist[]>([]);
  const [sel, setSel] = useState<Playlist | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [name, setName] = useState("");
  const { play } = useApp();
  const refresh = () => api.playlists().then(setPls);
  useEffect(() => { refresh(); }, []);
  useEffect(() => { if (sel) api.playlistTracks(sel.id).then(setTracks); }, [sel]);
  return (
    <div className="split">
      <div>
        <form className="row" onSubmit={(e) => { e.preventDefault(); if (name.trim()) api.createPlaylist(name.trim()).then(() => { setName(""); refresh(); }); }}>
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder={t.newPlaylist} />
          <Btn type="submit" variant="solid">{t.create}</Btn>
        </form>
        <ul className="pls">
          {pls.map((pl) => (
            <li key={pl.id} className={sel?.id === pl.id ? "active" : ""}>
              <button className="pl-name" onClick={() => setSel(pl)}>{pl.name} <small>{pl.count}</small></button>
              <Btn icon="close" size="sm" label={t.deletePlaylist}
                onClick={() => api.deletePlaylist(pl.id).then(() => { if (sel?.id === pl.id) { setSel(null); setTracks([]); } refresh(); })} />
            </li>
          ))}
        </ul>
      </div>
      <div>
        {sel && (
          <div className="row head">
            <div className="title"><b>{sel.name}</b></div>
            <Btn icon="play" variant="primary" disabled={!tracks.length} onClick={() => play(tracks, 0)}>{t.playAll}</Btn>
          </div>
        )}
        {sel && <TrackList tracks={tracks} onRemove={(tr) => api.removeFromPlaylist(sel.id, tr.id).then(() => api.playlistTracks(sel.id).then(setTracks))} />}
      </div>
    </div>
  );
}

export function Downloads() {
  const { onError } = useApp();
  const [d, setD] = useState<Track[]>([]);
  useEffect(() => { api.downloads().then((x) => setD(x.map(([tr]) => tr))).catch((e) => onError(String(e))); }, []);
  return <TrackList tracks={d} />;
}

export function Settings() {
  const { prefs, setPrefs, resetTaste } = useApp();
  const [confirmReset, setConfirmReset] = useState(false);
  const [alsoGroups, setAlsoGroups] = useState(false);
  const [dir, setDir] = useState("");
  const [info, setInfo] = useState<[number, string]>([0, ""]);
  useEffect(() => {
    api.getSetting("download_dir").then(setDir);
    api.downloadsInfo().then(setInfo);
  }, []);
  const Check = ({ k, label }: { k: "autoplay" | "gapless" | "normalize" | "eqOn" | "showVideo" | "langStrict"; label: string }) => (
    <label className="chk"><input type="checkbox" checked={prefs[k]} onChange={(e) => setPrefs({ [k]: e.target.checked })} /> {label}</label>
  );
  return (
    <div className="settings">
      <h3>{t.theme}</h3>
      <div className="themes" role="radiogroup" aria-label={t.theme}>
        {THEMES.map(([id, ru, en, bg, acc]) => (
          <button key={id} role="radio" aria-checked={prefs.theme === id} className={"theme" + (prefs.theme === id ? " on" : "")} onClick={() => setPrefs({ theme: id })}>
            <span className="sw" style={{ background: bg }}><i style={{ background: acc }} /></span>
            {prefs.lang === "en" ? en : ru}
          </button>
        ))}
      </div>

      <h3>{t.tastesTitle}</h3>
      <TastePicker value={prefs.tastes} lang={prefs.lang} onChange={(ts) => setPrefs({ tastes: ts, eq: eqFor(ts), eqOn: ts.length > 0 })} />
      <p className="muted">{t.tastesNote}</p>

      <h3>{t.artistsTitle}</h3>
      <ArtistPicker value={prefs.artists} lang={prefs.lang} onChange={(artists) => setPrefs({ artists })} />

      <h3>{t.diversityTitle}</h3>
      <div className="chips">
        {(["familiar", "balanced", "discovery"] as const).map((m) => (
          <Btn key={m} variant="chip" active={prefs.diversity === m} onClick={() => setPrefs({ diversity: m })}>{t[m === "familiar" ? "dFamiliar" : m === "balanced" ? "dBalanced" : "dDiscovery"]}</Btn>
        ))}
      </div>
      <p className="muted">{t.diversityNote}</p>

      <div className="row" style={{ marginTop: 4 }}>
        <Btn variant="solid" onClick={() => setConfirmReset(true)}>{t.resetTaste}</Btn>
      </div>
      {confirmReset && (
        <Confirm title={t.resetTitle} body={t.resetBody} confirmLabel={t.confirm} cancelLabel={t.cancel}
          onCancel={() => { setConfirmReset(false); setAlsoGroups(false); }}
          onConfirm={() => { setConfirmReset(false); resetTaste(alsoGroups); setAlsoGroups(false); }}>
          <label className="chk"><input type="checkbox" checked={alsoGroups} onChange={(e) => setAlsoGroups(e.target.checked)} /> {t.resetGroups}</label>
        </Confirm>
      )}

      <h3>{t.playbackTitle}</h3>
      <Check k="autoplay" label={t.autoplay} />
      <Check k="gapless" label={t.gapless} />
      <label className="chk">{t.crossfade}: <input type="range" min={0} max={12} value={prefs.crossfade} onChange={(e) => setPrefs({ crossfade: Number(e.target.value) })} /> {prefs.crossfade}</label>
      <Check k="normalize" label={t.normalize} />
      <Check k="eqOn" label={t.eqOn} />
      <Check k="showVideo" label={t.showVideo} />
      <Check k="langStrict" label={t.langStrict} />
      {prefs.eqOn && (
        <>
          <div className="eq">
            {EQ_FREQS.map((f, i) => (
              <label key={f}>
                <input type="range" min={-12} max={12} step={1} value={prefs.eq[i] ?? 0} aria-label={`${f} Hz`}
                  onChange={(e) => { const eq = [...prefs.eq]; eq[i] = Number(e.target.value); setPrefs({ eq }); }} />
                <small>{f >= 1000 ? f / 1000 + "k" : f}</small>
              </label>
            ))}
          </div>
          <div className="chips">
            {Object.keys(PRESETS).map((k) => <Btn key={k} variant="chip" size="sm" active={PRESETS[k].join() === prefs.eq.join()} onClick={() => setPrefs({ eq: PRESETS[k] })}>{t[k as "flat"]}</Btn>)}
          </div>
        </>
      )}
      <p className="muted">{t.fxOnly}</p>
      <label className="chk">{t.quality}:
        <select value={prefs.quality} onChange={(e) => setPrefs({ quality: e.target.value as typeof prefs.quality })}>
          <option value="low">{t.qLow}</option><option value="normal">{t.qNormal}</option><option value="high">{t.qHigh}</option>
        </select>
      </label>
      <label className="chk">{t.lang}:
        <select value={prefs.lang} onChange={(e) => setPrefs({ lang: e.target.value as typeof prefs.lang })}>
          <option value="ru">Русский</option><option value="en">English</option>
        </select>
      </label>

      <h3>{t.keysTitle}</h3>
      <Keys lang={prefs.lang} />

      <h3>{t.storageTitle}</h3>
      <form className="row" onSubmit={(e) => { e.preventDefault(); api.setSetting("download_dir", dir).then(() => api.downloadsInfo().then(setInfo)); }}>
        <input value={dir} onChange={(e) => setDir(e.target.value)} placeholder={t.dlDir} />
        <Btn type="submit" variant="solid">{t.save}</Btn>
      </form>
      <p className="muted">{t.dlInfo}: {(info[0] / 1048576).toFixed(1)} MB · {info[1]}</p>
      <p className="muted">{t.cacheNote}</p>

      <h3>{t.hotkeysTitle}</h3>
      <p className="muted">{t.hotkeys}</p>
      <h3>{t.limitsTitle}</h3>
      <p className="muted">{t.limits}</p>
    </div>
  );
}
