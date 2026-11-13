import { KeyboardEvent, ReactNode, useEffect, useRef, useState } from "react";
import { api, Playlist, PlaylistRef, Track } from "./api";
import { GENRES, LANGS, genreName, srcName, t } from "./i18n";
import { useApp } from "./ctx";
import { EQ_FREQS, PRESETS, THEMES, qualityFormat } from "./prefs";
import TrackList, { listSummary } from "./TrackList";
import { eqFor, seedFor, tasteById } from "./tastes";
import { ArtistPicker, TastePicker } from "./Onboarding";
import { Btn, Confirm, Cover, Empty, Icon, IconName, Menu, PageHead, SectionHead, SetRow, SkeletonCards, SkeletonRows, Slider, Spinner, Switch, ask, toast } from "./ui";
import Keys from "./Keys";

const toggle = <T,>(v: T[], x: T) => (v.includes(x) ? v.filter((y) => y !== x) : [...v, x]);
const shuffle = <T,>(a: T[]) => { const r = [...a]; for (let i = r.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [r[i], r[j]] = [r[j], r[i]]; } return r; };
/** Плейлист, который нужно открыть при переходе на вкладку «Мои плейлисты» (с Главной). */
let pendingPlaylist: number | null = null;

export function Search() {
  const { onError, prefs, play } = useApp();
  const [q, setQ] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [lang, setLang] = useState("");
  const [showGenres, setShowGenres] = useState(false);
  const [res, setRes] = useState<Track[]>([]);
  const [warn, setWarn] = useState<string[]>([]);
  const [searched, setSearched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [seq] = useState({ n: 0 });
  // последние 10 запросов
  const [recent, setRecent] = useState<string[]>([]);
  useEffect(() => { api.getSetting("search_history").then((x) => x && setRecent(JSON.parse(x))).catch(() => {}); }, []);
  const saveRecent = (n: string[]) => { setRecent(n); api.setSetting("search_history", JSON.stringify(n)).catch(() => {}); };
  const remember = (x: string) => {
    const v = x.trim(); if (!v || /^https?:/i.test(v)) return;
    saveRecent([v, ...recent.filter((r) => r.toLowerCase() !== v.toLowerCase())].slice(0, 10));
  };
  // подсказки при вводе (Deezer, без квоты) и счётчик поисков YouTube за сутки
  const [sug, setSug] = useState<Track[]>([]);
  const [sel, setSel] = useState(-1);
  const [usage, setUsage] = useState<[number, number] | null>(null);
  useEffect(() => {
    const tick = () => api.ytUsage().then((u) => u && setUsage(u)).catch(() => {});
    tick(); const id = window.setInterval(tick, 15000); return () => clearInterval(id);
  }, []);
  useEffect(() => {
    const s = q.trim(); setSel(-1);
    if (s.length < 2 || /^https?:/i.test(s) || !navigator.onLine) { setSug([]); return; }
    let live = true;
    const h = setTimeout(() => api.suggestTracks(s).then((v) => live && setSug(v ?? [])).catch(() => live && setSug([])), 250);
    return () => { live = false; clearTimeout(h); };
  }, [q]);
  const pick = (k: number) => { const tr = sug[k]; if (!tr) return; setSug([]); remember(`${tr.artist} ${tr.name}`); play(sug, k); };
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") { if (sug.length) { ev.preventDefault(); setSug([]); } else if (q) { ev.preventDefault(); reset(); } return; }
    if (!sug.length) return;
    if (ev.key === "ArrowDown") { ev.preventDefault(); setSel((sel + 1) % sug.length); }
    else if (ev.key === "ArrowUp") { ev.preventDefault(); setSel(sel <= 0 ? sug.length - 1 : sel - 1); }
    else if (ev.key === "Enter" && sel >= 0) { ev.preventDefault(); pick(sel); }
  };
  // поиск при наборе: через 0,7 с библиотека, Jamendo и Deezer; видео YouTube ищется только при запуске трека
  useEffect(() => {
    const s = q.trim();
    if (s.length < 3) return;
    const h = setTimeout(() => go(undefined, true), 700);
    return () => clearTimeout(h);
  }, [q]);
  const go = (o?: { tags: string[]; lang?: string }, auto = false, my = ++seq.n) => {
    setBusy(true);
    const done = () => { if (my === seq.n) setBusy(false); };
    return !o && /^https?:\/\/(www\.|m\.|on\.|music\.)?(soundcloud\.com|youtube\.com|youtu\.be)\//i.test(q.trim())
      ? api.linkTrack(q.trim()).then((tr) => { setRes([tr]); setWarn([]); setSearched(true); play([tr], 0); }).catch((e) => onError(String(e))).finally(done)
      : api.searchAll({ query: o ? "" : q, tags: o ? o.tags : tags, lang: (o ? o.lang : lang) || undefined, offset: 0, limit: 20,
      sources: !navigator.onLine ? ["local"] : o ? ["jamendo"] : auto ? ["local", "jamendo", "deezer"] : [], audioFormat: qualityFormat(prefs.quality), langStrict: prefs.langStrict })
      .then((r) => { if (my !== seq.n) return; setRes(r.tracks); setWarn(r.errors); setSearched(true); }).catch((e) => onError(String(e))).finally(done);
  };
  const nFilters = tags.length + (lang ? 1 : 0);
  const reset = () => { seq.n++; setSug([]); setQ(""); setRes([]); setWarn([]); setSearched(false); setBusy(false); document.getElementById("search-input")?.focus(); };
  return (
    <>
      <PageHead title={t.search}>{usage && <span className="quota" title={t.ytTodayHint}><Icon name="video" size={14} />{t.ytToday}: <b>{usage[0]}</b>/{usage[1]}<i style={{ "--w": Math.min(100, (usage[0] / Math.max(1, usage[1])) * 100) + "%" } as React.CSSProperties} /></span>}</PageHead>
      <form onSubmit={(e) => { e.preventDefault(); setSug([]); remember(q); go(); }} className="row" role="search">
        <div className="field lg suggest">
          {busy ? <Spinner size={18} /> : <Icon name="search" size={18} />}
          <input id="search-input" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.searchPlaceholder} aria-label={t.search} autoFocus autoComplete="off" spellCheck={false}
            onKeyDown={onKey} onBlur={() => setTimeout(() => setSug([]), 150)} role="combobox" aria-expanded={sug.length > 0} aria-controls="search-sug" aria-activedescendant={sel >= 0 ? "sug-" + sel : undefined} />
          {q && <Btn icon="close" size="sm" className="field-x" label={t.clear} onClick={reset} />}
          {sug.length > 0 && (
            <ul role="listbox" id="search-sug">
              {sug.map((x, k) => (
                <li key={x.id} id={"sug-" + k} role="option" aria-selected={k === sel} className={k === sel ? "on" : ""} onMouseDown={(ev) => { ev.preventDefault(); pick(k); }}>
                  <span className="cover"><Cover src={x.image} /></span>
                  <span className="meta"><b>{x.name}</b><span>{x.artist}{x.album ? " · " + x.album : ""}</span></span>
                  <Icon name="play" size={14} />
                </li>
              ))}
            </ul>
          )}
        </div>
        <Btn variant="solid" size="lg" icon="settings" active={showGenres || nFilters > 0} aria-expanded={showGenres} onClick={() => setShowGenres(!showGenres)}>
          {t.filters}{nFilters ? ` · ${nFilters}` : ""}
        </Btn>
        <Btn type="submit" variant="primary" size="lg" disabled={!q.trim() && !nFilters}>{t.find}</Btn>
      </form>
      {showGenres && (
        <div className="card-box" style={{ marginBottom: 16 }}>
          <div className="row">
            <select value={lang} onChange={(e) => setLang(e.target.value)} aria-label={t.language}>
              <option value="">{t.language}: {t.anyLang}</option>
              {LANGS.map(([k, n]) => <option key={k} value={k}>{n}</option>)}
            </select>
            {nFilters > 0 && <Btn size="sm" icon="close" onClick={() => { setTags([]); setLang(""); }}>{t.resetFilters}</Btn>}
          </div>
          <div className="chips" style={{ margin: 0 }}>
            {GENRES.map((g) => <Btn key={g[0]} variant="chip" size="sm" active={tags.includes(g[0])} onClick={() => setTags(toggle(tags, g[0]))}>{genreName(g)}</Btn>)}
          </div>
        </div>
      )}
      {warn.map((w, i) => <div key={i} className="err" role="alert"><Icon name="alert" size={18} /><span>{w}</span></div>)}
      {!searched && !busy && (
        <>
          {recent.length > 0 && (
            <section>
              <SectionHead title={t.recentQ}><Btn size="sm" onClick={() => saveRecent([])}>{t.clear}</Btn></SectionHead>
              <div className="chips">{recent.map((r) => <Btn key={r} variant="chip" size="sm" icon="clock" onClick={() => setQ(r)}>{r}</Btn>)}</div>
            </section>
          )}
          {prefs.tastes.length > 0 && (
            <section>
              <SectionHead title={t.forYou} />
              <div className="chips">
                {prefs.tastes.map((id) => { const g = tasteById(id); return g && (
                  <Btn key={id} variant="chip" onClick={() => go({ tags: g.tags.slice(0, 1), lang: g.lang })}>{prefs.lang === "en" ? g.en : g.ru}</Btn>
                ); })}
              </div>
            </section>
          )}
          {!recent.length && !prefs.tastes.length && <Empty icon="search" title={t.searchEmptyT} text={t.searchEmpty} />}
        </>
      )}
      {busy && !searched && <SkeletonRows n={8} />}
      {searched && <TrackList tracks={res} noFilter empty={<Empty icon="search" title={t.noResults} text={t.noResultsHint} />} />}
      {searched && res.length > 0 && !busy && <p className="set-note" style={{ marginTop: 16 }}>{t.searchYtHint}</p>}
    </>
  );
}

/** Готовые плейлисты Jamendo / YouTube / SoundCloud: найти, открыть, проиграть, сохранить к себе. */
export function Browse() {
  const { onError, play, prefs, refreshPlaylists } = useApp();
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<PlaylistRef[] | null>(null);
  const [warn, setWarn] = useState<string[]>([]);
  const [open, setOpen] = useState<PlaylistRef | null>(null);
  const [tracks, setTracks] = useState<Track[] | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const find = (query: string) => { if (!query.trim()) return; setBusy(true); api.searchPlaylists(query, []).then((r) => { setHits(r.playlists); setWarn(r.errors); }).catch((x) => onError(String(x))).finally(() => setBusy(false)); };
  if (open) {
    return (
      <>
        <PageHead title={open.title} sub={<>{srcName(open.source)}{open.owner ? " · " + open.owner : ""}{tracks?.length ? " · " + listSummary(tracks) : ""}</>} onBack={() => setOpen(null)} backLabel={t.back}>
          <Btn icon="save" variant="solid" active={saved} disabled={!tracks?.length || saved}
            onClick={() => api.savePlaylist(open.title, tracks!).then(() => { setSaved(true); refreshPlaylists(); toast(t.savedToMine, "ok"); }).catch((e) => onError(String(e)))}>{saved ? t.saved : t.saveToMine}</Btn>
          <Btn icon="shuffle" variant="solid" label={t.shufflePlay} disabled={!tracks?.length} onClick={() => play(shuffle(tracks!), 0)} />
          <Btn icon="play" variant="primary" disabled={!tracks?.length} onClick={() => play(tracks!, 0)}>{t.playAll}</Btn>
        </PageHead>
        {tracks === null ? <SkeletonRows /> : <TrackList tracks={tracks} numbered empty={<Empty title={t.plEmptyT} text={t.plOpenEmpty} />} />}
      </>
    );
  }
  return (
    <>
      <PageHead title={t.browse} sub={t.browseHint} />
      <form className="row" role="search" onSubmit={(e) => { e.preventDefault(); find(q); }}>
        <label className="field lg">{busy ? <Spinner size={18} /> : <Icon name="search" size={18} />}<input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.playlistSearch} aria-label={t.playlistSearch} autoFocus /></label>
        <Btn type="submit" variant="primary" size="lg" disabled={!q.trim()} busy={busy}>{t.find}</Btn>
      </form>
      {warn.map((w, i) => <div key={i} className="err" role="alert"><Icon name="alert" size={18} /><span>{w}</span></div>)}
      {hits === null && !busy && (
        <>
          {prefs.tastes.length > 0 && <div className="chips">{prefs.tastes.slice(0, 10).map((id) => { const g = tasteById(id); const n = g && (prefs.lang === "en" ? g.en : g.ru); return n && <Btn key={id} variant="chip" onClick={() => { setQ(n); find(n); }}>{n}</Btn>; })}</div>}
          <Empty icon="browse" title={t.browseEmptyT} text={t.browseEmpty} />
        </>
      )}
      {busy && hits === null && <SkeletonRows />}
      {hits !== null && !hits.length && <Empty icon="search" title={t.noResults} text={t.noResultsHint} />}
      {hits !== null && hits.length > 0 && (
        <ul className="tracks">
          {hits.map((p) => {
            const go = () => { setOpen(p); setSaved(false); setTracks(null); api.openPlaylist(p.id, prefs.langStrict ? seedFor(prefs.tastes).lang : undefined).then(setTracks).catch((x) => { setTracks([]); onError(String(x)); }); };
            return (
              <li key={p.id} className="clickable" tabIndex={0} role="button" onClick={go} onKeyDown={(e) => { if (e.key === "Enter") go(); }}>
                <span className="cover"><Cover src={p.image} /></span>
                <div className="meta"><b>{p.title}</b><span><em className={"src " + p.source}>{srcName(p.source)}</em><span className="sub-t">{p.owner}</span></span></div>
                <span className="dur" style={{ minWidth: 80 }}>{p.count ? `${p.count} ${t.tracksN}` : ""}</span>
                <Icon name="chevR" size={16} />
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}

/** Источники без ключа (Jamendo, YouTube, SoundCloud). null — ещё грузится. */
function useMissingKeys() {
  const [m, setM] = useState<string[] | null>(null);
  useEffect(() => {
    Promise.all(["jamendo_client_id", "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret"].map((k) => api.getSetting(k).catch(() => "")))
      .then(([j, y, i, s]) => setM([!(j ?? "").trim() && "jamendo", !(y ?? "").trim() && "youtube", !((i ?? "").trim() && (s ?? "").trim()) && "soundcloud"].filter(Boolean) as string[]));
  }, []);
  return m;
}

/** Большая кнопка запуска «Эхо» с состоянием загрузки: подборка собирается несколько секунд. */
function EchoHero({ title, kicker, children }: { title?: string; kicker?: string; children?: ReactNode }) {
  const { wave, startWave, stopWave, cur } = useApp();
  const [busy, setBusy] = useState(false);
  const run = () => { if (wave) { stopWave(); return; } setBusy(true); Promise.resolve(startWave()).finally(() => setBusy(false)); };
  return (
    <div className={"hero" + (wave ? " on" : "")}>
      <button className="hero-play" onClick={run} disabled={busy} aria-label={wave ? t.stopEcho : t.startEcho} aria-busy={busy}>
        {busy ? <Spinner size={30} /> : <svg width="30" height="30" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">{wave ? <path d="M7 4h3.5v16H7zM13.5 4H17v16h-3.5z" /> : <path d="M8 4.5v15l12-7.5z" />}</svg>}
      </button>
      <div className="hero-text">
        <span className="hero-kicker">{kicker ?? t.echo}</span>
        <h2>{busy ? t.echoBuilding : wave ? t.echoPlaying : title ?? t.startEcho}</h2>
        <p className="muted">{wave && cur ? `${cur.name} · ${cur.artist}` : t.echoPitch}</p>
        {children}
      </div>
    </div>
  );
}

export function Echo() {
  const { wave, upcoming, prefs, setPrefs, startWave, play, cur } = useApp();
  const missing = useMissingKeys();
  return (
    <>
      <PageHead title={t.echo} sub={t.echoHelp} />
      <EchoHero>
        {missing && missing.length > 0 && <p className="muted">{t.echoNoKeys}: {missing.map((x) => srcName(x)).join(", ")}. {t.echoNoKeysHint}</p>}
      </EchoHero>
      <SectionHead title={t.diversityTitle} />
      <div className="chips">
        {(["familiar", "balanced", "discovery"] as const).map((m) => (
          <Btn key={m} variant="chip" active={prefs.diversity === m} onClick={() => setPrefs({ diversity: m })}>{t[m === "familiar" ? "dFamiliar" : m === "balanced" ? "dBalanced" : "dDiscovery"]}</Btn>
        ))}
      </div>
      <p className="set-note">{t.diversityNote}</p>
      {prefs.tastes.length > 0 && (
        <>
          <SectionHead title={t.byGenre} />
          <div className="chips">
            {prefs.tastes.slice(0, 16).map((id) => { const g = tasteById(id); return g && <Btn key={id} variant="chip" icon="echo" onClick={() => startWave([id])}>{prefs.lang === "en" ? g.en : g.ru}</Btn>; })}
          </div>
        </>
      )}
      {wave && upcoming.length > 0 && (
        <>
          <SectionHead title={t.upNext} />
          <TrackList tracks={upcoming.slice(0, 20)} noFilter />
        </>
      )}
      {!wave && !cur && <Empty icon="echo" title={t.echoIdleT} text={t.echoIdle}><Btn variant="solid" onClick={() => api.liked().then((l) => l.length ? play(shuffle(l), 0) : toast(t.noLikedYet, "info")).catch(() => {})}>{t.playLiked}</Btn></Empty>}
    </>
  );
}

/** Горизонтальная полка карточек со стрелками прокрутки. */
function Shelf({ title, tracks, more }: { title: string; tracks: Track[] | null; more?: () => void }) {
  const { play, cur } = useApp();
  const ref = useRef<HTMLDivElement>(null);
  const [edge, setEdge] = useState({ l: false, r: false });
  const sync = () => { const el = ref.current; if (el) setEdge({ l: el.scrollLeft > 4, r: el.scrollLeft + el.clientWidth < el.scrollWidth - 4 }); };
  useEffect(() => { sync(); const el = ref.current; if (!el) return; const ro = new ResizeObserver(sync); ro.observe(el); return () => ro.disconnect(); }, [tracks?.length]);
  if (tracks !== null && !tracks.length) return null;
  const by = (d: number) => ref.current?.scrollBy({ left: d * Math.max(300, ref.current.clientWidth - 160), behavior: "smooth" });
  return (
    <section>
      <SectionHead title={title}>{more && <Btn size="sm" onClick={more}>{t.showAll}<Icon name="chevR" size={14} /></Btn>}</SectionHead>
      {tracks === null ? <SkeletonCards /> : (
        <div className="shelf-wrap">
          <button className="shelf-nav l" hidden={!edge.l} tabIndex={-1} aria-hidden="true" onClick={() => by(-1)}><Icon name="back" size={18} /></button>
          <div className="shelf" ref={ref} onScroll={sync}>
            {tracks.map((tr, i) => (
              <button key={tr.id + i} className={"card" + (cur?.id === tr.id ? " active" : "")} onClick={() => play(tracks, i)} title={`${tr.name} · ${tr.artist}`}>
                <span className="card-cover"><Cover src={tr.image} /><span className="card-play"><Icon name="play" size={18} /></span></span>
                <b>{tr.name}</b><small>{tr.artist}</small>
              </button>
            ))}
          </div>
          <button className="shelf-nav r" hidden={!edge.r} tabIndex={-1} aria-hidden="true" onClick={() => by(1)}><Icon name="chevR" size={18} /></button>
        </div>
      )}
    </section>
  );
}

const greeting = () => { const h = new Date().getHours(); return h < 5 ? t.gNight : h < 12 ? t.gMorning : h < 18 ? t.gDay : t.gEvening; };

/** Главная: запуск Эхо, настроения, плейлист дня, недавнее, понравившееся, плейлисты, итоги. */
export function Home() {
  const { prefs, startWave, go, playlists: pls } = useApp();
  const missing = useMissingKeys();
  const [hist, setHist] = useState<Track[] | null>(null);
  const [liked, setLiked] = useState<Track[] | null>(null);
  useEffect(() => {
    api.history().then((h) => { const seen = new Set<string>(); setHist(h.filter((x) => !seen.has(x.id) && !!seen.add(x.id)).slice(0, 18)); }).catch(() => setHist([]));
    api.liked().then((l) => setLiked(l.slice(0, 18))).catch(() => setLiked([]));
  }, []);
  const noKeys = missing !== null && missing.length === 3;
  const empty = hist !== null && liked !== null && !hist.length && !liked.length && !pls.length;
  return (
    <>
      <EchoHero kicker={greeting()} />
      {noKeys && (
        <div className="notice">
          <Icon name="key" size={18} /><span>{t.addKeysHint}</span>
          <Btn variant="solid" size="sm" onClick={() => go("settings")}>{t.openSettings}</Btn>
        </div>
      )}
      <HomeMoods />
      {prefs.tastes.length > 0 && (
        <section>
          <SectionHead title={t.byGenre} />
          <div className="chips">
            {prefs.tastes.slice(0, 12).map((id) => { const g = tasteById(id); return g && (
              <Btn key={id} variant="chip" icon="echo" onClick={() => startWave([id])}>{prefs.lang === "en" ? g.en : g.ru}</Btn>
            ); })}
          </div>
        </section>
      )}
      <Shelf title={t.recent} tracks={hist} more={() => go("history")} />
      <Shelf title={t.likedShelf} tracks={liked} more={() => go("liked")} />
      {pls.length > 0 && (
        <section>
          <SectionHead title={t.myPls}><Btn size="sm" onClick={() => go("playlists")}>{t.showAll}<Icon name="chevR" size={14} /></Btn></SectionHead>
          <div className="pl-grid">{pls.slice(0, 8).map((p) => (
            <button key={p.id} className="pl-card" onClick={() => { pendingPlaylist = p.id; go("playlists"); }}>
              <span className="pl-ico"><Icon name="playlist" size={20} /></span><span><b>{p.name}</b><small>{p.count} {t.tracksN}</small></span>
            </button>
          ))}</div>
        </section>
      )}
      {empty && (
        <Empty icon="music" title={t.homeEmptyT} text={t.homeEmpty}>
          <Btn variant="primary" icon="search" onClick={() => go("search")}>{t.search}</Btn>
          <Btn variant="solid" icon="browse" onClick={() => go("browse")}>{t.browse}</Btn>
        </Empty>
      )}
      <HomeExtras />
      <Stats />
    </>
  );
}

export function Liked() {
  const { onError, ratings, play, go } = useApp();
  const [d, setD] = useState<Track[] | null>(null);
  useEffect(() => { api.liked().then(setD).catch((e) => { setD([]); onError(String(e)); }); }, [ratings]);
  return (
    <>
      <PageHead title={t.liked} sub={d?.length ? listSummary(d) : t.likedHint}>
        <Btn icon="shuffle" variant="solid" label={t.shufflePlay} disabled={!d?.length} onClick={() => play(shuffle(d!), 0)} />
        <Btn icon="play" variant="primary" disabled={!d?.length} onClick={() => play(d!, 0)}>{t.playAll}</Btn>
      </PageHead>
      {d === null ? <SkeletonRows /> : <TrackList tracks={d} empty={<Empty icon="heart" title={t.likedEmptyT} text={t.likedEmpty}><Btn variant="primary" icon="search" onClick={() => go("search")}>{t.search}</Btn></Empty>} />}
    </>
  );
}

/** Трек, отправленный в бан, не попадает в «Эхо». Жанры и исполнители не банятся. */
export function Banned() {
  const { onError, ratings, rate, blocked, setBlocked } = useApp();
  const [d, setD] = useState<Track[] | null>(null);
  useEffect(() => { api.banned().then(setD).catch((e) => { setD([]); onError(String(e)); }); }, [ratings]);
  return (
    <>
      <PageHead title={t.banned} sub={t.bannedHint} />
      {blocked.length > 0 && (
        <section>
          <SectionHead title={t.blockedArtists} />
          <div className="chips">{blocked.map((a) => <Btn key={a} variant="chip" size="sm" onClick={() => { setBlocked(a, false); toast(t.unblocked + ": " + a, "ok", { label: t.undo, run: () => setBlocked(a, true) }); }} label={t.unblockArtist}>{a}<Icon name="close" size={12} /></Btn>)}</div>
        </section>
      )}
      {blocked.length > 0 && d !== null && d.length > 0 && <SectionHead title={t.bannedTracks} />}
      {d === null ? <SkeletonRows n={3} /> : !d.length ? (!blocked.length && <Empty icon="ban" title={t.bannedEmptyT} text={t.bannedEmpty} />) : (
        <ul className="tracks">
          {d.map((tr) => (
            <li key={tr.id}>
              <span className="cover"><Cover src={tr.image} /></span>
              <div className="meta" style={{ cursor: "default" }}><b>{tr.name}</b><span><em className={"src " + tr.source}>{srcName(tr.source)}</em><span className="sub-t">{tr.artist}</span></span></div>
              <Btn variant="solid" size="sm" icon="refresh" onClick={() => { rate(tr, 0); toast(t.restored + ": " + tr.name, "ok", { label: t.undo, run: () => rate(tr, -2) }); }}>{t.restore}</Btn>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

export function History() {
  const { onError, go } = useApp();
  const [d, setD] = useState<Track[] | null>(null);
  const load = () => api.history().then(setD).catch((e) => { setD([]); onError(String(e)); });
  useEffect(() => { load(); }, []);
  const clear = async () => {
    if (!(await ask.confirm({ title: t.clearHistory + "?", body: t.clearHistoryBody, ok: t.clear, cancel: t.cancel, danger: true }))) return;
    api.clearHistory().then(() => { load(); toast(t.historyCleared, "ok"); }).catch((e) => onError(String(e)));
  };
  return (
    <>
      <PageHead title={t.history} sub={d?.length ? listSummary(d) : t.historyHint}>
        <Btn variant="solid" icon="trash" disabled={!d?.length} onClick={clear}>{t.clearHistory}</Btn>
      </PageHead>
      {d === null ? <SkeletonRows /> : <TrackList tracks={d} empty={<Empty icon="history" title={t.historyEmptyT} text={t.historyEmpty}><Btn variant="primary" onClick={() => go("home")}>{t.home}</Btn></Empty>} />}
    </>
  );
}

const SMART: [string, "smartTop" | "smartForgot" | "smartLiked" | "smartDl", IconName][] = [["top", "smartTop", "speed"], ["forgot", "smartForgot", "clock"], ["liked", "smartLiked", "heart"], ["dl", "smartDl", "download"]];

export function Playlists() {
  const { play, onError, playlists: pls, refreshPlaylists } = useApp();
  const [selId, setSelId] = useState<number | null>(() => { const p = pendingPlaylist; pendingPlaylist = null; return p; });
  const [virt, setVirt] = useState("");
  const [tracks, setTracks] = useState<Track[] | null>(null);
  const sel = pls.find((p) => p.id === selId) ?? null;
  const err = (x: unknown) => onError(String(x));
  useEffect(() => { refreshPlaylists(); }, []);
  // умные плейлисты: собираются сами из истории, оценок и скачанного
  const smart = async (k: string) => {
    setSelId(null); setVirt(k); setTracks(null);
    try {
      if (k === "top") setTracks((await api.stats()).all.tracks.map((x) => x.track));
      else if (k === "liked") setTracks(await api.liked());
      else if (k === "dl") setTracks((await api.downloads()).map(([x]) => x));
      else { const recent = new Set((await api.history()).slice(0, 300).map((x) => x.id)); setTracks((await api.liked()).filter((x) => !recent.has(x.id))); }
    } catch (e) { setTracks([]); err(e); }
  };
  const reload = (id: number) => api.playlistTracks(id).then(setTracks).catch((e) => { setTracks([]); err(e); });
  useEffect(() => { if (selId !== null) { setVirt(""); setTracks(null); reload(selId); } }, [selId]);
  const create = async () => {
    const name = await ask.prompt({ title: t.newPlaylist, placeholder: t.plNamePh, ok: t.create, cancel: t.cancel });
    if (name) api.createPlaylist(name).then((id) => { refreshPlaylists(); setSelId(id); toast(t.plCreated + ": " + name, "ok"); }).catch(err);
  };
  const rename = async (p: Playlist) => {
    const n = await ask.prompt({ title: t.rename, value: p.name, ok: t.save, cancel: t.cancel });
    if (n && n !== p.name) api.renamePlaylist(p.id, n).then(() => refreshPlaylists()).catch(err);
  };
  const remove = async (p: Playlist) => {
    if (!(await ask.confirm({ title: t.deletePlaylist + "?", body: `«${p.name}» · ${p.count} ${t.tracksN}. ${t.cantUndo}`, ok: t.del, cancel: t.cancel, danger: true }))) return;
    api.deletePlaylist(p.id).then(() => { if (selId === p.id) { setSelId(null); setTracks(null); } refreshPlaylists(); toast(t.plDeleted + ": " + p.name, "ok"); }).catch(err);
  };
  const copyList = (list: Track[]) => navigator.clipboard.writeText(list.map((x) => x.artist + " - " + x.name).join("\n")).then(() => toast(t.copied, "ok")).catch(err);
  const saveM3u = (name: string, list: Track[]) => {
    const body = "#EXTM3U\n" + list.map((x) => `#EXTINF:${x.duration || -1},${x.artist} - ${x.name}\n${x.source === "local" ? x.audio : x.url}`).join("\n");
    const a = document.createElement("a"); a.href = URL.createObjectURL(new Blob([body], { type: "audio/x-mpegurl" })); a.download = name.replace(/[\\/:*?"<>|]/g, "_") + ".m3u8"; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  };
  const title = sel ? sel.name : virt ? t[SMART.find((s) => s[0] === virt)![1]] : "";
  const plMenu = (p: Playlist) => [
    { label: t.rename, icon: "edit" as IconName, onClick: () => rename(p) },
    { label: t.exportPl, icon: "copy" as IconName, onClick: () => api.playlistTracks(p.id).then(copyList).catch(err) },
    { label: t.exportM3u, icon: "download" as IconName, onClick: () => api.playlistTracks(p.id).then((l) => saveM3u(p.name, l)).catch(err) },
    { sep: true, label: "" },
    { label: t.deletePlaylist, icon: "trash" as IconName, danger: true, onClick: () => remove(p) },
  ];
  return (
    <>
      <PageHead title={t.playlists}><Btn variant="primary" icon="plus" onClick={create}>{t.newPlaylist}</Btn></PageHead>
      <div className="split">
        <div>
          <div className="pls-h">{t.smart}</div>
          <ul className="pls">{SMART.map(([k, key, ic]) => <li key={k} className={virt === k && !sel ? "active" : ""}><button className="pl-name" onClick={() => smart(k)}><Icon name={ic} size={16} /><span>{t[key]}</span></button></li>)}</ul>
          <div className="pls-h">{t.myPls}</div>
          {pls.length ? (
            <ul className="pls">
              {pls.map((pl) => (
                <li key={pl.id} className={selId === pl.id ? "active" : ""} onContextMenu={(e) => { e.preventDefault(); (e.currentTarget.querySelector(".menu-wrap .btn") as HTMLElement | null)?.click(); }}>
                  <button className="pl-name" onClick={() => setSelId(pl.id)} onDoubleClick={() => api.playlistTracks(pl.id).then((l) => l.length && play(l, 0)).catch(err)} title={pl.name}><Icon name="playlist" size={16} /><span>{pl.name}</span><small>{pl.count}</small></button>
                  <Menu icon="more" size="sm" label={t.more} items={() => plMenu(pl)} />
                </li>
              ))}
            </ul>
          ) : <p className="set-note" style={{ margin: "4px 12px" }}>{t.plNone}</p>}
        </div>
        <div>
          {!sel && !virt ? <Empty icon="playlist" title={t.plPickT} text={t.plPick}><Btn variant="solid" icon="plus" onClick={create}>{t.newPlaylist}</Btn></Empty> : (
            <>
              <div className="sec-h">
                <div className="grow"><h3>{title}</h3>{tracks && tracks.length > 0 && <small className="muted">{listSummary(tracks)}</small>}</div>
                <div className="sec-a">
                  {sel && <Menu icon="more" label={t.more} items={() => plMenu(sel)} />}
                  <Btn icon="shuffle" variant="solid" label={t.shufflePlay} disabled={!tracks?.length} onClick={() => play(shuffle(tracks!), 0)} />
                  <Btn icon="play" variant="primary" disabled={!tracks?.length} onClick={() => play(tracks!, 0)}>{t.playAll}</Btn>
                </div>
              </div>
              {tracks === null ? <SkeletonRows /> : sel
                ? <TrackList tracks={tracks} numbered empty={<Empty icon="playlist" title={t.plEmptyT} text={t.plEmpty} />}
                    onMove={(a, b) => api.reorderPlaylist(sel.id, a, b).then(() => reload(sel.id)).catch(err)}
                    onRemove={(tr) => api.removeFromPlaylist(sel.id, tr.id).then(() => { reload(sel.id); refreshPlaylists(); toast(t.removedFromPl, "ok", { label: t.undo, run: () => api.addToPlaylist(sel.id, tr).then(() => { reload(sel.id); refreshPlaylists(); }).catch(err) }); }).catch(err)} />
                : <TrackList tracks={tracks} numbered empty={<Empty icon="playlist" title={t.plEmptyT} text={t.smartEmpty} />} />}
            </>
          )}
        </div>
      </div>
    </>
  );
}

export function Downloads() {
  const { onError, play, go } = useApp();
  const [d, setD] = useState<Track[] | null>(null);
  useEffect(() => { api.downloads().then((x) => setD(x.map(([tr]) => tr))).catch((e) => { setD([]); onError(String(e)); }); }, []);
  return (
    <>
      <PageHead title={t.downloads} sub={d?.length ? listSummary(d) : t.downloadsHint}>
        <Btn icon="shuffle" variant="solid" label={t.shufflePlay} disabled={!d?.length} onClick={() => play(shuffle(d!), 0)} />
        <Btn icon="play" variant="primary" disabled={!d?.length} onClick={() => play(d!, 0)}>{t.playAll}</Btn>
      </PageHead>
      {d === null ? <SkeletonRows /> : <TrackList tracks={d} empty={<Empty icon="download" title={t.dlEmptyT} text={t.dlEmpty}><Btn variant="solid" icon="folder" onClick={() => go("settings")}>{t.folder}</Btn></Empty>} />}
    </>
  );
}

const SET_TABS: [string, "setLook" | "setTaste" | "playbackTitle" | "setSound" | "keysTitle" | "setData" | "setHelp", IconName][] = [
  ["look", "setLook", "sun"], ["taste", "setTaste", "heart"], ["play", "playbackTitle", "play"], ["sound", "setSound", "eq"], ["keys", "keysTitle", "key"], ["data", "setData", "database"], ["help", "setHelp", "help"],
];
let lastSetTab = "look";

export function Settings() {
  const { prefs, setPrefs, resetTaste, onError } = useApp();
  const [tab, setTabState] = useState(lastSetTab);
  const setTab = (k: string) => { lastSetTab = k; setTabState(k); };
  const [confirmReset, setConfirmReset] = useState(false);
  const [alsoGroups, setAlsoGroups] = useState(false);
  const [dir, setDir] = useState("");
  const [info, setInfo] = useState<[number, string]>([0, ""]);
  useEffect(() => {
    api.getSetting("download_dir").then((x) => setDir(x ?? "")).catch(() => {});
    api.downloadsInfo().then((x) => x && setInfo(x)).catch(() => {});
  }, []);
  type BoolKey = "autoplay" | "gapless" | "normalize" | "eqOn" | "showVideo" | "langStrict" | "notify" | "autostart" | "eqAuto";
  const sw = (k: BoolKey, label: string, hint?: string, disabled?: boolean) => <Switch checked={!!prefs[k]} onChange={(v) => setPrefs({ [k]: v })} label={label} hint={hint} disabled={disabled} />;
  const split = (s: string): [string, string | undefined] => { const m = s.match(/^(.*?)\s*[(:]\s*(.+?)\)?$/); return m && m[2].length > 12 ? [m[1], m[2]] : [s, undefined]; };
  const keys: [string, string[]][] = [
    [t.hkPlay, ["Space"]], [t.hkNext, ["Ctrl", "→"]], [t.hkPrev, ["Ctrl", "←"]], [t.hkSeek, ["←", "→"]], [t.hkVol, ["↑", "↓"]], [t.hkMute, ["M"]],
    [t.hkLike, ["L"]], [t.shuffle, ["S"]], [t.repeat, ["R"]], [t.queue, ["Q"]], [t.lyrics, ["T"]], [t.search, ["Ctrl", "F"]], [t.search, ["/"]],
    [t.settings, ["Ctrl", ","]], [t.hkTabs, ["Ctrl", "1…9"]], [t.hkGlobal, ["Ctrl", "Alt", "Space"]], [t.hkGlobalSkip, ["Ctrl", "Alt", "← →"]], [t.hkRow, ["Enter"]], [t.hkMenu, ["Shift", "F10"]], [t.hkMove, ["Alt", "↑ ↓"]],
  ];
  return (
    <div className="settings">
      <PageHead title={t.settings} />
      <div className="tabs" role="tablist" aria-label={t.settings}>
        {SET_TABS.map(([k, key, ic]) => <button key={k} role="tab" aria-selected={tab === k} className={"tab" + (tab === k ? " on" : "")} onClick={() => setTab(k)}><Icon name={ic} size={16} />{t[key]}</button>)}
      </div>

      {tab === "look" && (
        <div className="set-sec" role="tabpanel">
          <h3>{t.theme}</h3>
          <div className="themes" role="radiogroup" aria-label={t.theme}>
            {THEMES.map(([id, ru, en, bg, acc]) => (
              <button key={id} role="radio" aria-checked={prefs.theme === id} className={"theme" + (prefs.theme === id ? " on" : "")} onClick={() => setPrefs({ theme: id })}>
                <span className="theme-pv" style={{ background: bg }}><i style={{ background: acc }} /><i style={{ background: acc }} /><i style={{ background: acc }} /><i style={{ background: acc }} /></span>
                <span className="theme-n">{prefs.lang === "en" ? en : ru}{prefs.theme === id && <Icon name="check" size={16} />}</span>
              </button>
            ))}
          </div>
          <h3>{t.interfaceTitle}</h3>
          <div className="set-card">
            <SetRow label={t.lang}>
              <select value={prefs.lang} onChange={(e) => setPrefs({ lang: e.target.value as typeof prefs.lang })}>
                <option value="ru">Русский</option><option value="en">English</option>
              </select>
            </SetRow>
            {sw("showVideo", ...split(t.showVideo))}
            {sw("notify", t.notifyOpt)}
            {sw("autostart", t.autostartOpt)}
          </div>
        </div>
      )}

      {tab === "taste" && (
        <div className="set-sec" role="tabpanel">
          <h3>{t.tastesTitle}</h3>
          <TastePicker value={prefs.tastes} lang={prefs.lang} onChange={(ts) => setPrefs({ tastes: ts, eq: eqFor(ts), eqOn: ts.length > 0 })} />
          <p className="set-note">{t.tastesNote}</p>

          <h3>{t.artistsTitle}</h3>
          <ArtistPicker value={prefs.artists} lang={prefs.lang} onChange={(artists) => setPrefs({ artists })} />

          <h3>{t.diversityTitle}</h3>
          <div className="chips">
            {(["familiar", "balanced", "discovery"] as const).map((m) => (
              <Btn key={m} variant="chip" active={prefs.diversity === m} onClick={() => setPrefs({ diversity: m })}>{t[m === "familiar" ? "dFamiliar" : m === "balanced" ? "dBalanced" : "dDiscovery"]}</Btn>
            ))}
          </div>
          <p className="set-note">{t.diversityNote}</p>
          <div className="set-card">
            {sw("langStrict", ...split(t.langStrict))}
            <SetRow label={t.resetTaste} hint={t.resetBody}><Btn variant="solid" icon="refresh" onClick={() => setConfirmReset(true)}>{t.resetTaste}</Btn></SetRow>
          </div>
          {confirmReset && (
            <Confirm title={t.resetTitle} body={t.resetBody} confirmLabel={t.confirm} cancelLabel={t.cancel} danger
              onCancel={() => { setConfirmReset(false); setAlsoGroups(false); }}
              onConfirm={() => { setConfirmReset(false); resetTaste(alsoGroups); setAlsoGroups(false); }}>
              <label className="chk"><input type="checkbox" checked={alsoGroups} onChange={(e) => setAlsoGroups(e.target.checked)} /> {t.resetGroups}</label>
            </Confirm>
          )}
        </div>
      )}

      {tab === "play" && (
        <div className="set-sec" role="tabpanel">
          <div className="set-card">
            {sw("autoplay", ...split(t.autoplay))}
            {sw("gapless", ...split(t.gapless))}
            <SetRow label={t.crossfadeT} hint={t.crossfadeHint}>
              <Slider min={0} max={12} value={prefs.crossfade} onChange={(v) => setPrefs({ crossfade: v })} aria-label={t.crossfadeT} />
              <span className="set-val">{prefs.crossfade ? prefs.crossfade + " " + t.sec : t.off}</span>
            </SetRow>
            <SetRow label={t.speed}>
              <div className="chips" style={{ margin: 0 }}>{[0.75, 1, 1.25, 1.5, 2].map((r) => <Btn key={r} variant="chip" size="sm" active={(prefs.rate || 1) === r} onClick={() => setPrefs({ rate: r })}>{r}×</Btn>)}</div>
            </SetRow>
            <SetRow label={t.quality} hint={t.qualityNote}>
              <select value={prefs.quality} onChange={(e) => setPrefs({ quality: e.target.value as typeof prefs.quality })}>
                <option value="low">{t.qLow}</option><option value="normal">{t.qNormal}</option><option value="high">{t.qHigh}</option>
              </select>
            </SetRow>
          </div>
          <p className="set-note">{t.fxOnly}</p>
        </div>
      )}

      {tab === "sound" && (
        <div className="set-sec" role="tabpanel">
          <div className="set-card">
            {sw("normalize", t.normalize, t.normalizeHint)}
            {sw("eqOn", t.eqOn)}
            {sw("eqAuto", t.eqAutoOpt, undefined, !prefs.eqOn)}
          </div>
          {prefs.eqOn && (
            <div className="set-card" style={{ paddingTop: 16, paddingBottom: 16 }}>
              <div className="chips">
                {Object.keys(PRESETS).map((k) => <Btn key={k} variant="chip" size="sm" active={PRESETS[k].join() === prefs.eq.join()} onClick={() => setPrefs({ eq: PRESETS[k] })}>{t[k as "flat"]}</Btn>)}
              </div>
              <div className="eq">
                {EQ_FREQS.map((f, i) => (
                  <label key={f}>
                    <span className="eq-val">{(prefs.eq[i] ?? 0) > 0 ? "+" : ""}{prefs.eq[i] ?? 0}</span>
                    <Slider min={-12} max={12} step={1} value={prefs.eq[i] ?? 0} aria-label={`${f} Hz`} onDoubleClick={() => { const eq = [...prefs.eq]; eq[i] = 0; setPrefs({ eq }); }}
                      onChange={(v) => { const eq = [...prefs.eq]; eq[i] = v; setPrefs({ eq }); }} />
                    <small>{f >= 1000 ? f / 1000 + "k" : f}</small>
                  </label>
                ))}
              </div>
            </div>
          )}
          <p className="set-note">{t.fxOnly}</p>
        </div>
      )}

      {tab === "keys" && <div className="set-sec" role="tabpanel"><Keys lang={prefs.lang} /></div>}

      {tab === "data" && (
        <div className="set-sec" role="tabpanel">
          <h3>{t.storageTitle}</h3>
          <div className="set-card">
            <SetRow label={t.dlFolderT} hint={`${t.dlInfo}: ${(info[0] / 1048576).toFixed(1)} MB · ${info[1]}`} stack>
              <form className="row grow" style={{ margin: 0, flex: 1 }} onSubmit={(e) => { e.preventDefault(); api.setSetting("download_dir", dir).then(() => { toast(t.saved, "ok"); return api.downloadsInfo().then(setInfo); }).catch((x) => onError(String(x))); }}>
                <label className="field"><Icon name="folder" size={16} /><input value={dir} onChange={(e) => setDir(e.target.value)} placeholder={t.dlDir} aria-label={t.dlDir} spellCheck={false} /></label>
                <Btn type="submit" variant="solid">{t.save}</Btn>
              </form>
            </SetRow>
            <Folder />
          </div>
          <p className="set-note">{t.cacheNote}</p>
          <Backup />
        </div>
      )}

      {tab === "help" && (
        <div className="set-sec" role="tabpanel">
          <h3>{t.hotkeysTitle}</h3>
          <div className="set-card" style={{ paddingTop: 12, paddingBottom: 12 }}>
            <div className="keys-grid">{keys.map(([name, ks], i) => <div key={i}><span>{name}</span><span>{ks.map((k) => <kbd key={k}>{k}</kbd>)}</span></div>)}</div>
          </div>
          <p className="set-note">{t.globalKeys}</p>
          <h3>{t.limitsTitle}</h3>
          <p className="set-note">{t.limits}</p>
          <h3>{t.aboutApp}</h3>
          <p className="set-note">Silo Player {__APP_VERSION__} · {t.aboutText}</p>
        </div>
      )}
    </div>
  );
}

/** Страница исполнителя: его треки в библиотеке и топ на YouTube (один поиск на исполнителя, кэш 30 дней). */
export function Artist() {
  const { artist, startWave, prefs, blocked, setBlocked, play, go } = useApp();
  const isBlocked = blocked.some((x) => x.toLowerCase() === artist.toLowerCase());
  const [tracks, setTracks] = useState<Track[]>([]);
  const [busy, setBusy] = useState(true);
  useEffect(() => {
    setBusy(true); setTracks([]);
    let live = true;
    Promise.all([
      api.searchAll({ query: artist, tags: [], offset: 0, limit: 30, sources: ["local", "jamendo"], langStrict: false }).then((r) => r.tracks).catch(() => [] as Track[]),
      api.artistTracks(artist).then(([v]) => v).catch(() => [] as Track[]),
    ]).then(([a, b]) => {
      if (!live) return;
      const low = artist.toLowerCase();
      const mine = a.filter((x) => x.artist.toLowerCase().includes(low));
      const seen = new Set<string>(); setTracks([...mine, ...b].filter((x) => !seen.has(x.id) && !!seen.add(x.id)));
    }).finally(() => live && setBusy(false));
    return () => { live = false; };
  }, [artist]);
  return (
    <>
      <PageHead title={artist} sub={busy ? t.artistLoading : tracks.length ? listSummary(tracks) : undefined} onBack={() => go("home")} backLabel={t.back}>
        <Menu icon="more" label={t.more} items={[{ label: isBlocked ? t.unblockArtist : t.blockArtist, icon: "ban", danger: !isBlocked, onClick: () => { setBlocked(artist, !isBlocked); toast((isBlocked ? t.unblocked : t.blockedT) + ": " + artist, "ok"); } }]} />
        <Btn variant="solid" icon="radio" onClick={() => startWave(prefs.tastes, [{ name: artist, tags: [] }])}>{t.artistRadio}</Btn>
        <Btn icon="play" variant="primary" disabled={!tracks.length} onClick={() => play(tracks, 0)}>{t.playAll}</Btn>
      </PageHead>
      {isBlocked && <div className="notice"><Icon name="ban" size={18} /><span>{t.blockedNote}</span><Btn size="sm" variant="solid" onClick={() => setBlocked(artist, false)}>{t.unblockArtist}</Btn></div>}
      {busy ? <SkeletonRows /> : <TrackList tracks={tracks} empty={<Empty icon="user" title={t.noResults} text={t.artistNoKey} />} />}
    </>
  );
}

/** Итоги: часы прослушивания, топ исполнителей и треков (за 30 дней и за всё время). */
export function Stats() {
  const { openArtist, play } = useApp();
  const [d, setD] = useState<Awaited<ReturnType<typeof api.stats>> | null>(null);
  const [k, setK] = useState<"month" | "all">("month");
  useEffect(() => { api.stats().then(setD).catch(() => {}); }, []);
  if (!d) return null;
  const x = d[k];
  if (!d.all.tracks.length && !d.all.artists.length) return null;
  const maxA = Math.max(1, ...x.artists.map(([, n]) => n)), maxT = Math.max(1, ...x.tracks.map((r) => r.plays));
  return (
    <section>
      <SectionHead title={t.stats}>
        <Btn variant="chip" size="sm" active={k === "month"} onClick={() => setK("month")}>{t.month}</Btn>
        <Btn variant="chip" size="sm" active={k === "all"} onClick={() => setK("all")}>{t.allTime}</Btn>
      </SectionHead>
      <div className="stats">
        <div className="stat"><h4>{t.listened}</h4><div className="stat-big">{x.hours}<small>{t.hours}</small></div></div>
        <div className="stat"><h4>{t.topArtists}</h4>
          {x.artists.length ? <ol className="stat-list">{x.artists.slice(0, 6).map(([a, n], i) => <li key={a} style={{ "--w": (n / maxA) * 100 + "%" } as React.CSSProperties} onClick={() => openArtist(a)}><span className="n">{i + 1}</span><span className="nm">{a}</span><span className="n">{n}</span></li>)}</ol> : <p className="muted">{t.statsNone}</p>}
        </div>
        <div className="stat"><h4>{t.topTracks}</h4>
          {x.tracks.length ? <ol className="stat-list">{x.tracks.slice(0, 6).map((r, i) => <li key={r.track.id} style={{ "--w": (r.plays / maxT) * 100 + "%" } as React.CSSProperties} onClick={() => play(x.tracks.map((z) => z.track), i)}><span className="n">{i + 1}</span><span className="nm">{r.track.name}<small>{r.track.artist}</small></span><span className="n">{r.plays}</span></li>)}</ol> : <p className="muted">{t.statsNone}</p>}
        </div>
      </div>
    </section>
  );
}

/** Резервная копия: настройки, плейлисты, лайки и баны в один файл (без ключей сервисов) и восстановление из него. */
function Backup() {
  const { onError, setPrefs, refreshPlaylists } = useApp();
  const [busy, setBusy] = useState("");
  const exp = async () => {
    const pls = await api.playlists();
    const data = { app: "silo", v: 1, prefs: JSON.parse((await api.getSetting("prefs")) || "{}"), liked: await api.liked(), banned: await api.banned(),
      playlists: await Promise.all(pls.map(async (p) => ({ name: p.name, tracks: await api.playlistTracks(p.id) }))) };
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([JSON.stringify(data)], { type: "application/json" }));
    a.download = "silo-backup-" + new Date().toISOString().slice(0, 10) + ".json"; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    toast(t.backupSaved, "ok");
  };
  const imp = (f: File) => f.text().then(async (txt) => {
    const d = JSON.parse(txt);
    if (d.app !== "silo") throw new Error(t.backupBad);
    if (!(await ask.confirm({ title: t.backupImport + "?", body: `${t.backupWill}: ${(d.liked ?? []).length} ${t.backupLikes}, ${(d.playlists ?? []).length} ${t.backupPls}.`, ok: t.backupImport, cancel: t.cancel }))) return;
    setBusy("imp");
    for (const tr of d.liked ?? []) await api.rate(tr, 1);
    for (const tr of d.banned ?? []) await api.rate(tr, -2);
    for (const pl of d.playlists ?? []) await api.savePlaylist(pl.name, pl.tracks);
    if (d.prefs) setPrefs(d.prefs);
    refreshPlaylists(); toast(t.backupDone, "ok");
  }).catch((x) => onError(String(x))).finally(() => setBusy(""));
  return (
    <>
      <h3>{t.backup}</h3>
      <div className="set-card">
        <SetRow label={t.backupExport} hint={t.backupHint}><Btn variant="solid" icon="download" busy={busy === "exp"} onClick={() => { setBusy("exp"); exp().catch((x) => onError(String(x))).finally(() => setBusy("")); }}>{t.backupExport}</Btn></SetRow>
        <SetRow label={t.backupImport} hint={t.backupImportHint}>
          <label className={"btn" + (busy === "imp" ? " busy" : "")} tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") (e.currentTarget.querySelector("input") as HTMLInputElement).click(); }}>
            {busy === "imp" ? <Spinner size={16} /> : <Icon name="import" size={18} />}<span>{t.chooseFile}</span><input type="file" accept=".json" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ""; if (f) imp(f); }} />
          </label>
        </SetRow>
      </div>
    </>
  );
}

/** Главная: волны под настроение и занятие. */
const MOODS: [string, string, string, IconName, number][] = [["work_focus", "Работа", "Focus", "edit", 210], ["workout", "Спорт", "Workout", "speed", 12], ["night", "Вечер", "Evening", "moon", 262], ["sleep", "Сон", "Sleep", "clock", 232], ["energetic", "Бодрое", "Upbeat", "sun", 42], ["sad", "Грустное", "Sad", "volumeLow", 200], ["romantic", "Романтика", "Romantic", "heart", 340], ["driving", "В дорогу", "Driving", "radio", 150]];
function HomeMoods() {
  const { startWave, prefs } = useApp();
  return (
    <section>
      <SectionHead title={t.moods} />
      <div className="mood-grid">{MOODS.map(([id, ru, en, ic, h]) => (
        <button key={id} className="mood" style={{ "--h": h } as React.CSSProperties} onClick={() => startWave([id, ...prefs.tastes.slice(0, 2)])}><Icon name={ic} size={22} /><span>{prefs.lang === "en" ? en : ru}</span></button>
      ))}</div>
    </section>
  );
}
/** Главная: плейлист дня и открытия недели. */
export function HomeExtras() {
  const { prefs, play, onError } = useApp();
  const [daily, setDaily] = useState<Track[]>([]);
  const [disc, setDisc] = useState<Track[]>([]);
  const [busy, setBusy] = useState(false);
  const day = new Date().toISOString().slice(0, 10);
  const week = (() => { const d = new Date(); d.setDate(d.getDate() - ((d.getDay() + 6) % 7)); return d.toISOString().slice(0, 10); })();
  const build = async (force = false) => {
    const saved = JSON.parse((await api.getSetting("daily").catch(() => "")) || "{}");
    if (!force && saved.day === day && saved.tracks?.length) { setDaily(saved.tracks); } else {
      const r = await api.echoMixFull(qualityFormat(prefs.quality), seedFor(prefs.tastes, prefs.artists), prefs.diversity, prefs.langStrict);
      setDaily(r.tracks); saved.day = day; saved.tracks = r.tracks;
    }
    if (!force && saved.week === week && saved.disc?.length) { setDisc(saved.disc); } else {
      // открытия: только исполнители, которых ещё не было в истории
      const heard = new Set((await api.history()).map((x) => x.artist.toLowerCase()));
      const r = await api.echoMixFull(qualityFormat(prefs.quality), seedFor(prefs.tastes, []), "discovery", prefs.langStrict);
      const d = r.tracks.filter((x) => !heard.has(x.artist.toLowerCase()));
      setDisc(d); saved.week = week; saved.disc = d;
    }
    api.setSetting("daily", JSON.stringify(saved)).catch(() => {});
  };
  useEffect(() => { build().catch(() => {}); }, []);
  const [moreD, setMoreD] = useState(false), [moreW, setMoreW] = useState(false);
  return (
    <>
      {daily.length > 0 && <section>
        <SectionHead title={t.daily}>
          <Btn size="sm" icon="refresh" label={t.refresh} busy={busy} onClick={() => { setBusy(true); build(true).catch((x) => onError(String(x))).finally(() => setBusy(false)); }} />
          <Btn size="sm" icon="play" variant="solid" onClick={() => play(daily, 0)}>{t.playAll}</Btn>
        </SectionHead>
        <TrackList tracks={moreD ? daily : daily.slice(0, 5)} noFilter />
        {daily.length > 5 && <Btn size="sm" className="more-btn" onClick={() => setMoreD(!moreD)}>{moreD ? t.less : `${t.showAll} (${daily.length})`}</Btn>}
      </section>}
      {disc.length > 0 && <section>
        <SectionHead title={t.discover}><Btn size="sm" icon="play" variant="solid" onClick={() => play(disc, 0)}>{t.playAll}</Btn></SectionHead>
        <TrackList tracks={moreW ? disc : disc.slice(0, 5)} noFilter />
        {disc.length > 5 && <Btn size="sm" className="more-btn" onClick={() => setMoreW(!moreW)}>{moreW ? t.less : `${t.showAll} (${disc.length})`}</Btn>}
      </section>}
    </>
  );
}

/** Своя папка с музыкой: сканирование тегов, файлы попадают в библиотеку. */
function Folder() {
  const { onError } = useApp();
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.getSetting("music_folder").then((x) => x && setPath(x)).catch(() => {}); }, []);
  const scan = () => { setBusy(true); api.scanFolder(path).then((n) => toast(t.scanned + ": " + n, "ok")).catch((x) => onError(String(x))).finally(() => setBusy(false)); };
  return (
    <SetRow label={t.folder} hint={t.folderHint} stack>
      <form className="row grow" style={{ margin: 0, flex: 1 }} onSubmit={(e) => { e.preventDefault(); if (path.trim()) scan(); }}>
        <label className="field"><Icon name="music" size={16} /><input value={path} onChange={(e) => setPath(e.target.value)} placeholder="C:\Users\...\Music" aria-label={t.folder} spellCheck={false} /></label>
        <Btn type="submit" variant="solid" busy={busy} disabled={!path.trim()}>{t.scan}</Btn>
      </form>
    </SetRow>
  );
}
