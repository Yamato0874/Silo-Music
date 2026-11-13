/* Стенд для разработки интерфейса в обычном браузере (npm run dev без Tauri): подменяет команды ядра тестовыми данными.
   В сборку приложения не попадает (подключается только при import.meta.env.DEV и отсутствии Tauri). */
/* eslint-disable @typescript-eslint/no-explicit-any */
const cover = (i: number) => {
  const h = (i * 47) % 360;
  return "data:image/svg+xml," + encodeURIComponent(`<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='hsl(${h},55%,45%)'/><stop offset='1' stop-color='hsl(${(h + 70) % 360},60%,25%)'/></linearGradient></defs><rect width='160' height='160' fill='url(#g)'/><circle cx='${40 + (i * 13) % 80}' cy='${50 + (i * 29) % 60}' r='${24 + (i * 7) % 30}' fill='#fff2'/></svg>`);
};
const NAMES = ["Северный ветер", "Midnight Drive", "Тихий океан", "Glass Houses", "Последний трамвай", "Echoes of You", "Город засыпает", "Paper Planes at Dawn", "Соль и мёд", "Low Tide", "Очень длинное название трека, которое не помещается в одну строку и должно обрезаться", "Neon Rain", "Белые ночи", "Slow Motion", "Кометы", "Afterglow"];
const ARTISTS = ["Мельница света", "The Wandering", "Аврора", "Kite Club", "Hollow Coves", "Синий кит", "Ólafur Jónsson", "Polar Bears on Holiday"];
const SRC = ["jamendo", "youtube", "soundcloud", "local"];
const tr = (i: number) => ({ id: "t" + i, source: SRC[i % 4], kind: "music", url: "https://example.com/" + i, name: NAMES[i % NAMES.length], artist: ARTISTS[i % ARTISTS.length], album: "Альбом " + (i % 5), image: i % 7 === 6 ? "" : cover(i), audio: "", downloadUrl: "", downloadAllowed: i % 4 === 0, duration: 150 + ((i * 37) % 200), tags: ["indie", "rock", "chillout"].slice(0, 1 + (i % 3)), licenseUrl: i % 4 === 0 ? "https://creativecommons.org/licenses/by/4.0/" : "" });
const list = (n: number, from = 0) => Array.from({ length: n }, (_, i) => tr(from + i));
const store: Record<string, string> = {
  prefs: JSON.stringify({ onboarded: !location.search.includes("onb"), tastes: ["indie_rock", "ambient", "synthwave"], theme: new URLSearchParams(location.search).get("theme") || "amber", volume: 0.7 }),
  session: location.search.includes("empty") ? "" : JSON.stringify({ queue: list(12), order: list(12).map((_, i) => i), pos: 2, at: 42 }),
  search_history: JSON.stringify(["hollow coves", "ambient focus", "кино"]),
};
const empty = location.search.includes("empty");
const pls = empty ? [] : [{ id: 1, name: "Для работы", count: 42 }, { id: 2, name: "Дорога", count: 17 }, { id: 3, name: "Вечер пятницы и очень длинное имя", count: 8 }];
const stats = { hours: 37, artists: ARTISTS.slice(0, 6).map((a, i) => [a, 40 - i * 5]), tracks: list(8).map((t, i) => ({ track: t, plays: 30 - i * 3 })) };
const handlers: Record<string, (a: any) => unknown> = {
  get_setting: (a) => store[a.key] ?? "",
  set_setting: (a) => { store[a.key] = a.value; },
  all_ratings: () => (empty ? {} : { t1: 1, t3: -1, t5: 1 }),
  list_playlists: () => pls,
  playlist_tracks: (a) => list(a.id === 1 ? 42 : 9, a.id * 3),
  history: () => (empty ? [] : list(40, 3)),
  liked_tracks: () => (empty ? [] : list(24, 8)),
  banned_tracks: () => (empty ? [] : list(3, 20)),
  list_downloads: () => (empty ? [] : list(6, 4).map((t) => [t, "C:/x.mp3"])),
  downloads_info: () => [52428800, "C:\\Users\\me\\Music\\Silo"],
  stats: () => (empty ? { month: { hours: 0, artists: [], tracks: [] }, all: { hours: 0, artists: [], tracks: [] } } : { month: stats, all: { ...stats, hours: 412 } }),
  echo_mix: () => ({ tracks: list(30, 11), missing: [], errors: [] }),
  search_all: (a) => new Promise((r) => setTimeout(() => r({ tracks: a.params.query === "nothing" ? [] : list(20, 5), errors: a.params.query === "err" ? ["YouTube: дневной лимит поисков исчерпан"] : [] }), 600)),
  search_playlists: () => ({ playlists: list(8).map((t, i) => ({ id: "p" + i, source: t.source, title: "Плейлист " + t.name, owner: t.artist, image: t.image, count: 20 + i })), errors: [] }),
  open_playlist: () => list(25, 2),
  artist_tracks: () => [list(15, 7), true],
  lyrics: () => ["[00:10.00]Первая строка песни\n[00:20.00]Вторая строка\n[00:40.00]Третья строка, подлиннее остальных\n[00:50.00]Четвёртая", ""],
  suggest_artists: () => ARTISTS.map((name) => ({ name, tags: [], source: "musicbrainz", image: "", note: "" })),
  suggest_tracks: (a) => list(6, a.query.length).map((x) => ({ ...x, source: "deezer" })),
  yt_usage: () => [37, 100],
  resolve_track: (a) => ({ ...a.track, source: "youtube", id: "youtube:" + a.track.id }),
  "plugin:event|listen": () => 1,
  "plugin:window|is_maximized": () => false,
};
(window as any).__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  transformCallback: () => Math.floor(Math.random() * 1e9),
  convertFileSrc: (p: string) => p,
  invoke: async (cmd: string, args: any) => { const h = handlers[cmd]; if (!h) { if (!cmd.startsWith("plugin:")) console.debug("[mock] нет команды", cmd); return null; } return h(args ?? {}); },
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
export {};
