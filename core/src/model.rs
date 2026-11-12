use serde::{Deserialize, Serialize};

fn d_jamendo() -> String { "jamendo".into() }
fn d_music() -> String { "music".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// "<источник>:<id>", например "youtube:dQw4w9WgXcQ", "jamendo:123", "local:jamendo:123"
    pub id: String,
    /// jamendo | youtube | soundcloud | local
    #[serde(default = "d_jamendo")]
    pub source: String,
    /// music сейчас; позже video для скачанных фильмов
    #[serde(default = "d_music")]
    pub kind: String,
    /// ссылка на страницу трека (YouTube/SoundCloud) — по ней играет встроенный плеер
    #[serde(default)]
    pub url: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub image: String,
    pub audio: String,
    pub download_url: String,
    pub download_allowed: bool,
    pub duration: u32,
    pub tags: Vec<String>,
    pub license_url: String,
    /// число прослушиваний/просмотров на площадке (0 = неизвестно); нужно только для оценки «качественности» относительно жанра
    #[serde(default)]
    pub pop: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub query: Option<String>,
    /// жанры/теги Jamendo, например "rap", "rock"
    pub tags: Vec<String>,
    /// язык текста, ISO 639-1: "ru", "en"
    pub lang: Option<String>,
    pub order: Option<String>,
    pub offset: u32,
    pub limit: u32,
    /// какие источники опрашивать в едином поиске; пусто = все
    #[serde(default)]
    pub sources: Vec<String>,
    /// качество потока Jamendo: mp31 (низкое), mp32 (среднее), flac (высокое)
    #[serde(default)]
    pub audio_format: Option<String>,
    /// строгий языковой фильтр: при заданном `lang` отбрасывать треки другого языка во всех источниках (по умолчанию включён)
    #[serde(default)]
    pub lang_strict: Option<bool>,
}

impl Track {
    /// Пустая заготовка для старых записей, где сохранились только теги.
    pub fn stub(id: &str, tags: Vec<String>) -> Track {
        Track { id: id.into(), source: "jamendo".into(), kind: "music".into(), url: String::new(), name: String::new(), artist: String::new(),
            album: String::new(), image: String::new(), audio: String::new(), download_url: String::new(), download_allowed: false,
            duration: 0, tags, license_url: String::new(), pop: 0.0 }
    }
}

/// Ответ единого поиска: треки + ошибки отдельных источников (один упавший источник не ломает остальные).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchAll {
    pub tracks: Vec<Track>,
    pub errors: Vec<String>,
}

/// Готовый плейлист из внешнего сервиса (до открытия: без треков).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRef {
    /// "<источник>:<id>"
    pub id: String,
    pub source: String,
    pub title: String,
    pub owner: String,
    pub image: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaylistHits {
    pub playlists: Vec<PlaylistRef>,
    pub errors: Vec<String>,
}
