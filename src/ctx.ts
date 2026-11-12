import { createContext, useContext } from "react";
import { Track } from "./api";
import { Prefs } from "./prefs";

export type Tab = "home" | "search" | "browse" | "echo" | "liked" | "banned" | "history" | "playlists" | "downloads" | "import" | "settings";

export interface Ctx {
  play: (queue: Track[], i: number) => void;
  enqueue: (t: Track, next: boolean) => void;
  cur?: Track;
  ratings: Record<string, number>;
  rate: (t: Track, v: number) => void;
  onError: (m: string) => void;
  prefs: Prefs;
  setPrefs: (p: Partial<Prefs>) => void;
  resetTaste: (clearGroups: boolean) => void;
  /** «Эхо» — бесконечный поток: сам подгружает новые треки. seed — жанры вкуса для этой волны (по умолчанию — вкусы из настроек). */
  wave: boolean;
  startWave: (seedTastes?: string[]) => void;
  stopWave: () => void;
  upcoming: Track[];
  go: (tab: Tab) => void;
}
export const AppCtx = createContext<Ctx>(null as unknown as Ctx);
export const useApp = () => useContext(AppCtx);
