import { FormEvent, useEffect, useState } from "react";
import { api } from "./api";
import { Prefs } from "./prefs";
import { Btn } from "./ui";

type L = Prefs["lang"];
type Field = { key: string; ru: string; en: string; secret: boolean };
type Svc = {
  id: string; name: string; url: string; fields: Field[];
  why: [string, string];
  steps: [string[], string[]];
  note?: [string, string];
};

const SERVICES: Svc[] = [
  {
    id: "jamendo", name: "Jamendo", url: "https://devportal.jamendo.com",
    fields: [{ key: "jamendo_client_id", ru: "Jamendo client_id", en: "Jamendo client_id", secret: false }],
    why: [
      "Jamendo — главный источник музыки: свободные треки с лицензиями Creative Commons, их можно слушать, скачивать и обрабатывать эквалайзером. Без ключа поиск Jamendo, «Эхо» и скачивание не работают.",
      "Jamendo is the main music source: free Creative Commons tracks you can stream, download and run through the equalizer. Without a key Jamendo search, Echo and downloads do not work.",
    ],
    steps: [
      ["Откройте devportal.jamendo.com и войдите (или зарегистрируйтесь, это бесплатно).", "Создайте приложение (Create an application), название любое, например «Silo Player».", "Скопируйте Client ID и вставьте ниже."],
      ["Open devportal.jamendo.com and sign in (or sign up, it is free).", "Create an application, any name, e.g. “Silo Player”.", "Copy the Client ID and paste it below."],
    ],
  },
  {
    id: "youtube", name: "YouTube", url: "https://console.cloud.google.com/apis/library/youtube.googleapis.com",
    fields: [{ key: "youtube_api_key", ru: "YouTube Data API key", en: "YouTube Data API key", secret: true }],
    why: [
      "Нужен, чтобы искать видео и плейлисты на YouTube. Играют они внутри окна официальным плеером YouTube. Скачивать их и применять эквалайзер нельзя.",
      "Needed to search YouTube videos and playlists. They play inside the window in the official YouTube player. Downloads and the equalizer are not available for them.",
    ],
    steps: [
      ["Откройте console.cloud.google.com и войдите в аккаунт Google.", "Создайте проект, затем на странице «YouTube Data API v3» нажмите «Включить».", "Откройте «Учётные данные» → «Создать учётные данные» → «Ключ API» и скопируйте ключ."],
      ["Open console.cloud.google.com and sign in with Google.", "Create a project, then on the “YouTube Data API v3” page click Enable.", "Go to Credentials → Create credentials → API key and copy the key."],
    ],
    note: ["Бесплатная квота — 10 000 единиц в день, этого хватает на сотни поисков.", "The free quota is 10,000 units per day, enough for hundreds of searches."],
  },
  {
    id: "soundcloud", name: "SoundCloud", url: "https://soundcloud.com/you/apps",
    fields: [
      { key: "soundcloud_client_id", ru: "SoundCloud client_id", en: "SoundCloud client_id", secret: false },
      { key: "soundcloud_client_secret", ru: "SoundCloud client_secret", en: "SoundCloud client_secret", secret: true },
    ],
    why: [
      "Нужен для поиска треков и подборок SoundCloud. Играют во встроенном плеере SoundCloud, без скачивания и эквалайзера. Необязателен.",
      "Needed to search SoundCloud tracks and playlists. They play in the embedded SoundCloud player, without downloads or the equalizer. Optional.",
    ],
    steps: [
      ["Откройте soundcloud.com/you/apps и войдите.", "Зарегистрируйте приложение. SoundCloud выдаёт доступ по заявке, ответ может прийти не сразу.", "Скопируйте Client ID и Client Secret и вставьте ниже."],
      ["Open soundcloud.com/you/apps and sign in.", "Register an app. SoundCloud grants access on request and may take a while to reply.", "Copy the Client ID and Client Secret and paste them below."],
    ],
  },
];

export const ALL_KEYS = SERVICES.flatMap((s) => s.fields.map((f) => f.key));
/** Сервисы с выбором режима: по ссылке (вставка ссылок, без ключей) или через API (поиск по ключам). */
const MODE_SVC = ["youtube", "soundcloud"];
/** Открытые каталоги без ключей (Creative Commons). Включены по умолчанию, можно отключить. */
const CATALOGS: [string, string, [string, string]][] = [
  ["ccmixter", "ccMixter", ["Ремиксы и инструментал под лицензиями Creative Commons. Без ключа.", "Remixes and instrumentals under Creative Commons licenses. No key."]],
  ["archive", "Internet Archive", ["Аудиоархив archive.org, только записи с лицензией Creative Commons. Без ключа.", "archive.org audio, only Creative Commons recordings. No key."]],
];
const EXTRA = [...MODE_SVC.map((m) => "mode_" + m), ...CATALOGS.map(([c]) => "cat_" + c)];

/** Ключи сервисов: для каждого — зачем, как получить, ссылка и поля. Ключи хранятся только локально в базе приложения. */
export default function Keys({ lang, onSaved, saveLabel }: { lang: L; onSaved?: () => void; saveLabel?: string }) {
  const i = lang === "en" ? 1 : 0;
  const [v, setV] = useState<Record<string, string>>({});
  const [ok, setOk] = useState(false);
  const [err, setErr] = useState("");
  useEffect(() => { [...ALL_KEYS, ...EXTRA].forEach((k) => api.getSetting(k).then((x) => setV((o) => ({ ...o, [k]: x }))).catch(() => {})); }, []);
  const modeOf = (id: string) => v["mode_" + id] || (SERVICES.find((s) => s.id === id)!.fields.every((f) => (v[f.key] ?? "").trim()) ? "api" : "link");
  const set = (k: string, x: string) => { setV({ ...v, [k]: x }); setOk(false); };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    Promise.all([...ALL_KEYS.map((k) => api.setSetting(k, v[k] ?? "")), ...MODE_SVC.map((m) => api.setSetting("mode_" + m, modeOf(m))), ...CATALOGS.map(([c]) => api.setSetting("cat_" + c, v["cat_" + c] === "0" ? "0" : "1"))]).then(() => { setOk(true); setErr(""); onSaved?.(); }).catch((x) => setErr(String(x)));
  };
  return (
    <form className="keys" onSubmit={submit}>
      <section className="key-card">
        <h4>{i ? "Open catalogs" : "Открытые каталоги"}</h4>
        <p className="muted">{i ? "Free music with open licenses, plays right in the app. Untick what you don't need." : "Свободная музыка с открытыми лицензиями, играет прямо в приложении. Снимите галочку, если не нужно."}</p>
        {CATALOGS.map(([c, name, d]) => (
          <label key={c} className="row" style={{ gap: 8, alignItems: "flex-start", margin: "4px 0" }}>
            <input type="checkbox" style={{ width: "auto" }} checked={v["cat_" + c] !== "0"} onChange={(e) => set("cat_" + c, e.target.checked ? "1" : "0")} />
            <span><b>{name}</b> <span className="muted">{d[i]}</span></span>
          </label>
        ))}
      </section>
      {SERVICES.map((s) => (
        <section key={s.id} className="key-card">
          <h4>{s.name}</h4>
          <p>{s.why[i]}</p>
          <ol>{s.steps[i].map((x, n) => <li key={n}>{x}</li>)}</ol>
          {s.note && <p className="muted">{s.note[i]}</p>}
          {MODE_SVC.includes(s.id) && (
            <div className="chips" style={{ margin: "6px 0" }}>
              <Btn variant="chip" size="sm" active={modeOf(s.id) === "link"} onClick={() => set("mode_" + s.id, "link")}>{i ? "By link (no key)" : "По ссылке (без ключа)"}</Btn>
              <Btn variant="chip" size="sm" active={modeOf(s.id) === "api"} onClick={() => set("mode_" + s.id, "api")}>{i ? "Via API (search)" : "Через API (поиск)"}</Btn>
            </div>
          )}
          {MODE_SVC.includes(s.id) && modeOf(s.id) === "link" && <p className="muted">{i ? "Paste a track link into Search: it plays inside the app." : "Вставьте ссылку на трек в Поиск: он заиграет внутри приложения."}</p>}
          <div className="row" style={{ margin: "6px 0" }}>
            <Btn size="sm" onClick={() => api.openUrl(s.url).catch((x) => setErr(String(x)))}>{i ? `Open ${s.name} page` : `Открыть страницу ${s.name}`}</Btn>
          </div>
          {s.fields.map((f) => (
            <input key={f.key} type={f.secret ? "password" : "text"} autoComplete="off" spellCheck={false} value={v[f.key] ?? ""} placeholder={i ? f.en : f.ru} aria-label={i ? f.en : f.ru}
              onChange={(e) => { setV({ ...v, [f.key]: e.target.value }); setOk(false); }} />
          ))}
        </section>
      ))}
      <p className="muted">{i ? "Keys are stored only on this computer, in the app's local database, and are sent only to the matching service. You can change them later in Settings." : "Ключи хранятся только на этом компьютере, в локальной базе приложения, и отправляются только в соответствующий сервис. Позже их можно изменить в Настройках."}</p>
      {err && <p className="err">{err}</p>}
      <div><Btn type="submit" variant="solid">{ok ? (i ? "Saved" : "Сохранено") : (saveLabel ?? (i ? "Save" : "Сохранить"))}</Btn></div>
    </form>
  );
}
