import { useCallback, useEffect, useRef, useState } from "react";
import { api, Track } from "./api";
import { setLang, t } from "./i18n";
import { AppCtx, Tab } from "./ctx";
import { DEFAULTS, Prefs, isTheme, qualityFormat } from "./prefs";
import Player, { PlayerHandle } from "./Player";
import { psGet, psSet, usePs } from "./psStore";
import { RateButtons } from "./TrackList";
import Onboarding from "./Onboarding";
import { migrateTastes, seedFor } from "./tastes";
import { Banned, Browse, Downloads, Echo, History, Home, Liked, Playlists, Search, Settings } from "./views";
import { Btn } from "./ui";
import Titlebar from "./Titlebar";
import Import from "./Import";

const TABS: Tab[] = ["home", "search", "browse", "echo", "liked", "banned", "history", "playlists", "downloads", "import", "settings"];

/** Громкость: локальное значение при перетаскивании, наружу отдаём не чаще раза в кадр. */
function Volume({ value, onChange, live }: { value: number; onChange: (v: number) => void; live: (v: number) => void }) {
  const [local, setLocal] = useState(value);
  const tm = useRef(0);
  useEffect(() => setLocal(value), [value]);
  return (
    <input className="vol" type="range" min={0} max={100} aria-label={t.volume} value={local * 100}
      onChange={(e) => { const v = Number(e.target.value) / 100; setLocal(v); live(v); clearTimeout(tm.current); tm.current = window.setTimeout(() => onChange(v), 200); }} />
  );
}

/** Нижняя панель: только она перерисовывается каждый тик воспроизведения. */
function Transport({ player, skip, prefs, setPrefs }: { player: React.RefObject<PlayerHandle>; skip: (d: 1 | -1) => void; prefs: Prefs; setPrefs: (p: Partial<Prefs>) => void }) {
  const ps = usePs();
  const [drag, setDrag] = useState<number | null>(null); // значение ползунка, пока его тянут: плеер не дёргаем на каждый пиксель
  const commit = () => { if (drag !== null) { player.current?.seek(drag / 1000); setDrag(null); } };
  return (
    <div className="transport">
      <div className="ctrls">
        <Btn icon="shuffle" size="sm" label={t.shuffle} active={prefs.shuffle} onClick={() => setPrefs({ shuffle: !prefs.shuffle })} />
        <Btn icon="prev" label={t.prev} onClick={() => skip(-1)} />
        <Btn icon={ps.playing ? "pause" : "play"} variant="primary" size="lg" className="round" label={ps.playing ? t.pause : t.play} onClick={() => player.current?.toggle()} />
        <Btn icon="next" label={t.next} onClick={() => skip(1)} />
        <Btn icon={prefs.repeat === "one" ? "repeatOne" : "repeat"} size="sm" label={t.repeat} active={prefs.repeat !== "off"}
          onClick={() => setPrefs({ repeat: prefs.repeat === "off" ? "all" : prefs.repeat === "all" ? "one" : "off" })} />
      </div>
      <div className="bar">
        <span className="time">{fmt(drag !== null && ps.dur ? (drag / 1000) * ps.dur : ps.pos)}</span>
        <input className="seek" type="range" min={0} max={1000} aria-label="seek" value={drag ?? (ps.dur ? (ps.pos / ps.dur) * 1000 : 0)}
          onChange={(e) => setDrag(Number(e.target.value))} onPointerUp={commit} onKeyUp={commit} onBlur={commit} />
        <span className="time">{fmt(ps.dur)}</span>
      </div>
    </div>
  );
}

const shuffled = (n: number, first: number) => {
  const r = Array.from({ length: n }, (_, i) => i).filter((i) => i !== first);
  for (let i = r.length - 1; i > 0; i--) { const j = Math.floor(Math.random() * (i + 1)); [r[i], r[j]] = [r[j], r[i]]; }
  return first >= 0 ? [first, ...r] : r;
};
const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

export default function App() {
  const [tab, setTab] = useState<Tab>("home");
  const [wave, setWave] = useState(false);
  const waveSeed = useRef<string[] | undefined>(undefined);
  const waveBusy = useRef(false);
  const waveRetry = useRef(0);
  const [err, setErr] = useState("");
  const [prefs, setPrefsState] = useState<Prefs>(DEFAULTS);
  const [loaded, setLoaded] = useState(false);
  const [ratings, setRatings] = useState<Record<string, number>>({});
  // очередь: queue — треки, order — порядок воспроизведения (индексы в queue), pos — позиция в order
  const [queue, setQueue] = useState<Track[]>([]);
  const [order, setOrder] = useState<number[]>([]);
  const [pos, setPos] = useState(-1);
  const [showQ, setShowQ] = useState(false);
  const [redo, setRedo] = useState(false);
  const player = useRef<PlayerHandle>(null);

  setLang(prefs.lang);
  const cur = pos >= 0 ? queue[order[pos]] : undefined;
  const S = useRef({ queue, order, pos, prefs, cur });
  S.current = { queue, order, pos, prefs, cur };

  // настройки: загрузка и сохранение в SQLite
  useEffect(() => {
    api.getSetting("prefs").then((x) => { if (x) { const p = { ...DEFAULTS, ...JSON.parse(x) }; p.tastes = migrateTastes(p.tastes ?? []); if (!isTheme(p.theme)) p.theme = DEFAULTS.theme; setPrefsState(p); } }).catch(() => {}).finally(() => setLoaded(true));
    api.ratings().then(setRatings).catch(() => {});
  }, []);
  const setPrefs = useCallback((p: Partial<Prefs>) => setPrefsState((o) => ({ ...o, ...p })), []);
  // сохранение настроек с задержкой (ползунки не пишут в базу на каждый пиксель) и принудительно при закрытии/сворачивании
  useEffect(() => {
    if (!loaded) return;
    const id = window.setTimeout(() => api.setSetting("prefs", JSON.stringify(prefs)).catch(() => {}), 350);
    return () => clearTimeout(id);
  }, [prefs, loaded]);
  useEffect(() => {
    const flush = () => { if (document.visibilityState === "hidden") api.setSetting("prefs", JSON.stringify(S.current.prefs)).catch(() => {}); };
    document.addEventListener("visibilitychange", flush);
    window.addEventListener("pagehide", flush);
    return () => { document.removeEventListener("visibilitychange", flush); window.removeEventListener("pagehide", flush); };
  }, []);
  useEffect(() => { document.documentElement.dataset.theme = prefs.theme; }, [prefs.theme]);
  useEffect(() => {
    if (!loaded || !queue.length) return;
    // смена shuffle на лету: текущий трек остаётся первым
    const c = order[pos];
    if (prefs.shuffle) { setOrder(shuffled(queue.length, c)); setPos(0); } else { setOrder(queue.map((_, i) => i)); setPos(c); }
  }, [prefs.shuffle]);

  const rate = useCallback((tr: Track, v: number) => {
    api.rate(tr, v).then(() => {
      setRatings((r) => { const n = { ...r }; if (v) n[tr.id] = v; else delete n[tr.id]; return n; });
      // забаненный трек сразу перестаёт играть: переходим к следующему
      if (v === -2 && S.current.cur?.id === tr.id) skipRef.current(1);
    }).catch((x) => setErr(String(x)));
  }, []);
  /** «Сбросить вкусы»: чистим модель и историю, при желании и оценки; затем заново выбор жанров. */
  const resetTaste = useCallback((clearGroups: boolean) => {
    api.resetTaste(clearGroups).then(() => {
      if (clearGroups) setRatings({});
      setRedo(true);
      setPrefs({ onboarded: false, tastes: [], artists: [], eq: Array(10).fill(0), eqOn: false });
    }).catch((x) => setErr(String(x)));
  }, [setPrefs]);

  const play = (list: Track[], i: number, keepWave = false) => {
    if (!keepWave) setWave(false); // любой ручной запуск списка выключает бесконечное «Эхо»
    setQueue(list);
    if (prefs.shuffle) { setOrder(shuffled(list.length, i)); setPos(0); } else { setOrder(list.map((_, k) => k)); setPos(i); }
  };
  const enqueue = (tr: Track, next: boolean) => {
    const { queue: q, order: o, pos: p } = S.current;
    if (!q.length) { play([tr], 0); return; }
    setQueue([...q, tr]);
    const at = next ? p + 1 : o.length;
    setOrder([...o.slice(0, at), q.length, ...o.slice(at)]);
  };

  /** Позиция следующего трека. manual — нажатие «вперёд» (повтор одного трека тогда не действует). -1 = очередь кончилась. */
  const nextPos = (manual: boolean) => {
    const { order: o, pos: p, prefs: pf } = S.current;
    if (pf.repeat === "one" && !manual) return p;
    if (p + 1 < o.length) return p + 1;
    return pf.repeat === "all" ? 0 : -1;
  };
  const nextIdx = nextPos(false);
  const nextTrack = pos >= 0 && nextIdx >= 0 && prefs.repeat !== "one" ? queue[order[nextIdx]] : undefined;

  const record = (fraction: number) => { const c = S.current.cur; if (c) api.recordPlay(c, fraction).catch(() => {}); };

  /** Подгрузить следующую порцию «Эхо» в конец очереди. Возвращает, сколько новых треков добавлено. */
  const extendWave = async (): Promise<number> => {
    if (waveBusy.current) return 0;
    waveBusy.current = true;
    try {
      const pf = S.current.prefs;
      const tastes = waveSeed.current ?? pf.tastes;
      const artists = waveSeed.current ? [] : pf.artists;
      const r = await api.echoMixFull(qualityFormat(pf.quality), seedFor(tastes, artists), pf.diversity, pf.langStrict);
      const have = new Set(S.current.queue.map((x) => x.id));
      const more = r.tracks.filter((x) => !have.has(x.id));
      if (more.length) {
        const q = S.current.queue, o = S.current.order;
        setQueue([...q, ...more]);
        setOrder([...o, ...more.map((_, i) => q.length + i)]);
      }
      return more.length;
    } catch (e) { setErr(String(e)); return 0; }
    finally { waveBusy.current = false; }
  };
  const startWave = async (seedTastes?: string[]) => {
    waveSeed.current = seedTastes;
    const pf = S.current.prefs;
    try {
      const r = await api.echoMixFull(qualityFormat(pf.quality), seedFor(seedTastes ?? pf.tastes, seedTastes ? [] : pf.artists), pf.diversity, pf.langStrict);
      if (!r.tracks.length) { setErr(t.echoEmpty); return; }
      setWave(true);
      play(r.tracks, 0, true);
    } catch (e) { setErr(String(e)); }
  };
  const skip = async (d: 1 | -1) => {
    const ps = psGet();
    if (ps.dur && ps.pos / ps.dur < 0.8) record(ps.pos / ps.dur); // сколько реально прослушано — градуированный сигнал для модели вкуса
    if (d === -1) { if (ps.pos > 3) player.current?.seek(0); else setPos(Math.max(0, S.current.pos - 1)); return; }
    const n = nextPos(true);
    if (n >= 0) setPos(n);
  };
  const ended = async () => {
    record(1);
    const n = nextPos(false);
    if (S.current.prefs.repeat === "one") { player.current?.restart(); return; }
    if (n >= 0) { setPos(n); return; }
    if (S.current.prefs.autoplay || wave) {
      // очередь кончилась раньше, чем подгрузилась новая порция: догружаем и продолжаем с первого нового трека
      const at = S.current.order.length;
      if ((await extendWave()) > 0) setPos(at);
    }
  };

  // «Эхо» как бесконечный поток: когда впереди осталось меньше 6 треков, заранее подгружаем следующую порцию
  const upcoming = order.slice(pos + 1).map((i) => queue[i]);
  useEffect(() => {
    if (!wave || pos < 0 || upcoming.length >= 6 || waveBusy.current || Date.now() < waveRetry.current) return;
    extendWave().then((n) => { if (n === 0) waveRetry.current = Date.now() + 30000; }); // нет новых — не долбим источники чаще раза в 30 с
  }, [wave, pos, queue.length]);
  const crossfaded = () => { record(1); const n = nextPos(false); if (n >= 0) setPos(n); };

  // горячие клавиши
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (["INPUT", "SELECT", "TEXTAREA"].includes(el.tagName) && !(el as HTMLInputElement).type?.match(/range|checkbox/)) return;
      const p = S.current.prefs;
      if (e.code === "Space") { e.preventDefault(); player.current?.toggle(); }
      else if (e.ctrlKey && e.key === "ArrowRight") skipRef.current(1);
      else if (e.ctrlKey && e.key === "ArrowLeft") skipRef.current(-1);
      else if (e.key === "ArrowUp") { e.preventDefault(); setPrefs({ volume: Math.min(1, p.volume + 0.05) }); }
      else if (e.key === "ArrowDown") { e.preventDefault(); setPrefs({ volume: Math.max(0, p.volume - 0.05) }); }
      else if (e.key === "s" && !e.ctrlKey) setPrefs({ shuffle: !p.shuffle });
      else if (e.key === "r" && !e.ctrlKey) setPrefs({ repeat: p.repeat === "off" ? "all" : p.repeat === "all" ? "one" : "off" });
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);
  const skipRef = useRef(skip);
  skipRef.current = skip;

  // медиаклавиши и системные элементы управления (Media Session)
  useEffect(() => {
    if (!("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    if (cur) ms.metadata = new MediaMetadata({ title: cur.name, artist: cur.artist, album: cur.album, artwork: cur.image ? [{ src: cur.image }] : [] });
    ms.setActionHandler("play", () => player.current?.toggle());
    ms.setActionHandler("pause", () => player.current?.toggle());
    ms.setActionHandler("nexttrack", () => skipRef.current(1));
    ms.setActionHandler("previoustrack", () => skipRef.current(-1));
  }, [cur?.id]);

  const View = { home: Home, search: Search, browse: Browse, echo: Echo, liked: Liked, banned: Banned, history: History, playlists: Playlists, downloads: Downloads, import: Import, settings: Settings }[tab];

  if (!loaded) return null;
  if (!prefs.onboarded) return <><Titlebar /><Onboarding prefs={prefs} done={(x) => { setRedo(false); setPrefs(x); }} startStep={redo ? 1 : 0} /></>;

  return (
    <AppCtx.Provider value={{ play, enqueue, cur, ratings, rate, onError: setErr, prefs, setPrefs, resetTaste, wave, startWave, stopWave: () => setWave(false), upcoming, go: (k) => { setTab(k); setErr(""); } }}>
      <Titlebar />
      <div className="app">
        <nav>
          <h1>Silo</h1>
          {TABS.map((k) => <Btn key={k} variant="nav" active={tab === k} onClick={() => { setTab(k); setErr(""); }}>{t[k]}{k === "echo" && wave ? " ●" : ""}</Btn>)}
        </nav>
        <main>
          {err && <div className="err" onClick={() => setErr("")}>{err}</div>}
          <View />
        </main>
        {showQ && (
          <aside className="queue">
            <div className="row head"><div className="title"><b>{t.queue}</b><span className="muted">{upcoming.length}</span></div><Btn icon="close" size="sm" label={t.queue} onClick={() => setShowQ(false)} /></div>
            <ul>
              {upcoming.map((tr, i) => (
                <li key={tr.id + i}>
                  <span onClick={() => setPos(pos + 1 + i)}>{tr.name} <small>{tr.artist}</small></span>
                  <Btn icon="close" size="sm" label={t.removeFromQueue} onClick={() => { const at = pos + 1 + i; setOrder(order.filter((_, k) => k !== at)); }} />
                </li>
              ))}
            </ul>
          </aside>
        )}
        <Player ref={player} track={cur} nextTrack={nextTrack} prefs={prefs} onEnded={ended} onCrossfaded={crossfaded} onState={psSet} onWarn={setErr} />
        <footer>
          <div className="now">
            {cur ? <>{cur.image ? <img src={cur.image} alt="" /> : <span className="ph" />}<div className="np"><b>{cur.name}</b><span>{cur.artist}</span></div><RateButtons tr={cur} /></> : <div className="np" />}
          </div>
          <Transport player={player} skip={skip} prefs={prefs} setPrefs={setPrefs} />
          <div className="side">
            <Btn icon={prefs.volume === 0 ? "mute" : "volume"} size="sm" label={t.volume} onClick={() => setPrefs({ volume: prefs.volume === 0 ? 0.8 : 0 })} />
            <Volume value={prefs.volume} live={(v) => player.current?.setVolume(v)} onChange={(v) => setPrefs({ volume: v })} />
            <Btn icon="queue" size="sm" label={t.queue} active={showQ} onClick={() => setShowQ(!showQ)} />
          </div>
        </footer>
      </div>
    </AppCtx.Provider>
  );
}
