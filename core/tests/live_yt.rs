// Ручная проверка на реальном ключе из базы приложения: cargo test --test live_yt -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn chart_and_items() {
    let p = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.silo.player").join("silo.db");
    let key = silo_core::db::Db::open(&p).unwrap().get_setting("youtube_api_key").unwrap().unwrap_or_default();
    println!("key set: {}", !key.is_empty());
    for r in ["RU", "US"] {
        match silo_core::youtube::music_chart(&key, r).await { Ok(v) => println!("chart {r}: {} (durations ok: {})", v.len(), v.iter().filter(|t| (60..=720).contains(&t.duration)).count()), Err(e) => println!("chart {r} err: {e}") }
    }
}
