//! Вкусы поверх таксономии ядра (то же, что src/tastes.ts).
use silo_core::taxonomy::{self, Node};

pub fn tax() -> &'static taxonomy::Taxonomy { taxonomy::get() }
pub fn node(id: &str) -> Option<&'static Node> { tax().node(id) }
pub fn name(n: &Node, en: bool) -> &str { if en { &n.en } else { &n.ru } }
pub fn families() -> Vec<&'static Node> { tax().nodes().iter().filter(|n| n.kind == "family").collect() }
pub fn of_kind(k: &str) -> Vec<&'static Node> { tax().nodes().iter().filter(|n| n.kind == k).collect() }
pub fn descendants(id: &str) -> Vec<&'static Node> { tax().nodes().iter().filter(|n| tax().ancestors(&n.id).contains(&id)).collect() }
pub fn selected_in(id: &str, sel: &[String]) -> usize { sel.iter().filter(|s| tax().ancestors(s).contains(&id)).count() }
pub fn family_name(id: &str, en: bool) -> String {
    let anc = tax().ancestors(id);
    anc.last().and_then(|r| node(r)).filter(|f| f.kind == "family").map(|f| name(f, en).to_string()).unwrap_or_default()
}

fn norm(s: &str) -> String { s.to_lowercase().replace('ё', "е").chars().filter(|c| c.is_alphanumeric()).collect() }

/// Поиск узлов по названию, синониму и тегу на обоих языках.
pub fn search(q: &str, limit: usize) -> Vec<&'static Node> {
    let nq = norm(q);
    if nq.is_empty() { return vec![]; }
    let mut sc: Vec<(usize, usize, &Node)> = vec![];
    for (i, n) in tax().nodes().iter().enumerate() {
        if n.kind == "group" { continue; }
        let names: Vec<String> = [n.ru.as_str(), n.en.as_str()].into_iter().chain(n.alias.iter().map(|x| x.as_str())).map(norm).collect();
        let s = if names.iter().any(|x| *x == nq) { 0 } else if names.iter().any(|x| x.starts_with(&nq)) { 1 } else if n.tags.iter().any(|x| norm(x) == nq) { 2 } else if names.iter().any(|x| x.contains(&nq)) { 3 } else { continue };
        let depth = if n.kind == "family" { 0 } else if n.parent.as_deref().and_then(node).is_some_and(|p| p.kind == "family") { 1 } else { 2 };
        sc.push((s * 10 + depth, i, n));
    }
    sc.sort_by_key(|x| (x.0, x.1));
    sc.into_iter().take(limit).map(|x| x.2).collect()
}

pub fn eq_of(id: &str) -> Option<Vec<i32>> {
    std::iter::once(id).chain(tax().ancestors(id)).find_map(|x| node(x).and_then(|n| n.eq.clone()))
}
pub fn eq_for(ids: &[String]) -> Vec<f32> {
    let es: Vec<Vec<i32>> = ids.iter().filter_map(|i| eq_of(i)).collect();
    if es.is_empty() { return vec![0.0; 10]; }
    (0..10).map(|i| (es.iter().map(|e| e[i] as f32).sum::<f32>() / es.len() as f32).round()).collect()
}
pub fn query_tags(id: &str) -> Vec<String> {
    for x in std::iter::once(id).chain(tax().ancestors(id)) { if let Some(n) = node(x) { if !n.tags.is_empty() { return n.tags.iter().take(2).cloned().collect(); } } }
    vec![]
}
pub struct Seed { pub tags: Vec<String>, pub lang: Option<String>, pub artists: Vec<silo_core::echo::SeedArtist> }
/// Стартовые узлы «Эхо», язык (если выбран один) и любимые исполнители.
pub fn seed_for(ids: &[String], artists: &[silo_core::echo::SeedArtist]) -> Seed {
    let ns: Vec<&Node> = ids.iter().filter_map(|i| node(i)).collect();
    let tags = ns.iter().filter(|n| n.kind != "lang" && n.kind != "era").map(|n| n.id.clone()).collect();
    let mut langs: Vec<String> = ns.iter().filter_map(|n| n.lang.clone()).collect();
    langs.dedup();
    langs.sort(); langs.dedup();
    Seed { tags, lang: if langs.len() == 1 { Some(langs[0].clone()) } else { None }, artists: artists.to_vec() }
}
pub fn taste_name(id: &str, en: bool) -> Option<String> { node(id).map(|n| name(n, en).to_string()) }
pub fn taste_lang(id: &str) -> Option<String> { node(id).and_then(|n| n.lang.clone()) }
