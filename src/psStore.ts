import { useSyncExternalStore } from "react";
import type { PState } from "./Player";

/** Состояние воспроизведения (позиция, пауза) живёт вне React-дерева App: тик плеера обновляет только полосу перемотки, а не всё приложение. */
let state: PState = { playing: false, pos: 0, dur: 0 };
const listeners = new Set<() => void>();

export const psGet = () => state;
export const psSet = (n: PState) => {
  if (n.playing === state.playing && n.dur === state.dur && Math.abs(n.pos - state.pos) < 0.2) return;
  state = n;
  listeners.forEach((f) => f());
};
export const usePs = () =>
  useSyncExternalStore((cb) => { listeners.add(cb); return () => { listeners.delete(cb); }; }, psGet);
