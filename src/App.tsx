import { Fragment, Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, ArtistSeed, Playlist, Track } from "./api";
import { setLang, srcName, t } from "./i18n";
import { AppCtx, Tab } from "./ctx";
import { DEFAULTS, Prefs, isTheme, qualityFormat } from "./prefs";
import Player, { PlayerHandle, analyser, holdNext, rekeyHold } from "./Player";
import { psGet, psSet, usePs } from "./psStore";
import { RateButtons } from "./TrackList";
import { eqOf, migrateTastes, searchNodes, seedFor } from "./tastes";
import { Artist, Banned, Browse, Downloads, Echo, History, Home, Liked, Playlists, Search, Settings } from "./views";
import { AskHost, Bars, Btn, Cover, Empty, Icon, IconName, Menu, Slider, Spinner, Toaster, ask, toast } from "./ui";
import Titlebar from "./Titlebar";
// редкие экраны грузятся по требованию: меньше стартовый код и память
const Onboarding = lazy(() => import("./Onboarding"));
const Import = lazy(() => import("./Import"));

/** Навигация слева: разделы по смыслу, у каждого своя иконка. */
const NAV: ["navListen" | "navMine" | "navMore", [Exclude<Tab, "artist">, IconName][]][] = [
  ["navListen", [["home", "home"], ["search", "search"], ["browse", "browse"], ["echo", "echo"]]],
  ["navMine", [["liked", "heart"], ["playlists", "playlist"], ["downloads", "download"], ["history", "history"]]],
  ["navMore", [["import", "import"], ["banned", "ban"], ["settings", "settings"]]],
];
const TABS: Tab[] = NAV.flatMap(([, items]) => items.map(([k]) => k));

/** Громкость: локальное значение при перетаскивании, наружу отдаём не чаще раза в кадр. */
function Volume({ value, onChange, live }: { value: number; onChange: (v: number) => void; live: (v: number) => void }) {
  const [local, setLocal] = useState(value);
  const tm = useRef(0);
  useEffect(() => setLocal(value), [value]);
  return (
    <Slider className="vol" min={0} max={100} aria-label={t.volume} value={Math.round(local * 100)}
      onChange={(x) => { const v = x / 100; setLocal(v); live(v); clearTimeout(tm.current); tm.current = window.setTimeout(() => onChange(v), 200); }}
      onWheel={(e) => { const v = Math.min(1, Math.max(0, local + (e.deltaY < 0 ? 0.05 : -0.05))); setLocal(v); live(v); onChange(v); }} />
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
        <Btn icon={ps.playing ? "pause" : "play"} variant="primary" className="round" label={ps.playing ? t.pause : t.play} onClick={() => player.current?.toggle()} />
        <Btn icon="next" label={t.next} onClick={() => skip(1)} />
        <Btn icon={prefs.repeat === "one" ? "repeatOne" : "repeat"} size="sm" label={t.repeat} active={prefs.repeat !== "off"}
          onClick={() => setPrefs({ repeat: prefs.repeat === "off" ? "all" : prefs.repeat === "all" ? "one" : "off" })} />
      </div>
      <div className="bar">
        <span className="time">{fmt(drag !== null && ps.dur ? (drag / 1000) * ps.dur : ps.pos)}</span>
        <Slider className="seek" min={0} max={1000} aria-label={t.seek} aria-valuetext={fmt(ps.pos)} disabled={!ps.dur} value={Math.round(drag ?? (ps.dur ? (ps.pos / ps.dur) * 1000 : 0))}
          onChange={(v) => setDrag(v)} onPointerUp={commit} onKeyUp={commit} onBlur={commit} />
        <span className="time">{fmt(ps.dur)}</span>
      </div>
    </div>
  );
}

/** Текст песни из LRCLIB; синхронный подсвечивает текущую строку. */
function Lyrics({ tr, onClose, seek }: { tr: Track; onClose: () => void; seek: (sec: number) => void }) {
  const ps = usePs();
  const [txt, setTxt] = useState<[string, string] | null>(null);
  useEffect(() => { setTxt(null); api.lyrics(tr.artist, tr.name, tr.duration).then(setTxt).catch(() => setTxt(["", ""])); }, [tr.id]);
  const lines = useMemo(() => (txt?.[0] ?? "").split("\n").map((l) => { const m = l.match(/^\[(\d+):(\d+(?:\.\d+)?)\](.*)$/); return m ? { at: +m[1] * 60 + +m[2], text: m[3].trim() } : null; }).filter(Boolean) as { at: number; text: string }[], [txt]);
  const cur = lines.reduce((k, l, i) => (l.at <= ps.pos ? i : k), -1);
  const box = useRef<HTMLUListElement>(null);
  useEffect(() => { box.current?.querySelector(".on")?.scrollIntoView({ block: "center", behavior: "smooth" }); }, [cur]);
  return (
    <aside className="queue lyrics" aria-label={t.lyrics}>
      <div className="panel-h"><h3>{t.lyrics}</h3><Btn icon="close" size="sm" label={t.close} onClick={onClose} /></div>
      <div className="panel-b">
        {txt === null ? <div className="empty sm"><Spinner size={22} /></div> : lines.length ? (
          <ul ref={box}>{lines.map((l, i) => <li key={i} className={i === cur ? "on" : ""} onClick={() => seek(l.at)}>{l.text || "♪"}</li>)}</ul>
        ) : txt[1] ? <pre className="plain">{txt[1]}</pre> : <Empty icon="lyrics" title={t.noLyrics} text={tr.name + " · " + tr.artist} />}
      </div>
    </aside>
  );
}

/** Визуализатор спектра на месте обложки. */
function Viz() {
  const c = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let raf = 0;
    const draw = () => {
      const cv = c.current, an = analyser;
      if (cv && an) {
        const g = cv.getContext("2d")!, d = new Uint8Array(an.frequencyBinCount); an.getByteFrequencyData(d);
        g.clearRect(0, 0, cv.width, cv.height);
        g.fillStyle = getComputedStyle(cv).color;
        const w = cv.width / d.length;
        d.forEach((v, i) => { const h = (v / 255) * cv.height; g.fillRect(i * w, cv.height - h, w - 1, h); });
      }
      raf = requestAnimationFrame(draw);
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, []);
  return <canvas ref={c} className="viz" width={112} height={112} />;
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
  const waveArtists = useRef<ArtistSeed[] | undefined>(undefined);
  const [artist, setArtist] = useState("");
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [blocked, setBlockedList] = useState<string[]>([]);
  useEffect(() => { api.getSetting("blocked_artists").then((x) => x && setBlockedList(JSON.parse(x))).catch(() => {}); }, []);
  const setBlocked = useCallback((name: string, on: boolean) => setBlockedList((o) => {
    const n = on ? [...o.filter((x) => x.toLowerCase() !== name.toLowerCase()), name] : o.filter((x) => x.toLowerCase() !== name.toLowerCase());
    api.setSetting("blocked_artists", JSON.stringify(n)).catch(() => {});
    return n;
  }), []);
  const refreshPlaylists = useCallback(() => { api.playlists().then(setPlaylists).catch(() => {}); }, []);
  const dragFrom = useRef(-1);
  const waveBusy = useRef(false);
  const waveRetry = useRef(0);
  const setErr = useCallback((m: string) => { if (m) toast(m, "error"); }, []);
  const [dropping, setDropping] = useState(false);
  const [prefs, setPrefsState] = useState<Prefs>(DEFAULTS);
  const [loaded, setLoaded] = useState(false);
  const [ratings, setRatings] = useState<Record<string, number>>({});
  // очередь: queue — треки, order — порядок воспроизведения (индексы в queue), pos — позиция в order
  const [queue, setQueue] = useState<Track[]>([]);
  const [order, setOrder] = useState<number[]>([]);
  const [pos, setPos] = useState(-1);
  const [showQ, setShowQ] = useState(false);
  const [showL, setShowL] = useState(false);
  const [mini, setMini] = useState(false);
  const [viz, setViz] = useState(false);
  /** мини-плеер: маленькое окно поверх всех */
  const toggleMini = async () => {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    const w = getCurrentWindow();
    if (!mini) { await w.setMinSize(new LogicalSize(360, 96)); await w.setSize(new LogicalSize(520, 112)); await w.setAlwaysOnTop(true); }
    else { await w.setAlwaysOnTop(false); await w.setSize(new LogicalSize(1180, 760)); await w.setMinSize(new LogicalSize(800, 560)); }
    setMini(!mini);
  };
  const [redo, setRedo] = useState(false);
  const player = useRef<PlayerHandle>(null);

  setLang(prefs.lang);
  const cur = pos >= 0 ? queue[order[pos]] : undefined;
  const S = useRef({ queue, order, pos, prefs, cur, blockedNow: blocked });
  S.current = { queue, order, pos, prefs, cur, blockedNow: blocked };

  // настройки: загрузка и сохранение в SQLite
  useEffect(() => {
    api.getSetting("prefs").then((x) => { if (x) { const p = { ...DEFAULTS, ...JSON.parse(x) }; p.tastes = migrateTastes(p.tastes ?? []); if (!isTheme(p.theme)) p.theme = DEFAULTS.theme; setPrefsState(p); } }).catch(() => {}).finally(() => setLoaded(true));
    api.ratings().then(setRatings).catch(() => {});
    refreshPlaylists();
    // продолжить с того же места: очередь, трек и позиция — на паузе
    api.getSetting("session").then((x) => {
      if (!x) return;
      const ss = JSON.parse(x) as { queue: Track[]; order: number[]; pos: number; at: number };
      if (!ss.queue?.length || ss.pos < 0 || ss.pos >= ss.order.length) return;
      const tr = ss.queue[ss.order[ss.pos]];
      if (tr) holdNext(tr.id, ss.at || 0);
      setQueue(ss.queue); setOrder(ss.order); setPos(ss.pos);
    }).catch(() => {});
  }, []);
  // сохраняем сессию при смене очереди и раз в 10 с (позиция)
  const saveSession = () => {
    const { queue: q, order: o, pos: p } = S.current;
    if (!q.length) return;
    api.setSetting("session", JSON.stringify({ queue: q.slice(0, 500), order: o.filter((i) => i < 500), pos: p, at: psGet().pos })).catch(() => {});
  };
  useEffect(() => { if (loaded) { const id = window.setTimeout(saveSession, 500); return () => clearTimeout(id); } }, [queue, order, pos, loaded]);
  useEffect(() => { const id = window.setInterval(saveSession, 10000); return () => clearInterval(id); }, []);
  const setPrefs = useCallback((p: Partial<Prefs>) => setPrefsState((o) => ({ ...o, ...p })), []);
  // сохранение настроек с задержкой (ползунки не пишут в базу на каждый пиксель) и принудительно при закрытии/сворачивании
  useEffect(() => {
    if (!loaded) return;
    const id = window.setTimeout(() => api.setSetting("prefs", JSON.stringify(prefs)).catch(() => {}), 350);
    return () => clearTimeout(id);
  }, [prefs, loaded]);
  useEffect(() => {
    const flush = () => { if (document.visibilityState === "hidden") { api.setSetting("prefs", JSON.stringify(S.current.prefs)).catch(() => {}); saveSession(); } };
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
      // забаненный трек сразу перестаёт играть и уходит из очереди
      if (v === -2) {
        if (S.current.cur?.id === tr.id) skipRef.current(1);
        const { queue: q, order: o, pos: ps } = S.current;
        setOrder(o.filter((i, k) => k <= ps || q[i]?.id !== tr.id));
        // 10+ забаненных треков исполнителя: предложить не рекомендовать его (только с согласия)
        if (tr.artist) api.banned().then((b) => {
          const n = b.filter((x) => x.artist.toLowerCase() === tr.artist.toLowerCase()).length;
          if (n >= 10 && !S.current.blockedNow.some((x) => x.toLowerCase() === tr.artist.toLowerCase())) ask.confirm({ title: t.blockArtist, body: t.blockAsk.replace("{n}", String(n)).replace("{a}", tr.artist), ok: t.blockArtist, cancel: t.cancel }).then((y) => { if (y) setBlockedRef.current(tr.artist, true); });
        }).catch(() => {});
      }
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

  // трек Deezer: видео YouTube ищется только сейчас, при запуске (один поиск, повторно — из кэша в базе)
  useEffect(() => {
    const tr = cur;
    if (!tr || tr.source !== "deezer" || tr.kind === "preview") return;
    let live = true;
    api.resolveTrack(tr).then((r) => {
      if (!live) return;
      if (r.kind === "preview") setErr(t.previewOnly);
      rekeyHold(tr.id, r.id);
      setQueue((q) => q.map((x) => (x.id === tr.id ? r : x)));
    }).catch((x) => { if (live) setErr(String(x)); });
    return () => { live = false; };
  }, [cur?.id]);
  const play = useCallback((list: Track[], i: number, keepWave = false) => {
    if (!keepWave) setWave(false); // любой ручной запуск списка выключает бесконечное «Эхо»
    setQueue(list);
    if (S.current.prefs.shuffle) { setOrder(shuffled(list.length, i)); setPos(0); } else { setOrder(list.map((_, k) => k)); setPos(i); }
  }, []);
  const enqueue = useCallback((tr: Track, next: boolean) => {
    const { queue: q, order: o, pos: p } = S.current;
    if (!q.length) { play([tr], 0); return; }
    setQueue([...q, tr]);
    const at = next ? p + 1 : o.length;
    setOrder([...o.slice(0, at), q.length, ...o.slice(at)]);
  }, []);

  /** Позиция следующего трека. manual — нажатие «вперёд» (повтор одного трека тогда не действует). -1 = очередь кончилась. */
  const nextPos = (manual: boolean) => {
    const { order: o, pos: p, prefs: pf } = S.current;
    if (pf.repeat === "one" && !manual) return p;
    if (p + 1 < o.length) return p + 1;
    return pf.repeat === "all" ? 0 : -1;
  };
  const nextIdx = nextPos(false);
  const nextTrack = pos >= 0 && nextIdx >= 0 && prefs.repeat !== "one" ? queue[order[nextIdx]] : undefined;

  const record = (fraction: number) => { const c = S.current.cur; if (c) { api.recordPlay(c, fraction).catch(() => {}); if (fraction >= 0.5) api.scrobble(c).catch(() => {}); } };
  // офлайн-режим: плашка вместо россыпи ошибок
  const [online, setOnline] = useState(navigator.onLine);
  useEffect(() => { const on = () => setOnline(true), off = () => setOnline(false); window.addEventListener("online", on); window.addEventListener("offline", off); return () => { window.removeEventListener("online", on); window.removeEventListener("offline", off); }; }, []);
  // автозапуск с Windows
  useEffect(() => { if (loaded) api.setAutostart(prefs.autostart).catch(() => {}); }, [prefs.autostart, loaded]);
  // перетащить аудиофайлы в окно — играют сразу
  useEffect(() => {
    let off: (() => void) | undefined;
    import("@tauri-apps/api/webview").then(({ getCurrentWebview }) => getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") { setDropping(true); return; }
      setDropping(false);
      if (e.payload.type !== "drop") return;
      const files = e.payload.paths.filter((x) => /\.(mp3|flac|ogg|opus|m4a|wav|aac)$/i.test(x));
      if (!files.length) { toast(t.dropBad, "info"); return; }
      const list: Track[] = files.map((f) => { const n = f.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, ""); const [a, b] = n.includes(" - ") ? n.split(" - ", 2) : ["", n];
        return { id: "local:file:" + f, source: "local", kind: "music", url: "", name: b.trim(), artist: a.trim(), album: "", image: "", audio: f, downloadUrl: "", downloadAllowed: false, duration: 0, tags: [], licenseUrl: "" }; });
      play(list, 0);
    })).then((u) => (off = u)).catch(() => {});
    return () => off?.();
  }, []);

  /** Подгрузить следующую порцию «Эхо» в конец очереди. Возвращает, сколько новых треков добавлено. */
  const extendWave = async (): Promise<number> => {
    if (waveBusy.current) return 0;
    waveBusy.current = true;
    try {
      const pf = S.current.prefs;
      const tastes = waveSeed.current ?? pf.tastes;
      const artists = waveArtists.current ?? (waveSeed.current ? [] : pf.artists);
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
  const startWave = async (seedTastes?: string[], seedArtists?: ArtistSeed[]) => {
    waveSeed.current = seedTastes;
    waveArtists.current = seedArtists;
    const pf = S.current.prefs;
    try {
      const r = await api.echoMixFull(qualityFormat(pf.quality), seedFor(seedTastes ?? pf.tastes, seedArtists ?? (seedTastes ? [] : pf.artists)), pf.diversity, pf.langStrict);
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
  const upcoming = useMemo(() => order.slice(pos + 1).map((i) => queue[i]), [order, pos, queue]);
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
      if (el.isContentEditable || document.querySelector(".scrim, .menu")) return; // открыт диалог или меню: клавиши принадлежат ему
      if (e.altKey || e.metaKey) return;
      if ((e.code === "Space" || e.key === "Enter") && ["BUTTON", "A"].includes(el.tagName)) return; // пробел на кнопке нажимает кнопку, а не паузу
      if (el.tagName === "INPUT" && e.key.startsWith("Arrow") && !e.ctrlKey) return; // стрелки двигают сам ползунок
      const p = S.current.prefs;
      if (e.code === "Space") { e.preventDefault(); player.current?.toggle(); }
      else if (e.ctrlKey && e.key === "ArrowRight") skipRef.current(1);
      else if (e.ctrlKey && e.key === "ArrowLeft") skipRef.current(-1);
      else if (e.key === "ArrowUp") { e.preventDefault(); setPrefs({ volume: Math.min(1, p.volume + 0.05) }); }
      else if (e.key === "ArrowDown") { e.preventDefault(); setPrefs({ volume: Math.max(0, p.volume - 0.05) }); }
      else if (e.ctrlKey && e.code === "KeyF") { e.preventDefault(); setTab("search"); setTimeout(() => document.getElementById("search-input")?.focus(), 50); }
      else if (!e.ctrlKey && (e.key === "ArrowRight" || e.key === "ArrowLeft")) { const ps = psGet(); if (ps.dur) player.current?.seek(Math.min(1, Math.max(0, (ps.pos + (e.key === "ArrowRight" ? 5 : -5)) / ps.dur))); }
      else if (e.code === "KeyM" && !e.ctrlKey) setPrefs({ volume: p.volume === 0 ? 0.8 : 0 });
      else if (e.code === "KeyL" && !e.ctrlKey) { const c = S.current.cur; if (c) rateRef.current(c, 1); }
      else if (e.code === "KeyS" && !e.ctrlKey) setPrefs({ shuffle: !p.shuffle });
      else if (e.code === "KeyR" && !e.ctrlKey) setPrefs({ repeat: p.repeat === "off" ? "all" : p.repeat === "all" ? "one" : "off" });
      else if (e.code === "KeyQ" && !e.ctrlKey) { setShowQ((x) => !x); setShowL(false); }
      else if (e.code === "KeyT" && !e.ctrlKey) { setShowL((x) => !x); setShowQ(false); }
      else if (e.key === "/" || (e.ctrlKey && e.code === "KeyK")) { e.preventDefault(); setTab("search"); setTimeout(() => document.getElementById("search-input")?.focus(), 50); }
      else if (e.ctrlKey && e.key === ",") { e.preventDefault(); setTab("settings"); }
      else if (e.ctrlKey && /^Digit[1-9]$/.test(e.code)) { e.preventDefault(); const k = TABS[Number(e.code.slice(5)) - 1]; if (k) setTab(k); }
      else if (e.key === "Escape") { setShowQ(false); setShowL(false); }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);
  const skipRef = useRef(skip);
  skipRef.current = skip;
  const rateRef = useRef(rate);
  const setBlockedRef = useRef(setBlocked);

  // команды из трея
  useEffect(() => {
    let off: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => listen<string>("tray", (e) => {
      if (e.payload === "toggle") player.current?.toggle();
      else if (e.payload === "next") skipRef.current(1);
      else if (e.payload === "prev") skipRef.current(-1);
    })).then((u) => (off = u));
    return () => off?.();
  }, []);

  // медиаклавиши и системные элементы управления (Media Session)
  useEffect(() => {
    if (!("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    ms.setActionHandler("play", () => player.current?.toggle());
    ms.setActionHandler("pause", () => player.current?.toggle());
    ms.setActionHandler("nexttrack", () => skipRef.current(1));
    ms.setActionHandler("previoustrack", () => skipRef.current(-1));
    ms.setActionHandler("seekto", (d) => { const ps = psGet(); if (ps.dur && d.seekTime != null) player.current?.seek(d.seekTime / ps.dur); });
    // прогресс в системной плашке Windows
    const id = window.setInterval(() => { const ps = psGet(); if (ps.dur > 0 && ps.pos <= ps.dur) try { ms.setPositionState({ duration: ps.dur, position: ps.pos, playbackRate: 1 }); ms.playbackState = ps.playing ? "playing" : "paused"; } catch { /* нет данных */ } }, 1000);
    return () => clearInterval(id);
  }, []);
  useEffect(() => {
    const title = cur ? cur.name + " — " + cur.artist : "Silo Player";
    document.title = title;
    import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title)).catch(() => {});
    // уведомление, когда окно свёрнуто или в трее
    if (cur && document.hidden && S.current.prefs.notify) api.notify(cur.name, cur.artist).catch(() => {});
    // эквалайзер под жанр трека
    const pf = S.current.prefs;
    if (cur && pf.eqAuto && pf.eqOn) { for (const tg of cur.tags) { const n = searchNodes(tg, 1)[0]; const eq = n && eqOf(n.id); if (eq) { setPrefs({ eq }); break; } } }
    if (cur && "mediaSession" in navigator) navigator.mediaSession.metadata = new MediaMetadata({ title: cur.name, artist: cur.artist, album: cur.album, artwork: cur.image ? [{ src: cur.image }] : [] });
  }, [cur?.id]);

  const View = { home: Home, search: Search, browse: Browse, echo: Echo, liked: Liked, banned: Banned, history: History, playlists: Playlists, downloads: Downloads, import: Import, settings: Settings, artist: Artist }[tab];
  const go = useCallback((k: Tab) => { setTab(k); }, []);
  const startWaveRef = useRef(startWave);
  startWaveRef.current = startWave;
  const radio = useCallback((tr: Track) => { startWaveRef.current(S.current.prefs.tastes, tr.artist ? [{ name: tr.artist, tags: tr.tags.slice(0, 3) }] : undefined); }, []);
  const openArtist = useCallback((name: string) => { setArtist(name); setTab("artist"); }, []);
  const stopWave = useCallback(() => setWave(false), []);
  const ctx = useMemo(() => ({ play, enqueue, cur, ratings, rate, onError: setErr, prefs, setPrefs, resetTaste, wave, startWave: (a?: string[], b?: ArtistSeed[]) => startWaveRef.current(a, b), stopWave, upcoming, go, radio, openArtist, artist, playlists, refreshPlaylists, blocked, setBlocked }),
    [play, enqueue, cur, ratings, rate, prefs, setPrefs, resetTaste, wave, stopWave, upcoming, go, radio, openArtist, artist, playlists, refreshPlaylists, blocked, setBlocked]);
  const saveQueue = () => {
    const list = [cur, ...upcoming].filter(Boolean) as Track[];
    if (!list.length) return;
    api.savePlaylist(t.queue + " " + new Date().toLocaleString(), list).then(() => { refreshPlaylists(); toast(t.queueSaved, "ok"); }).catch((x) => setErr(String(x)));
  };
  const moveQueue = (from: number, to: number) => {
    if (from === to || from < 0) return;
    const head = order.slice(0, pos + 1), rest = order.slice(pos + 1);
    const [m] = rest.splice(from, 1); rest.splice(to, 0, m);
    setOrder([...head, ...rest]);
  };

  const vizOn = () => { if (!analyser) { toast(t.vizHint, "info"); return; } setViz(!viz); };
  const speedItems = [0.75, 1, 1.25, 1.5, 2].map((r) => ({ label: r + "×", checked: (prefs.rate || 1) === r, onClick: () => setPrefs({ rate: r }) }));
  const dragOver = useRef<HTMLElement | null>(null);
  const markOver = (el: HTMLElement | null) => { dragOver.current?.classList.remove("drag-over"); dragOver.current = el; el?.classList.add("drag-over"); };
  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => { mainRef.current?.scrollTo({ top: 0, behavior: "instant" as ScrollBehavior }); }, [tab, artist]);

  if (!loaded) return <><Titlebar /><div className="onb"><Spinner size={28} /></div></>;
  if (!prefs.onboarded) return <><Titlebar /><Suspense fallback={<div className="onb"><Spinner size={28} /></div>}><Onboarding prefs={prefs} done={(x) => { setRedo(false); setPrefs(x); }} startStep={redo ? 1 : 0} /></Suspense><Toaster /><AskHost /></>;

  return (
    <AppCtx.Provider value={ctx}>
      <Titlebar now={cur ? cur.name + " · " + cur.artist : ""} />
      <div className={"app" + (mini ? " mini" : "")}>
        <nav aria-label={t.navLabel}>
          {NAV.map(([group, items]) => (
            <Fragment key={group}>
              <div className="nav-g">{t[group]}</div>
              {items.map(([k, ic]) => (
                <Btn key={k} variant="nav" icon={ic} active={tab === k} label={t[k]} aria-current={tab === k ? "page" : undefined} onClick={() => setTab(k)}>
                  {t[k]}{k === "echo" && wave ? <span className="nav-live" title={t.echoPlaying} /> : null}
                </Btn>
              ))}
            </Fragment>
          ))}
        </nav>
        <main ref={mainRef}>
          {!online && <div className="offline-bar" role="status"><Icon name="wifiOff" size={16} />{t.offline}</div>}
          <div className="view" key={tab + ":" + (tab === "artist" ? artist : "")}>
            <Suspense fallback={<div className="empty"><Spinner size={26} /></div>}><View /></Suspense>
          </div>
        </main>
        <div className="panels">
          {showQ && !mini && (
            <aside className="queue" aria-label={t.queue}>
              <div className="panel-h">
                <h3>{t.queue}{upcoming.length > 0 && <small>{upcoming.length}</small>}</h3>
                <Btn icon="save" size="sm" label={t.saveQueue} disabled={!cur} onClick={saveQueue} />
                <Btn icon="trash" size="sm" label={t.clearQueue} disabled={!upcoming.length} onClick={() => { const old = order; setOrder(order.slice(0, pos + 1)); toast(t.queueCleared, "info", { label: t.undo, run: () => setOrder(old) }); }} />
                <Btn icon="close" size="sm" label={t.close} onClick={() => setShowQ(false)} />
              </div>
              <div className="panel-b">
                {cur && <><div className="panel-sub">{t.nowPlaying}</div>
                  <div className="q-i cur"><span className="cover"><Cover src={cur.image} /></span><div className="meta"><b>{cur.name}</b><span>{cur.artist}</span></div><Bars /></div></>}
                {upcoming.length > 0 ? <>
                  <div className="panel-sub">{wave ? t.upNext : t.nextUp}</div>
                  <ul>
                    {upcoming.map((tr, i) => (
                      <li key={tr.id + i} className="q-i" draggable tabIndex={0}
                        onDragStart={(e) => { dragFrom.current = i; e.dataTransfer.effectAllowed = "move"; e.currentTarget.classList.add("dragging"); }}
                        onDragEnd={(e) => { e.currentTarget.classList.remove("dragging"); markOver(null); }}
                        onDragOver={(e) => { e.preventDefault(); markOver(e.currentTarget); }}
                        onDrop={() => { moveQueue(dragFrom.current, i); dragFrom.current = -1; markOver(null); }}
                        onKeyDown={(e) => { if (e.key === "Enter") setPos(pos + 1 + i); else if (e.key === "Delete") { const at = pos + 1 + i; setOrder(order.filter((_, k) => k !== at)); } else if (e.altKey && e.key === "ArrowUp" && i > 0) moveQueue(i, i - 1); else if (e.altKey && e.key === "ArrowDown" && i < upcoming.length - 1) moveQueue(i, i + 1); }}
                        onClick={() => setPos(pos + 1 + i)}>
                        <span className="q-grip"><Icon name="grip" size={14} /></span>
                        <span className="cover"><Cover src={tr.image} /></span>
                        <div className="meta"><b>{tr.name}</b><span>{tr.artist}</span></div>
                        <Btn icon="close" size="sm" label={t.removeFromQueue} onClick={(e) => { e.stopPropagation(); const at = pos + 1 + i; setOrder(order.filter((_, k) => k !== at)); }} />
                      </li>
                    ))}
                  </ul>
                </> : <Empty icon="queue" title={t.queueEmptyT} text={t.queueEmpty} />}
              </div>
            </aside>
          )}
          {showL && cur && !mini && <Lyrics tr={cur} onClose={() => setShowL(false)} seek={(sec) => { const d = psGet().dur; if (d) player.current?.seek(sec / d); }} />}
          <Player ref={player} track={cur} nextTrack={nextTrack} prefs={prefs} onEnded={ended} onCrossfaded={crossfaded} onState={psSet} onWarn={setErr} />
        </div>
        <footer aria-label={t.playerLabel}>
          <div className="now">
            {cur ? <>
              <button className="now-cover" onClick={vizOn} aria-label={t.vizToggle} title={t.vizToggle}>{viz && analyser ? <Viz /> : <Cover src={cur.image} />}</button>
              <div className="np"><b title={cur.name}>{cur.name}</b>{cur.artist ? <a className="artist-link" role="link" tabIndex={0} onClick={() => !mini && openArtist(cur.artist)} onKeyDown={(e) => { if (e.key === "Enter") openArtist(cur.artist); }}>{cur.artist}</a> : <span>{srcName(cur.source)}</span>}</div>
              <div className="now-acts"><RateButtons tr={cur} /></div>
            </> : <div className="now-idle"><span className="now-cover" style={{ cursor: "default" }}><Cover /></span>{t.nothingPlaying}</div>}
          </div>
          <Transport player={player} skip={skip} prefs={prefs} setPrefs={setPrefs} />
          <div className="side">
            <Btn icon={prefs.volume === 0 ? "mute" : prefs.volume < 0.4 ? "volumeLow" : "volume"} size="sm" label={t.volume + " (M)"} onClick={() => setPrefs({ volume: prefs.volume === 0 ? 0.8 : 0 })} />
            <Volume value={prefs.volume} live={(v) => player.current?.setVolume(v)} onChange={(v) => setPrefs({ volume: v })} />
            <span className="hide-sm"><Menu icon="speed" size="sm" up label={t.speed + ": " + (prefs.rate || 1) + "×"} active={(prefs.rate || 1) !== 1} items={speedItems} /></span>
            <Btn icon="lyrics" size="sm" label={t.lyrics + " (T)"} active={showL} disabled={!cur} onClick={() => { setShowL(!showL); setShowQ(false); }} />
            <Btn icon="queue" size="sm" label={t.queue + " (Q)"} active={showQ} onClick={() => { setShowQ(!showQ); setShowL(false); }} />
            <Btn icon={mini ? "expand" : "mini"} size="sm" className="keep" label={mini ? t.miniOff : t.mini} active={mini} onClick={toggleMini} />
          </div>
        </footer>
      </div>
      {dropping && <div className="drop-zone"><div><Icon name="music" size={40} />{t.dropHere}</div></div>}
      <Toaster />
      <AskHost />
    </AppCtx.Provider>
  );
}
