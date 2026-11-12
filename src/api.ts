import { invoke } from "@tauri-apps/api/core";

export interface Track {
  id: string; source: Source; kind: string; url: string; name: string; artist: string; album: string; image: string; audio: string;
  downloadUrl: string; downloadAllowed: boolean; duration: number; tags: string[]; licenseUrl: string;
}
export interface PlaylistRef { id: string; source: Source; title: string; owner: string; image: string; count: number }
export interface PlaylistHits { playlists: PlaylistRef[]; errors: string[] }
export type Source = "jamendo" | "youtube" | "soundcloud" | "local" | "ccmixter" | "archive";
export interface Playlist { id: number; name: string; count: number }
export interface SearchParams { query?: string; tags: string[]; lang?: string; order?: string; offset: number; limit: number; sources?: Source[]; audioFormat?: string; langStrict?: boolean }
export interface EchoOut { tracks: Track[]; missing: string[]; errors: string[] }
export interface SearchAll { tracks: Track[]; errors: string[] }

export const api = {
  getSetting: (key: string) => invoke<string>("get_setting", { key }),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  artistTracks: (artist: string) => invoke<[Track[], boolean]>("artist_tracks", { artist }),
  lyrics: (artist: string, title: string, duration: number) => invoke<[string, string]>("lyrics", { artist, title, duration }),
  renamePlaylist: (id: number, name: string) => invoke<void>("rename_playlist", { id, name }),
  stats: () => invoke<Record<"month" | "all", { hours: number; artists: [string, number][]; tracks: { track: Track; plays: number }[] }>>("stats"),
  linkTrack: (url: string) => invoke<Track>("link_track", { url }),
  searchAll: (params: SearchParams) => invoke<SearchAll>("search_all", { params }),
  rate: (track: Track, value: number) => invoke<void>("rate_track", { track, value }),
  ratings: () => invoke<Record<string, number>>("all_ratings"),
  search: (params: SearchParams) => invoke<Track[]>("search_tracks", { params }),
  /** «Эхо» из всех доступных источников; missing — источники без ключа, errors — сбои источников. */
  echoMixFull: (audioFormat?: string, seed?: { tags: string[]; lang?: string; artists?: ArtistSeed[] }, diversity?: string, langStrict?: boolean) =>
    invoke<EchoOut>("echo_mix", { offset: 0, audioFormat, seedTags: seed?.tags ?? [], seedLang: seed?.lang, diversity, seedArtists: seed?.artists ?? [], langStrict }),
  echoMix: (audioFormat?: string, seed?: { tags: string[]; lang?: string; artists?: ArtistSeed[] }, diversity?: string) =>
    api.echoMixFull(audioFormat, seed, diversity).then((r) => r.tracks),
  suggestArtists: (query: string) => invoke<ArtistHit[]>("suggest_artists", { query }),
  searchPlaylists: (query: string, sources: Source[]) => invoke<PlaylistHits>("search_playlists", { query, sources }),
  openPlaylist: (id: string, lang?: string) => invoke<Track[]>("open_playlist", { id, lang }),
  resolvePlaylist: (url: string) => invoke<string>("resolve_playlist", { url }),
  savePlaylist: (name: string, tracks: Track[]) => invoke<number>("save_playlist", { name, tracks }),
  liked: () => invoke<Track[]>("liked_tracks"),
  banned: () => invoke<Track[]>("banned_tracks"),
  resetTaste: (clearGroups: boolean) => invoke<void>("reset_taste", { clearGroups }),
  history: () => invoke<Track[]>("history"),
  clearHistory: () => invoke<void>("clear_history"),
  downloadsInfo: () => invoke<[number, string]>("downloads_info"),
  /** fraction — доля прослушанного трека, 0..1 */
  recordPlay: (track: Track, fraction: number) => invoke<void>("record_play", { track, fraction }),
  playlists: () => invoke<Playlist[]>("list_playlists"),
  createPlaylist: (name: string) => invoke<number>("create_playlist", { name }),
  deletePlaylist: (id: number) => invoke<void>("delete_playlist", { id }),
  playlistTracks: (id: number) => invoke<Track[]>("playlist_tracks", { id }),
  addToPlaylist: (id: number, track: Track) => invoke<void>("add_to_playlist", { id, track }),
  removeFromPlaylist: (id: number, trackId: string) => invoke<void>("remove_from_playlist", { id, trackId }),
  downloads: () => invoke<[Track, string][]>("list_downloads"),
  download: (track: Track) => invoke<string>("download_track", { track }),
};

/** Любимый исполнитель из знакомства: имя и жанры (id таксономии). */
export interface ArtistSeed { name: string; tags: string[] }
/** Подсказка исполнителя: library | jamendo | soundcloud | musicbrainz. Известных артистов может не быть в свободных источниках — тогда это лишь сигнал вкуса. */
export interface ArtistHit extends ArtistSeed { source: string; image: string; note: string }
