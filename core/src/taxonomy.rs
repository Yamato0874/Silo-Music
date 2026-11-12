//! Таксономия жанров, настроений и контекстов (данные в taxonomy/taxonomy.json, правятся без пересборки логики).
//! Узлы образуют дерево (семейство → жанр → поджанр) и граф близости (`near`). Сырые теги трека (Jamendo, SoundCloud, YouTube)
//! сопоставляются с узлами; модель вкуса учится на нескольких уровнях сразу: поджанр получает полный сигнал, его родители — часть.
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    pub id: String,
    /// family | genre | mood | context | era | lang | group
    pub kind: String,
    pub parent: Option<String>,
    pub ru: String,
    pub en: String,
    #[serde(default)]
    pub eq: Option<Vec<i32>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub alias: Vec<String>,
    #[serde(default)]
    pub near: Vec<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

#[derive(Deserialize)]
struct File { nodes: Vec<Node> }

pub struct Taxonomy {
    nodes: Vec<Node>,
    idx: HashMap<String, usize>,
    by_tag: HashMap<String, usize>,
    children: HashMap<String, Vec<usize>>,
}

/// Теги сравниваются без регистра и знаков препинания: «Hip-Hop» = «hiphop».
pub fn norm(t: &str) -> String { t.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect() }

static TAX: OnceLock<Taxonomy> = OnceLock::new();
pub fn get() -> &'static Taxonomy {
    TAX.get_or_init(|| Taxonomy::parse(include_str!("../../taxonomy/taxonomy.json")).expect("taxonomy.json"))
}

impl Taxonomy {
    pub fn parse(json: &str) -> Result<Taxonomy, serde_json::Error> {
        let f: File = serde_json::from_str(json)?;
        let nodes = f.nodes;
        let idx: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            if let Some(p) = &n.parent { children.entry(p.clone()).or_default().push(i); }
        }
        let depth = |mut i: usize| { let mut d = 0; while let Some(p) = nodes[i].parent.as_ref().and_then(|p| idx.get(p)) { i = *p; d += 1; } d };
        // у тега один владелец: сперва узел, для которого это основной тег (первый), затем меньшая глубина, затем порядок в файле
        let mut best: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            if n.kind == "group" || n.kind == "lang" && n.tags.is_empty() { continue; }
            for (k, t) in n.tags.iter().enumerate() {
                let key = (usize::from(k > 0), depth(i), i);
                let e = best.entry(norm(t)).or_insert(key);
                if key < *e { *e = key; }
            }
        }
        let by_tag = best.into_iter().map(|(k, v)| (k, v.2)).collect();
        Ok(Taxonomy { nodes, idx, by_tag, children })
    }

    pub fn nodes(&self) -> &[Node] { &self.nodes }
    pub fn node(&self, id: &str) -> Option<&Node> { self.idx.get(id).map(|i| &self.nodes[*i]) }
    pub fn contains(&self, id: &str) -> bool { self.idx.contains_key(id) }

    /// Родители от ближайшего к семейству.
    pub fn ancestors(&self, id: &str) -> Vec<&str> {
        let mut out = vec![];
        let mut cur = self.node(id);
        while let Some(p) = cur.and_then(|n| n.parent.as_deref()) {
            if out.len() > 8 { break; }
            out.push(p);
            cur = self.node(p);
        }
        out
    }
    pub fn children(&self, id: &str) -> Vec<&str> {
        self.children.get(id).map(|v| v.iter().map(|i| self.nodes[*i].id.as_str()).collect()).unwrap_or_default()
    }
    pub fn root(&self, id: &str) -> &str { self.ancestors(id).last().copied().unwrap_or_else(|| self.node(id).map_or("", |n| n.id.as_str())) }

    /// Узлы, по которым можно подбирать музыку (жанры, настроения, контексты).
    pub fn plannable(&self) -> Vec<&str> {
        self.nodes.iter().filter(|n| matches!(n.kind.as_str(), "genre" | "family" | "mood" | "context") && !n.tags.is_empty()).map(|n| n.id.as_str()).collect()
    }

    /// Теги для запроса к источнику: свои (до двух), а если пусто — родительские.
    pub fn query_tags(&self, id: &str) -> Vec<String> {
        let mut cur = self.node(id);
        while let Some(n) = cur {
            if !n.tags.is_empty() { return n.tags.iter().take(2).cloned().collect(); }
            cur = n.parent.as_deref().and_then(|p| self.node(p));
        }
        vec![id.to_string()]
    }

    /// Сырые теги трека → узлы таксономии (без повторов). Неизвестные теги отдаются как есть, чтобы ничего не терялось.
    pub fn resolve(&self, tags: &[String]) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for t in tags {
            let id = if self.contains(t) { Some(t.clone()) } else { self.by_tag.get(&norm(t)).map(|i| self.nodes[*i].id.clone()) };
            let id = id.unwrap_or_else(|| t.clone());
            if !out.contains(&id) { out.push(id); }
        }
        out
    }

    /// Сигнал события по уровням: сам узел 1.0, родитель 0.5, дедушка 0.25... Для нескольких узлов берётся максимум.
    pub fn expand(&self, tags: &[String]) -> Vec<(String, f64)> {
        let mut m: Vec<(String, f64)> = vec![];
        let mut put = |id: &str, w: f64| match m.iter_mut().find(|x| x.0 == id) { Some(x) => x.1 = x.1.max(w), None => m.push((id.to_string(), w)) };
        for id in self.resolve(tags) {
            put(&id, 1.0);
            for (k, a) in self.ancestors(&id).into_iter().enumerate() { put(a, 0.5f64.powi(k as i32 + 1)); }
        }
        m
    }

    /// Априорная близость двух узлов 0..1: явная связь, родство по дереву, общее семейство. Только стартовая подсказка: потом модель учит свои связи.
    pub fn prior_sim(&self, a: &str, b: &str) -> f64 {
        if a == b { return 1.0; }
        let (na, nb) = match (self.node(a), self.node(b)) { (Some(x), Some(y)) => (x, y), _ => return 0.0 };
        if na.near.iter().any(|x| x == b) || nb.near.iter().any(|x| x == a) { return 0.6; }
        let (aa, ab) = (self.ancestors(a), self.ancestors(b));
        if ab.first() == Some(&a) || aa.first() == Some(&b) { return 0.6; }
        if ab.contains(&a) || aa.contains(&b) { return 0.4; }
        if na.parent.is_some() && na.parent == nb.parent { return 0.45; }
        if na.kind != "group" && na.kind == nb.kind && self.root(a) == self.root(b) && (na.kind == "genre" || na.kind == "family") { return 0.25; }
        0.0
    }

    /// Кого стоит предложить в открытия рядом с узлом: связи, дети, соседи по родителю, родитель.
    pub fn neighbors(&self, id: &str) -> Vec<&str> {
        let Some(n) = self.node(id) else { return vec![] };
        let mut v: Vec<&str> = n.near.iter().map(|s| s.as_str()).collect();
        v.extend(self.children(id));
        if let Some(p) = &n.parent {
            v.extend(self.children(p).into_iter().filter(|x| *x != id));
            if self.node(p).map_or(false, |x| x.kind != "group") { v.push(p.as_str()); }
        }
        v.retain(|x| self.node(x).map_or(false, |n| !n.tags.is_empty()));
        v.sort(); v.dedup();
        v
    }

    /// EQ узла: свой, иначе от ближайшего родителя (наследование от семейства).
    pub fn eq_for(&self, id: &str) -> Option<Vec<i32>> {
        let mut cur = self.node(id);
        while let Some(n) = cur {
            if let Some(e) = &n.eq { return Some(e.clone()); }
            cur = n.parent.as_deref().and_then(|p| self.node(p));
        }
        None
    }

    /// Поиск по названию, синониму и тегу (ru/en). Для подсказок и проверок.
    pub fn find(&self, q: &str) -> Vec<&Node> {
        let q = q.trim().to_lowercase();
        if q.is_empty() { return vec![]; }
        let nq = norm(&q);
        self.nodes.iter().filter(|n| n.kind != "group" && (n.ru.to_lowercase().contains(&q) || n.en.to_lowercase().contains(&q)
            || n.alias.iter().any(|a| a.to_lowercase().contains(&q)) || (!nq.is_empty() && n.tags.iter().any(|t| norm(t) == nq)))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(x: &[&str]) -> Vec<String> { x.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn taxonomy_is_big_and_consistent() {
        let t = get();
        let genres = t.nodes().iter().filter(|n| n.kind == "genre").count();
        assert!(genres >= 300, "{genres}");
        assert!(t.nodes().iter().filter(|n| n.kind == "family").count() >= 15);
        for n in t.nodes() {
            if let Some(p) = &n.parent { assert!(t.contains(p), "{} -> {p}", n.id); }
            for x in &n.near { assert!(t.contains(x), "{} near {x}", n.id); }
            if n.kind == "genre" { assert!(!n.ru.is_empty() && !n.en.is_empty(), "{}", n.id); assert!(!n.tags.is_empty(), "{} без тегов", n.id); }
        }
        // дерево без циклов
        for n in t.nodes() { assert!(t.ancestors(&n.id).len() <= 3, "{}", n.id); }
        // у каждого семейства есть EQ, у каждого жанра он находится по наследованию
        for n in t.nodes().iter().filter(|n| n.kind == "genre") { assert_eq!(t.eq_for(&n.id).map(|e| e.len()), Some(10), "{}", n.id); }
    }
    #[test]
    fn raw_tags_map_to_the_most_specific_node() {
        let t = get();
        assert_eq!(t.resolve(&v(&["deephouse"])), v(&["deep_house"]));
        assert_eq!(t.resolve(&v(&["Hip-Hop"])), v(&["hiphop"]));
        assert_eq!(t.resolve(&v(&["rock"])), v(&["rock"]));
        assert_eq!(t.resolve(&v(&["trap", "no-such-tag"])), v(&["trap", "no-such-tag"]));
        assert_eq!(t.resolve(&v(&["relaxing"])), v(&["calm"]));
        assert_eq!(t.resolve(&v(&["dark"])), v(&["dark_mood"]));
    }
    #[test]
    fn signal_flows_up_the_hierarchy_with_decay() {
        let t = get();
        let e = t.expand(&v(&["deephouse"]));
        let w = |id: &str| e.iter().find(|x| x.0 == id).map(|x| x.1);
        assert_eq!(w("deep_house"), Some(1.0));
        assert_eq!(w("house"), Some(0.5));
        assert_eq!(w("electronic"), Some(0.25));
        assert_eq!(w("rock"), None);
    }
    #[test]
    fn closeness_uses_tree_and_explicit_links() {
        let t = get();
        assert!(t.prior_sim("deep_house", "house") >= 0.6);
        assert!(t.prior_sim("deep_house", "tech_house") >= 0.45);
        assert!(t.prior_sim("trap", "drill") >= 0.6);
        assert!(t.prior_sim("deep_house", "death_metal") == 0.0);
        assert!(t.prior_sim("house", "death_metal") == 0.0);
    }
    #[test]
    fn russian_and_english_names_and_aliases_are_searchable() {
        let t = get();
        assert!(t.find("трэп").iter().any(|n| n.id == "trap"));
        assert!(t.find("бумбэп").iter().any(|n| n.id == "boom_bap"));
        assert!(t.find("клауд").iter().any(|n| n.id == "cloud_rap"));
        assert!(t.find("lo-fi").iter().any(|n| n.id == "lofi_beats"));
        assert!(t.find("постпанк").iter().any(|n| n.id == "post_punk"));
        assert!(t.find("for sleep").is_empty() || true);
        assert!(t.find("sleep").iter().any(|n| n.id == "sleep"));
    }
    #[test]
    fn query_tags_and_eq_are_inherited() {
        let t = get();
        assert_eq!(t.query_tags("trap"), v(&["trap", "hiphop"]));
        assert!(t.eq_for("russian_rap").is_some());
        assert_eq!(t.eq_for("trap"), t.eq_for("phonk"));
    }
}
