import { useEffect, useMemo, useState } from "react";
import { api, ArtistHit, ArtistSeed } from "./api";
import { setLang, t } from "./i18n";
import { Prefs } from "./prefs";
import { FAMILIES, TNode, ancestors, descendants, eqFor, nodeById, nodeName, ofKind, searchNodes, selectedIn } from "./tastes";
import { Btn, Icon, IconName } from "./ui";
import Keys from "./Keys";

type L = Prefs["lang"];
const toggleIn = (v: string[], id: string) => (v.includes(id) ? v.filter((x) => x !== id) : [...v, id]);

/** Выбор вкусов: крупные плитки семейств, раскрытие поджанров, поиск по всем узлам (жанры, настроения, эпохи, языки). Сотни узлов не выводятся стеной. */
export function TastePicker({ value, onChange, lang }: { value: string[]; onChange: (v: string[]) => void; lang: L }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const hits = useMemo(() => searchNodes(q), [q]);
  const chip = (n: TNode, key = n.id) => (
    <Btn key={key} variant="chip" size="sm" active={value.includes(n.id)} onClick={() => onChange(toggleIn(value, n.id))}>{nodeName(n, lang)}</Btn>
  );
  const subs = open ? descendants(open) : [];
  const fam = (n: TNode) => { const anc = ancestors(n.id); const r = anc[anc.length - 1]; const f = r ? nodeById(r) : undefined; return f && f.kind === "family" ? nodeName(f, lang) : ""; };
  return (
    <div className="picker">
      <input className="pk-search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.genresSearch} aria-label={t.genresSearch} />
      {value.length > 0 && (
        <div className="chips picked" aria-label={t.pickedN}>
          {value.map((id) => { const n = nodeById(id); return n && (
            <Btn key={id} variant="chip" size="sm" active onClick={() => onChange(value.filter((x) => x !== id))} label={t.remove}>{nodeName(n, lang)}</Btn>
          ); })}
        </div>
      )}
      {q.trim() ? (
        <div className="chips">
          {hits.map((n) => <span key={n.id} className="hit">{chip(n)}{fam(n) && <small>{fam(n)}</small>}</span>)}
          {!hits.length && <span className="muted">{t.noResults}</span>}
        </div>
      ) : (
        <>
          <div className="tiles fam">
            {FAMILIES.map((f) => {
              const on = value.includes(f.id), cnt = selectedIn(f.id, value);
              return (
                <div key={f.id} className={"tilex" + (on ? " on" : cnt ? " part" : "") + (open === f.id ? " open" : "")}>
                  <button className="tile" aria-pressed={on} onClick={() => onChange(toggleIn(value, f.id))}>
                    {nodeName(f, lang)}{cnt > 0 && <em>{cnt}</em>}
                  </button>
                  <button className="tile-open" aria-expanded={open === f.id} aria-label={`${t.openFamily}: ${nodeName(f, lang)}`} title={t.subgenres}
                    onClick={() => setOpen(open === f.id ? null : f.id)}><Icon name="back" size={16} /></button>
                </div>
              );
            })}
          </div>
          {open && (
            <div className="subs">
              <h4>{t.subgenres}: {nodeName(nodeById(open)!, lang)}</h4>
              <div className="chips">{subs.map((n) => chip(n))}</div>
            </div>
          )}
          <Btn size="sm" onClick={() => setMore(!more)}>{more ? t.less : t.moreGroups}</Btn>
          {more && (
            <div className="groups">
              {([["mood", t.moodsTitle], ["context", t.contextTitle], ["era", t.eraTitle], ["lang", t.langTitle]] as const).map(([k, title]) => (
                <div key={k}><h4>{title}</h4><div className="chips">{ofKind(k).map((n) => chip(n))}</div></div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

const srcLabel = (s: string, lang: L) => (s === "library" ? (lang === "en" ? "Your library" : "Ваша библиотека") : s === "musicbrainz" ? "MusicBrainz" : s === "jamendo" ? "Jamendo" : s === "soundcloud" ? "SoundCloud" : s);

/** Выбор 5–10 любимых исполнителей: поиск с подсказками (библиотека, Jamendo, SoundCloud, MusicBrainz), выбранные — чипами. Неизвестного артиста можно добавить как есть. */
export function ArtistPicker({ value, onChange, lang }: { value: ArtistSeed[]; onChange: (v: ArtistSeed[]) => void; lang: L }) {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<ArtistHit[]>([]);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const s = q.trim();
    if (s.length < 2) { setHits([]); setBusy(false); return; }
    let live = true;
    setBusy(true);
    const h = setTimeout(() => {
      api.suggestArtists(s).then((r) => live && setHits(r)).catch(() => live && setHits([])).finally(() => live && setBusy(false));
    }, 350);
    return () => { live = false; clearTimeout(h); };
  }, [q]);
  const has = (n: string) => value.some((a) => a.name.toLowerCase() === n.toLowerCase());
  const full = value.length >= 10;
  const add = (a: ArtistSeed) => { if (!has(a.name) && !full) { onChange([...value, { name: a.name, tags: a.tags }]); setQ(""); setHits([]); } };
  const typed = q.trim();
  const tagNames = (ids: string[]) => ids.map((i) => nodeById(i)).filter(Boolean).slice(0, 3).map((n) => nodeName(n!, lang)).join(", ");
  return (
    <div className="picker">
      <input className="pk-search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t.artistsSearch} aria-label={t.artistsSearch} disabled={full} />
      {typed.length >= 2 && !full && (
        <div className="hits" role="listbox">
          {hits.filter((h) => !has(h.name)).map((h) => (
            <button key={h.source + h.name} className="hit-row" role="option" onClick={() => add(h)}>
              <b>{h.name}</b>
              <small>{[srcLabel(h.source, lang), h.note, tagNames(h.tags)].filter(Boolean).join(" · ")}</small>
            </button>
          ))}
          {busy && <div className="muted hit-note">{t.artistLoading}</div>}
          {!busy && !has(typed) && !hits.some((h) => h.name.toLowerCase() === typed.toLowerCase()) && (
            <button className="hit-row" onClick={() => add({ name: typed, tags: [] })}>
              <b>{t.artistAdd}: {typed}</b><small>{t.artistNoFree}</small>
            </button>
          )}
          {!busy && !hits.length && <div className="muted hit-note">{t.noResults}</div>}
        </div>
      )}
      <div className="chips picked">
        {value.map((a) => (
          <Btn key={a.name} variant="chip" size="sm" active onClick={() => onChange(value.filter((x) => x.name !== a.name))} label={t.remove}>{a.name}</Btn>
        ))}
      </div>
      <p className="muted pk-count">{t.artistsCount}: {value.length} / 10{value.length < 5 ? ` · ${t.artistsMin}` : full ? ` · ${t.artistsMax}` : ""}</p>
    </div>
  );
}

const TIPS: { t: "tip1t" | "tip2t" | "tip3t" | "tip4t"; b: "tip1" | "tip2" | "tip3" | "tip4"; icons?: IconName[] }[] = [
  { t: "tip1t", b: "tip1", icons: ["like", "dislike", "ban"] },
  { t: "tip2t", b: "tip2" },
  { t: "tip3t", b: "tip3" },
  { t: "tip4t", b: "tip4" },
];

/** Первый запуск: язык, жанры, затем короткое знакомство с функциями (карточки). Показывается один раз. */
export default function Onboarding({ prefs, done, startStep = 0 }: { prefs: Prefs; done: (p: Partial<Prefs>) => void; startStep?: number }) {
  const redo = startStep === 1; // после «Сбросить вкусы» знакомство повторно не показываем
  const [keysStep, setKeysStep] = useState(!redo); // первый экран знакомства: ключи API (можно пропустить)
  const [step, setStep] = useState(startStep);
  const [tip, setTip] = useState(0);
  const [lang, setL] = useState<Prefs["lang"]>(prefs.lang);
  const [tastes, setTastes] = useState<string[]>([]);
  const [artists, setArtists] = useState<ArtistSeed[]>([]);
  setLang(lang);
  const finish = (ts: string[], ar: ArtistSeed[] = artists) => done({ lang, tastes: ts, artists: ar, eq: eqFor(ts), eqOn: ts.length > 0, onboarded: true });
  const afterArtists = (ar: ArtistSeed[]) => { setArtists(ar); if (redo) finish(tastes, ar); else setStep(3); };
  return (
    <div className="onb">
      <div className="onb-card">
        <div className="dots">{Array.from({ length: redo ? 3 : 4 + TIPS.length }, (_, i) => <i key={i} className={i === (redo ? step : keysStep ? 0 : 1 + (step < 3 ? step : 3 + tip)) ? "on" : ""} />)}</div>
        {keysStep && (
          <>
            <h2>{lang === "en" ? "Connect music services" : "Подключите музыкальные сервисы"}</h2>
            <div className="chips" style={{ marginBottom: 8 }}>
              <Btn variant="chip" size="sm" active={lang === "ru"} onClick={() => setL("ru")}>Русский</Btn>
              <Btn variant="chip" size="sm" active={lang === "en"} onClick={() => setL("en")}>English</Btn>
            </div>
            <p className="muted">{lang === "en"
              ? "Open catalogs (ccMixter, Internet Archive) work right away, no keys. YouTube and SoundCloud play by pasted link without keys; for search inside them add a free key. Everything plays inside the app. You can change this later in Settings."
              : "Открытые каталоги (ccMixter, Internet Archive) работают сразу, без ключей. YouTube и SoundCloud играют по вставленной ссылке без ключей; для поиска внутри них добавьте бесплатный ключ. Вся музыка играет внутри приложения. Позже всё можно изменить в Настройках."}</p>
            <Keys lang={lang} saveLabel={lang === "en" ? "Save" : "Сохранить"} />
            <div className="onb-foot">
              <Btn onClick={() => setKeysStep(false)}>{t.skip}</Btn>
              <Btn variant="primary" onClick={() => setKeysStep(false)}>{t.continue}</Btn>
            </div>
          </>
        )}
        {!keysStep && step === 0 && (
          <>
            <h2>Выберите язык · Choose language</h2>
            <div className="tiles two">
              <button className={"tile big" + (lang === "ru" ? " on" : "")} onClick={() => setL("ru")}>Русский</button>
              <button className={"tile big" + (lang === "en" ? " on" : "")} onClick={() => setL("en")}>English</button>
            </div>
            <div className="onb-foot"><span /><Btn variant="primary" onClick={() => setStep(1)}>{t.continue}</Btn></div>
          </>
        )}
        {!keysStep && step === 1 && (
          <>
            <h2>{t.tastesQ}</h2>
            <p className="muted">{t.tastesHint}</p>
            <TastePicker value={tastes} onChange={setTastes} lang={lang} />
            <div className="onb-foot">
              <Btn onClick={() => { setTastes([]); setStep(2); }}>{t.skip}</Btn>
              <div className="row" style={{ margin: 0 }}>
                <Btn icon="back" label={t.back} onClick={() => setStep(0)} />
                <Btn variant="primary" disabled={!tastes.length} onClick={() => setStep(2)}>{t.onbNext}</Btn>
              </div>
            </div>
          </>
        )}
        {!keysStep && step === 2 && (
          <>
            <h2>{t.artistsQ}</h2>
            <p className="muted">{t.artistsHint}</p>
            <ArtistPicker value={artists} onChange={setArtists} lang={lang} />
            <div className="onb-foot">
              <Btn onClick={() => afterArtists([])}>{t.skip}</Btn>
              <div className="row" style={{ margin: 0 }}>
                <Btn icon="back" label={t.back} onClick={() => setStep(1)} />
                <Btn variant="primary" disabled={!artists.length} onClick={() => afterArtists(artists)}>{redo ? t.done : t.onbNext}</Btn>
              </div>
            </div>
          </>
        )}
        {!keysStep && step === 3 && (
          <div key={tip} className="tip">
            {TIPS[tip].icons && <div className="tip-icons">{TIPS[tip].icons!.map((n) => <span key={n}><Icon name={n} size={22} /></span>)}</div>}
            <h2>{t[TIPS[tip].t]}</h2>
            <p className="muted">{t[TIPS[tip].b]}</p>
            <div className="onb-foot">
              <Btn onClick={() => finish(tastes)}>{t.skip}</Btn>
              <Btn variant="primary" onClick={() => (tip + 1 < TIPS.length ? setTip(tip + 1) : finish(tastes))}>{tip + 1 < TIPS.length ? t.onbNext : t.done}</Btn>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
