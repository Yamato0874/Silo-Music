/// Один общий HTTP-клиент на всё приложение: пул соединений и TLS переиспользуются, у запросов есть таймауты.
pub fn http() -> reqwest::Client {
    static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    C.get_or_init(|| reqwest::Client::builder()
        .user_agent("SiloPlayer/1.0 (open-source desktop music player)")
        .connect_timeout(std::time::Duration::from_secs(8))
        .read_timeout(std::time::Duration::from_secs(15))
        .build().expect("http client")).clone()
}
pub mod db;
pub mod jamendo;
pub mod model;
pub mod soundcloud;
pub mod echo;
pub mod taxonomy;
pub mod artists;
pub mod youtube;
pub mod opencat;
pub mod lbradio;
pub mod lrclib;
pub mod langfilter;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Msg(String),
}
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Msg(s.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;
