import { useState } from "react";
import { api, Track } from "./api";
import { Btn } from "./ui";

/** Браузер бесплатных библиотек (Internet Archive, ccMixter): найти и скачать треки к себе. Скачанное играет из «Скачано» без интернета. */
export default function LibBrowser({ lang }: { lang: "ru" | "en" }) {
  const i = lang === "en";
  const [q, setQ] = useState("");
  const [res, setRes] = useState<Track[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [got, setGot] = useState<Record<string, string>>({});
  const find = () => {
    if (!q.trim()) return;
    setBusy(true); setMsg("");
    api.searchAll({ query: q, tags: [], offset: 0, limit: 20, sources: ["ccmixter", "archive"] })
      .then((r) => { setRes(r.tracks); if (!r.tracks.length) setMsg(i ? "Nothing found" : "Ничего не нашлось"); })
      .catch((e) => setMsg(String(e))).finally(() => setBusy(false));
  };
  const get = (t: Track) => {
    setGot((g) => ({ ...g, [t.id]: "…" }));
    api.download(t).then(() => setGot((g) => ({ ...g, [t.id]: "✓" }))).catch((e) => { setGot((g) => ({ ...g, [t.id]: "" })); setMsg(String(e)); });
  };
  return (
    <section className="key-card">
      <h4>{i ? "Free music libraries" : "Бесплатные библиотеки музыки"}</h4>
      <p className="muted">{i
        ? "Internet Archive and ccMixter: music under open licenses. Find and download tracks; they will play offline from Downloads."
        : "Internet Archive и ccMixter: музыка под открытыми лицензиями. Найдите и скачайте треки, они будут играть без интернета из раздела «Скачано»."}</p>
      <div className="row" style={{ margin: "6px 0" }}>
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={i ? "Genre, artist, album…" : "Жанр, исполнитель, альбом…"}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); find(); } }} />
        <Btn size="sm" variant="solid" onClick={find}>{busy ? "…" : i ? "Find" : "Найти"}</Btn>
      </div>
      {msg && <p className="muted">{msg}</p>}
      {res.map((t) => (
        <div key={t.id} className="row" style={{ justifyContent: "space-between", gap: 8, margin: "2px 0" }}>
          <span><b>{t.name}</b> <span className="muted">{t.artist} · {t.source === "archive" ? "Internet Archive" : "ccMixter"}</span></span>
          <Btn size="sm" disabled={!!got[t.id]} onClick={() => get(t)}>{got[t.id] || (i ? "Download" : "Скачать")}</Btn>
        </div>
      ))}
    </section>
  );
}
