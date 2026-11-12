//! Открытые каталоги без ключей: ccMixter и Internet Archive (только Creative Commons). Звук играет прямо в приложении.
use crate::{model::*, Result};
use serde_json::Value;

fn s(v: &Value) -> String { v.as_str().unwrap_or("").trim().to_string() }

fn track(id: String, src: &str, url: String, name: String, artist: String, audio: String, lic: String, tags: Vec<String>) -> Track {
    Track {
        id, source: src.into(), kind: "music".into(), url, name, artist, album: String::new(), image: String::new(),
        download_url: audio.clone(), audio, download_allowed: true, duration: 0, tags, license_url: lic, pop: 0.0,
    }
}

pub fn parse_ccmixter(body: &str) -> Result<Vec<Track>> {
    let v: Value = serde_json::from_str(body)?;
    Ok(v.as_array().map(|a| a.iter().filter_map(|x| {
        let f = x["files"].as_array()?.iter().find(|f| s(&f["download_url"]).ends_with(".mp3"))?;
        let tags = s(&x["upload_extra"]["usertags"]).split(',').filter(|t| !t.is_empty()).map(String::from).collect();
        Some(track(format!("ccmixter:{}", x["upload_id"]), "ccmixter", s(&x["file_page_url"]), s(&x["upload_name"]), s(&x["user_name"]),
            s(&f["download_url"]), s(&x["license_url"]), tags))
    }).collect()).unwrap_or_default())
}

pub async fn ccmixter(q: &str, limit: u32) -> Result<Vec<Track>> {
    let body = crate::http().get("https://ccmixter.org/api/query")
        .query(&[("f", "json"), ("search", q), ("search_type", "match"), ("sort", "rank"), ("limit", &limit.to_string())]).send().await?.text().await?;
    parse_ccmixter(&body)
}

pub async fn archive(q: &str, limit: u32) -> Result<Vec<Track>> {
    let c = crate::http();
    let query = format!("({q}) AND mediatype:audio AND licenseurl:*creativecommons*");
    let v: Value = c.get("https://archive.org/advancedsearch.php")
        .query(&[("q", query.as_str()), ("fl[]", "identifier"), ("fl[]", "title"), ("fl[]", "creator"), ("fl[]", "licenseurl"),
            ("rows", &limit.to_string()), ("output", "json")]).send().await?.json().await?;
    let docs = v["response"]["docs"].as_array().cloned().unwrap_or_default();
    let mut files = Vec::with_capacity(docs.len());
    for d in &docs {
        let id = s(&d["identifier"]);
        let f: Option<Value> = match c.get(format!("https://archive.org/metadata/{id}/files")).send().await { Ok(r) => r.json().await.ok(), Err(_) => None };
        let name = f.and_then(|f| f["result"].as_array().and_then(|a| a.iter().map(|x| s(&x["name"])).find(|n| n.to_lowercase().ends_with(".mp3"))));
        files.push(name.map(|n| (id, n)));
    }
    Ok(docs.iter().zip(files).filter_map(|(d, f)| {
        let (id, file) = f?;
        let creator = match &d["creator"] { Value::Array(a) => a.first().map(s).unwrap_or_default(), x => s(x) };
        Some(track(format!("archive:{id}"), "archive", format!("https://archive.org/details/{id}"), s(&d["title"]), creator,
            format!("https://archive.org/download/{id}/{}", file.split('/').map(|p| p.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>()).collect::<Vec<_>>().join("/")), s(&d["licenseurl"]), vec![]))
    }).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_ccmixter() {
        let j = r#"[{"upload_id":5,"upload_name":"R","user_name":"U","file_page_url":"https://ccmixter.org/files/U/5","license_url":"http://cc/by",
          "upload_extra":{"usertags":"piano,chill"},"files":[{"download_url":"https://ccmixter.org/content/U/r.mp3"}]}]"#;
        let t = super::parse_ccmixter(j).unwrap();
        assert_eq!((t[0].id.as_str(), t[0].audio.as_str(), t[0].tags.len()), ("ccmixter:5", "https://ccmixter.org/content/U/r.mp3", 2));
    }
}
#[cfg(test)]
mod live {
    #[tokio::test]
    #[ignore]
    async fn live_search() {
        let a = super::ccmixter("piano", 5).await;
        println!("ccmixter: {:?}", a.as_ref().map(|v| v.len()));
        let b = super::archive("piano", 3).await;
        println!("archive: {:?}", b.as_ref().map(|v| v.iter().map(|t| t.audio.clone()).collect::<Vec<_>>()));
        let c = super::archive("рок", 3).await;
        println!("archive ru: {:?}", c.as_ref().map(|v| v.len()));
    }
}
