import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Track } from "./api";
import { EQ_FREQS, Prefs } from "./prefs";

/* eslint-disable @typescript-eslint/no-explicit-any */
declare global { interface Window { YT: any; onYouTubeIframeAPIReady: () => void; SC: any } }

const scripts: Record<string, Promise<void>> = {};
function loadScript(src: string): Promise<void> {
  return (scripts[src] ??= new Promise((res, rej) => {
    const el = document.createElement("script");
    el.src = src;
    el.onerror = () => rej(new Error("не удалось загрузить " + src));
    if (src.includes("youtube")) window.onYouTubeIframeAPIReady = () => res();
    else el.onload = () => res();
    document.head.appendChild(el);
  }));
}

export interface PState { playing: boolean; pos: number; dur: number }
export interface PlayerHandle { toggle(): void; pause(): void; restart(): void; seek(frac: number): void; setVolume(v: number): void }
interface Props {
  track?: Track; nextTrack?: Track; prefs: Prefs;
  onEnded(): void; onCrossfaded(): void; onState(s: PState): void; onWarn(m: string): void;
}

/** Восстановление сессии: следующий трек загрузить на паузе с этой секунды (не играть сам). */
let hold: { id: string; at: number } | null = null;
/** Тихий зацикленный звук в главном окне, пока играет встроенный YouTube/SoundCloud: тогда плашка Windows управляет Silo (кнопки «назад/вперёд» не серые). */
const silent = (() => {
  const n = 8000, b = new ArrayBuffer(44 + n), v = new DataView(b), w = (o: number, t: string) => [...t].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  w(0, "RIFF"); v.setUint32(4, 36 + n, true); w(8, "WAVEfmt "); v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
  v.setUint32(24, 8000, true); v.setUint32(28, 8000, true); v.setUint16(32, 1, true); v.setUint16(34, 8, true); w(36, "data"); v.setUint32(40, n, true);
  for (let i = 0; i < n; i++) v.setUint8(44 + i, 128);
  const a = new Audio(URL.createObjectURL(new Blob([b], { type: "audio/wav" }))); a.loop = true; a.volume = 0.01; return a;
})();
/** Анализатор спектра для визуализатора (есть, пока включена цепочка Web Audio). */
export let analyser: AnalyserNode | null = null;
export const holdNext = (id: string, at: number) => { hold = { id, at }; };
/** Трек Deezer превратился в видео YouTube: отложенная позиция переходит к новому id. */
export const rekeyHold = (from: string, to: string) => { if (hold?.id === from) hold = { id: to, at: hold.at }; };
const isAudio = (t?: Track): boolean => !!t && t.source !== "youtube" && t.source !== "soundcloud";
const srcOf = (t?: Track) => (t!.source === "local" ? convertFileSrc(t!.audio) : t!.audio);

/**
 * Единый плеер внутри окна. Jamendo/локальные: два <audio> (gapless, кроссфейд, эквалайзер, выравнивание).
 * YouTube: IFrame Player API. SoundCloud: Widget API. Встроенные плееры показываются в приложении
 * (это требование их правил) и управляются теми же кнопками; эффекты на них не действуют.
 */
const Player = forwardRef<PlayerHandle, Props>((props, ref) => {
  const P = useRef(props);
  P.current = props;
  const decks = useRef<HTMLAudioElement[]>([]);
  if (!decks.current.length) decks.current = [new Audio(), new Audio()];
  const ids = useRef<(string | null)[]>([null, null]);
  const act = useRef(0);
  const fading = useRef(false);
  const fadeTimer = useRef<number>();
  const fx = useRef<{ ctx: AudioContext; filters: BiquadFilterNode[]; comp: DynamicsCompressorNode } | null>(null);
  const ytHost = useRef<HTMLDivElement>(null);
  const yt = useRef<any>(null);
  const sc = useRef<any>(null);
  const scS = useRef({ pos: 0, dur: 0, playing: false });

  const A = () => decks.current[act.current];
  const B = () => decks.current[1 - act.current];
  const vol = () => P.current.prefs.volume;

  const stopEmbeds = () => { try { yt.current?.pauseVideo?.(); sc.current?.pause?.(); } catch { /* ignore */ } };
  const stopFade = () => { clearInterval(fadeTimer.current); fading.current = false; };

  useImperativeHandle(ref, () => ({
    toggle() {
      const t = P.current.track;
      if (isAudio(t)) { fx.current?.ctx.resume(); A().paused ? A().play().catch(() => {}) : A().pause(); }
      else if (t?.source === "youtube") yt.current?.getPlayerState?.() === 1 ? yt.current.pauseVideo() : yt.current?.playVideo?.();
      else if (t?.source === "soundcloud") sc.current?.toggle?.();
    },
    pause() { A().pause(); stopEmbeds(); },
    /** громкость «на лету» при перетаскивании ползунка, без перерисовки приложения */
    setVolume(v: number) { if (!fading.current) A().volume = v; yt.current?.setVolume?.(v * 100); sc.current?.setVolume?.(v * 100); },
    restart() {
      const t = P.current.track;
      if (isAudio(t)) { A().currentTime = 0; A().play().catch(() => {}); }
      else if (t?.source === "youtube") { yt.current?.seekTo(0, true); yt.current?.playVideo(); }
      else if (t?.source === "soundcloud") { sc.current?.seekTo(0); sc.current?.play(); }
    },
    seek(f: number) {
      const t = P.current.track;
      if (isAudio(t)) { if (A().duration) A().currentTime = f * A().duration; }
      else if (t?.source === "youtube") { const d = yt.current?.getDuration?.(); if (d) yt.current.seekTo(f * d, true); }
      else if (t?.source === "soundcloud") sc.current?.seekTo(f * scS.current.dur * 1000);
    },
  }));

  // смена трека
  useEffect(() => {
    const tr = props.track;
    stopEmbeds();
    // плеер YouTube держит в памяти видео и отдельный процесс: убираем его, когда играет не YouTube
    if (tr?.source === "soundcloud") hold = null; // виджет SoundCloud начнёт сам
    if (tr?.source !== "youtube" && yt.current) { try { yt.current.destroy(); } catch { /* уже уничтожен */ } yt.current = null; if (ytHost.current) ytHost.current.innerHTML = ""; }
    if (!tr) { A().pause(); B().pause(); return; }
    // трек Deezer ждёт подбора видео YouTube (App), превью играет, только если полной версии нет
    if (tr.source === "deezer" && tr.kind !== "preview") { A().pause(); B().pause(); ids.current = [null, null]; return; }
    if (isAudio(tr)) {
      if (ids.current[act.current] === tr.id) return; // уже играет (кроссфейд/gapless)
      stopFade();
      const o = 1 - act.current;
      if (ids.current[o] === tr.id) { A().pause(); act.current = o; A().volume = vol(); A().play().catch(() => {}); return; }
      decks.current.forEach((d) => d.pause());
      ids.current = [null, null];
      const a = A();
      a.src = srcOf(tr); ids.current[act.current] = tr.id; a.volume = vol();
      fx.current?.ctx.resume();
      const h = hold?.id === tr.id ? hold : null; hold = null;
      if (h) { const seek = () => { a.currentTime = h.at; }; a.readyState >= 1 ? seek() : a.addEventListener("loadedmetadata", seek, { once: true }); }
      else a.play().catch(() => {});
      return;
    }
    stopFade();
    decks.current.forEach((d) => d.pause());
    ids.current = [null, null];
    if (tr.source === "youtube") {
      const vid = tr.id.replace("youtube:", "");
      const h = hold?.id === tr.id ? hold : null; hold = null;
      loadScript("https://www.youtube.com/iframe_api").then(() => {
        if (P.current.track?.id !== tr.id) return;
        if (yt.current) { if (h) yt.current.cueVideoById({ videoId: vid, startSeconds: h.at }); else yt.current.loadVideoById(vid); return; }
        const host = document.createElement("div");
        ytHost.current!.appendChild(host);
        yt.current = new window.YT.Player(host, {
          width: "100%", height: "100%", videoId: vid, playerVars: { autoplay: h ? 0 : 1, start: h ? Math.floor(h.at) : 0, playsinline: 1, rel: 0, fs: 0, iv_load_policy: 3, origin: window.location.origin },
          events: {
            onReady: (e: any) => { e.target.setVolume(vol() * 100); if (!h) e.target.playVideo(); },
            onStateChange: (e: any) => { if (e.data === 0) P.current.onEnded(); },
            onError: () => { P.current.onWarn("YouTube: видео нельзя воспроизвести во встроенном плеере, пропускаю"); P.current.onEnded(); },
          },
        });
      }).catch(() => P.current.onEnded());
    }
  }, [props.track?.id]);

  // предзагрузка следующего для gapless / кроссфейда
  useEffect(() => {
    const n = props.nextTrack, pf = props.prefs;
    if (!isAudio(n) || !isAudio(props.track) || !(pf.gapless || pf.crossfade > 0) || fading.current) return;
    const oi = 1 - act.current;
    if (ids.current[oi] === n!.id) return;
    B().src = srcOf(n); ids.current[oi] = n!.id; B().load();
  }, [props.nextTrack?.id, props.track?.id, props.prefs.gapless, props.prefs.crossfade]);

  // конец трека, ошибки
  useEffect(() => {
    const cleanups = decks.current.map((el) => {
      const ended = () => {
        if (el !== A() || fading.current) return;
        const n = P.current.nextTrack, oi = 1 - act.current;
        if (isAudio(n) && ids.current[oi] === n!.id && P.current.prefs.gapless) {
          act.current = oi; A().volume = vol(); A().play().catch(() => {}); P.current.onCrossfaded();
        } else P.current.onEnded();
      };
      const err = () => {
        if (el !== A() || !el.src) return;
        P.current.onWarn(el.crossOrigin ? "Не удалось загрузить трек. Если включены эквалайзер или выравнивание, источник может их не поддерживать — отключите их." : "Не удалось воспроизвести трек");
      };
      el.addEventListener("ended", ended); el.addEventListener("error", err);
      return () => { el.removeEventListener("ended", ended); el.removeEventListener("error", err); };
    });
    return () => cleanups.forEach((c) => c());
  }, []);

  const startFade = (len: number) => {
    fading.current = true;
    const from = A(), to = B(), t0 = Date.now();
    to.volume = 0; to.currentTime = 0; to.play().catch(() => {});
    fadeTimer.current = window.setInterval(() => {
      const p = Math.min(1, (Date.now() - t0) / 1000 / len);
      from.volume = vol() * (1 - p); to.volume = vol() * p;
      if (p >= 1) { clearInterval(fadeTimer.current); from.pause(); act.current = 1 - act.current; fading.current = false; P.current.onCrossfaded(); }
    }, 100);
  };

  // опрос состояния
  useEffect(() => {
    // в свёрнутом окне опрашиваем раз в секунду вместо 4 раз (кроссфейд успевает)
    let id = 0;
    const loop = () => { tick(); id = window.setTimeout(loop, document.hidden ? 1000 : 250); };
    const tick = () => {
      const t = P.current.track;
      const embedOn = !!t && !isAudio(t) && (t.source === "youtube" ? yt.current?.getPlayerState?.() === 1 : scS.current.playing);
      if (embedOn && silent.paused) silent.play().catch(() => {}); else if (!embedOn && !silent.paused) silent.pause();
      if (!t) { P.current.onState({ playing: false, pos: 0, dur: 0 }); return; }
      if (isAudio(t)) {
        const a = A();
        const st = { playing: !a.paused && !a.ended, pos: a.currentTime || 0, dur: isFinite(a.duration) ? a.duration : 0 };
        const cf = P.current.prefs.crossfade, n = P.current.nextTrack;
        if (cf > 0 && !fading.current && st.playing && isAudio(n) && ids.current[1 - act.current] === n!.id && st.dur - st.pos <= cf && st.dur - st.pos > 0.3) startFade(st.dur - st.pos);
        P.current.onState(st);
      } else if (t.source === "youtube" && yt.current?.getPlayerState) {
        P.current.onState({ playing: yt.current.getPlayerState() === 1, pos: yt.current.getCurrentTime?.() || 0, dur: yt.current.getDuration?.() || 0 });
      } else if (t.source === "soundcloud" && sc.current) {
        sc.current.getPosition((p: number) => (scS.current.pos = p / 1000));
        sc.current.getDuration((d: number) => (scS.current.dur = d / 1000));
        P.current.onState({ ...scS.current });
      }
    };
    loop();
    return () => clearTimeout(id);
  }, []);

  // скорость воспроизведения
  useEffect(() => {
    const r = props.prefs.rate || 1;
    decks.current.forEach((d) => (d.playbackRate = r));
    yt.current?.setPlaybackRate?.(r);
  }, [props.prefs.rate, props.track?.id]);

  // громкость
  useEffect(() => {
    if (!fading.current) A().volume = props.prefs.volume;
    yt.current?.setVolume?.(props.prefs.volume * 100);
    sc.current?.setVolume?.(props.prefs.volume * 100);
  }, [props.prefs.volume]);

  // эквалайзер и выравнивание громкости (Web Audio; только Jamendo и локальные)
  useEffect(() => {
    const pf = props.prefs;
    if ((pf.eqOn || pf.normalize) && !fx.current) {
      decks.current.forEach((el) => {
        const t = el.currentTime, was = !el.paused, has = !!el.src;
        el.crossOrigin = "anonymous";
        if (has) { const s = el.src; el.src = s; el.currentTime = t; if (was) el.play().catch(() => {}); }
      });
      const ctx = new AudioContext();
      const bus = ctx.createGain();
      decks.current.forEach((el) => ctx.createMediaElementSource(el).connect(bus));
      let node: AudioNode = bus;
      const filters = EQ_FREQS.map((f, i) => {
        const b = ctx.createBiquadFilter();
        b.type = i === 0 ? "lowshelf" : i === EQ_FREQS.length - 1 ? "highshelf" : "peaking";
        b.frequency.value = f; b.Q.value = 1.1;
        node.connect(b); node = b;
        return b;
      });
      const comp = ctx.createDynamicsCompressor();
      node.connect(comp);
      const an = ctx.createAnalyser(); an.fftSize = 128; comp.connect(an); an.connect(ctx.destination); analyser = an;
      fx.current = { ctx, filters, comp };
    }
    const f = fx.current;
    if (!f) return;
    f.filters.forEach((b, i) => (b.gain.value = pf.eqOn ? pf.eq[i] ?? 0 : 0));
    // «выравнивание»: компрессор сглаживает разницу громкости между треками (это не ReplayGain)
    f.comp.threshold.value = pf.normalize ? -26 : 0;
    f.comp.ratio.value = pf.normalize ? 4 : 1;
    f.comp.knee.value = pf.normalize ? 24 : 0;
    f.ctx.resume();
  }, [props.prefs.eqOn, props.prefs.normalize, props.prefs.eq]);

  const t = props.track;
  return (
    <div className={"embed" + (t?.source === "youtube" || t?.source === "soundcloud" ? (props.prefs.showVideo ? " show" : " hidden") : "")}>
      <div className="embed-video">
      <div ref={ytHost} className="yt" style={{ display: t?.source === "youtube" ? "block" : "none" }} />
      {t?.source === "soundcloud" && (
        <iframe key={t.id} title="SoundCloud" allow="autoplay" sandbox="allow-scripts allow-same-origin allow-presentation"
          src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(t.url)}&auto_play=true&visual=false`}
          onLoad={(e) => {
            loadScript("https://w.soundcloud.com/player/api.js").then(() => {
              const w = window.SC.Widget(e.currentTarget);
              sc.current = w; scS.current = { pos: 0, dur: 0, playing: false };
              const E = window.SC.Widget.Events;
              w.bind(E.FINISH, () => P.current.onEnded());
              w.bind(E.PLAY, () => (scS.current.playing = true));
              w.bind(E.PAUSE, () => (scS.current.playing = false));
              w.setVolume(vol() * 100);
            });
          }} />
      )}
      </div>
      {t && <div className="embed-info"><b>{t.name}</b><span>{t.artist}</span></div>}
    </div>
  );
});
export default Player;
