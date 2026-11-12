export type ThemeId = "sage" | "mint" | "amber" | "paper" | "ocean" | "forest" | "mono";
/** id, название (ru), название (en), фон, акцент — для образца в настройках */
export const THEMES: [ThemeId, string, string, string, string][] = [
  ["sage", "Шалфей", "Sage", "#f3efe3", "#3f6b3a"],
  ["mint", "Графит и мята", "Graphite & mint", "#14171a", "#3ddc97"],
  ["amber", "Тёплая ночь", "Warm night", "#1a1612", "#f0a830"],
  ["paper", "Бумага", "Paper", "#fafaf7", "#b04a22"],
  ["ocean", "Глубокий синий", "Deep blue", "#0c1424", "#5aa2ff"],
  ["forest", "Лес", "Forest", "#0f1a14", "#8bd17c"],
  ["mono", "Серый минимализм", "Mono gray", "#1e1e20", "#d0d0d8"],
];
export const isTheme = (x: unknown): x is ThemeId => THEMES.some(([id]) => id === x);

export interface Prefs {
  shuffle: boolean; repeat: "off" | "all" | "one"; autoplay: boolean;
  crossfade: number; gapless: boolean; normalize: boolean; eqOn: boolean; eq: number[];
  quality: "low" | "normal" | "high"; theme: ThemeId; lang: "ru" | "en"; volume: number;
  /** экран первого запуска пройден */
  onboarded: boolean;
  /** разнообразие «Эхо» */
  diversity: "familiar" | "balanced" | "discovery";
  /** выбранные жанры-вкусы (id узлов taxonomy.json) */
  tastes: string[];
  /** любимые исполнители (знакомство, настройки): мягкий стартовый плюс для «Эхо» */
  artists: { name: string; tags: string[] }[];
  /** показывать панель с видео YouTube/SoundCloud (выключено: звук играет, картинка скрыта) */
  showVideo: boolean;
  /** строгий языковой фильтр: выбранный язык исключает треки других языков в поиске, «Эхо» и подборках */
  langStrict: boolean;
  /** скорость воспроизведения 0.75–1.5 */
  rate: number;
}
export const DEFAULTS: Prefs = {
  shuffle: false, repeat: "off", autoplay: true, crossfade: 0, gapless: true, normalize: false, eqOn: false,
  eq: Array(10).fill(0), quality: "normal", theme: "amber", lang: "ru", volume: 0.8, onboarded: false, tastes: [], artists: [], diversity: "balanced", showVideo: false, langStrict: true, rate: 1,
};
export const EQ_FREQS = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
export const PRESETS: Record<string, number[]> = {
  flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  bass: [6, 5, 4, 2, 0, 0, 0, 0, 0, 0],
  vocal: [-2, -1, 0, 2, 4, 4, 3, 1, 0, -1],
  rock: [4, 3, 2, -1, -2, -1, 2, 3, 4, 4],
  treble: [0, 0, 0, 0, 0, 1, 3, 5, 6, 6],
};
/** Формат потока Jamendo: низкое mp31 (~96 кбит/с), среднее mp32 (VBR), высокое flac. */
export const qualityFormat = (q: Prefs["quality"]) => (q === "low" ? "mp31" : q === "high" ? "flac" : "mp32");
