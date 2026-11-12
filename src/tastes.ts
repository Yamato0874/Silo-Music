/** Вкусы: тонкий слой над таксономией (taxonomy/taxonomy.json). Узлы: семейства, жанры, поджанры, настроения, контексты, эпохи, языки. */
import data from "../taxonomy/taxonomy.json";

export type NodeKind = "family" | "genre" | "mood" | "context" | "era" | "lang" | "group";
export interface TNode { id: string; kind: NodeKind; group?: string; parent: string | null; ru: string; en: string; eq: number[] | null; tags: string[]; alias: string[]; near: string[]; lang: string | null }
export interface SeedArtist { name: string; tags: string[] }

export const NODES = (data as { nodes: TNode[] }).nodes;
const BY_ID = new Map(NODES.map((n) => [n.id, n]));
export const FAMILIES = NODES.filter((n) => n.kind === "family");
export const ofKind = (k: NodeKind) => NODES.filter((n) => n.kind === k);
export const nodeById = (id: string) => BY_ID.get(id);
export const nodeName = (n: { ru: string; en: string }, lang: "ru" | "en") => (lang === "en" ? n.en : n.ru);
export const childrenOf = (id: string) => NODES.filter((n) => n.parent === id);
export function ancestors(id: string): string[] {
  const out: string[] = [];
  let p = BY_ID.get(id)?.parent;
  while (p && out.length < 8) { out.push(p); p = BY_ID.get(p)?.parent; }
  return out;
}
/** Все потомки узла в порядке файла (для раскрытия семейства). */
export const descendants = (id: string) => NODES.filter((n) => ancestors(n.id).includes(id));
/** Сколько выбранных узлов лежит внутри семейства. */
export const selectedIn = (id: string, sel: string[]) => sel.filter((s) => ancestors(s).includes(id)).length;

const norm = (s: string) => s.toLowerCase().replace(/ё/g, "е").replace(/[^\p{L}\p{N}]+/gu, "");
/** Поиск по названию, синониму и тегу на обоих языках. Сначала совпадения с начала слова. */
export function searchNodes(q: string, limit = 14): TNode[] {
  const nq = norm(q);
  if (!nq) return [];
  const scored: [number, TNode][] = [];
  for (const n of NODES) {
    if (n.kind === "group") continue;
    const names = [n.ru, n.en, ...n.alias].map(norm);
    let s = -1;
    if (names.some((x) => x === nq)) s = 0;
    else if (names.some((x) => x.startsWith(nq))) s = 1;
    else if (n.tags.some((x) => norm(x) === nq)) s = 2;
    else if (names.some((x) => x.includes(nq))) s = 3;
    if (s >= 0) scored.push([s * 10 + (n.kind === "family" ? 0 : n.parent && BY_ID.get(n.parent)?.kind === "family" ? 1 : 2), n]);
  }
  return scored.sort((a, b) => a[0] - b[0]).slice(0, limit).map((x) => x[1]);
}

/** Теги для поиска: свои или ближайшего родителя. */
export function queryTags(id: string): string[] {
  for (const x of [id, ...ancestors(id)]) { const n = BY_ID.get(x); if (n?.tags.length) return n.tags.slice(0, 2); }
  return [];
}
/** Эквалайзер узла: свой профиль или ближайшего родителя (наследование от семейства). */
export function eqOf(id: string): number[] | null {
  for (const x of [id, ...ancestors(id)]) { const e = BY_ID.get(x)?.eq; if (e) return e; }
  return null;
}

/** Старые id первого запуска → узлы таксономии. */
const LEGACY: Record<string, string[]> = {
  ru_rap: ["russian_rap"], ru_rock: ["russian_rock"], us_rap: ["us_rap"], pop: ["pop"], rnb: ["rnb"], electronic: ["electronic"], rock: ["rock"], metal: ["metal"],
  indie: ["indie"], jazz: ["jazz"], classical: ["classical"], lofi: ["lofi_beats"], folk: ["folk"],
};
export const migrateTastes = (ids: string[]) => [...new Set(ids.flatMap((i) => (BY_ID.has(i) ? [i] : LEGACY[i] ?? [])))];

/** Совместимость: узел вкуса с тегами для запроса и языком. */
export function tasteById(id: string) {
  const n = BY_ID.get(id);
  if (!n) return undefined;
  const lang = n.kind === "lang" ? n.lang ?? undefined : n.lang ?? undefined;
  return { id, ru: n.ru, en: n.en, tags: queryTags(id), lang };
}

/** Эквалайзер под выбранное: среднее профилей жанров (настроения, эпохи и языки профиля не имеют). */
export function eqFor(ids: string[]): number[] {
  const es = ids.map(eqOf).filter(Boolean) as number[][];
  if (!es.length) return Array(10).fill(0);
  return Array.from({ length: 10 }, (_, i) => Math.round(es.reduce((s, e) => s + e[i], 0) / es.length));
}

/** Стартовые узлы «Эхо», язык (если выбран один) и любимые исполнители. */
export function seedFor(ids: string[], artists: SeedArtist[] = []): { tags: string[]; lang?: string; artists: SeedArtist[] } {
  const ns = ids.map((i) => BY_ID.get(i)).filter(Boolean) as TNode[];
  const tags = ns.filter((n) => n.kind !== "lang" && n.kind !== "era").map((n) => n.id);
  const langs = [...new Set(ns.map((n) => n.lang).filter(Boolean))] as string[];
  return { tags, lang: langs.length === 1 ? langs[0] : undefined, artists };
}
