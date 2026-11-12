#!/usr/bin/env python3
"""Собирает taxonomy/taxonomy.json из tools/taxonomy.src.txt и проверяет ссылки. Запуск: python3 tools/build_taxonomy.py"""
import json, sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
EQ = {
    "bass": [6, 5, 4, 2, 0, 0, 0, 1, 1, 1], "rock": [4, 3, 2, -1, -2, -1, 2, 3, 4, 4], "metal": [4, 3, 1, -2, -3, -1, 2, 4, 5, 4],
    "pop": [1, 1, 0, 1, 3, 3, 2, 1, 1, 1], "rnb": [5, 4, 2, 0, -1, 1, 2, 1, 1, 1], "electronic": [5, 4, 2, 0, -1, 0, 1, 2, 4, 4],
    "indie": [1, 1, 0, 1, 2, 2, 1, 1, 2, 1], "jazz": [2, 1, 0, 1, -1, -1, 0, 1, 2, 3], "classical": [0, 0, 0, 0, 0, 0, 0, 1, 2, 2],
    "chill": [3, 2, 1, 1, 0, -1, -1, 0, 0, -1], "folk": [0, 1, 1, 2, 2, 1, 1, 2, 2, 1], "punk": [3, 2, 1, 0, -1, 0, 2, 3, 3, 3],
    "funk": [4, 3, 2, 1, 0, 1, 2, 2, 2, 2], "latin": [3, 2, 1, 1, 1, 2, 2, 2, 2, 1], "reggae": [5, 4, 2, 1, 0, 0, 1, 1, 1, 0],
    "country": [1, 1, 1, 2, 3, 2, 2, 2, 2, 1], "world": [1, 1, 1, 1, 1, 1, 1, 1, 1, 1], "ambient": [2, 1, 0, 0, 0, 0, 0, 1, 1, 1],
    "soundtrack": [1, 1, 0, 0, 0, 0, 1, 1, 2, 2], "blues": [3, 2, 1, 1, 1, 1, 2, 2, 1, 0], "flat": [0] * 10,
}
# уровни глубже семейства: ребёнок -> родитель-жанр (иначе родитель — семейство)
PARENT = {}
def kids(p, *c):
    for x in c: PARENT[x] = p
kids("house", "deep_house", "tech_house", "progressive_house", "electro_house", "acid_house", "chicago_house", "afro_house", "tropical_house", "lounge_house", "future_house")
kids("techno", "minimal_techno", "detroit_techno", "acid_techno", "hard_techno", "industrial_techno")
kids("trance", "progressive_trance", "uplifting_trance", "psytrance")
kids("psytrance", "goa_trance", "full_on")
kids("dubstep", "brostep", "riddim", "melodic_dubstep")
kids("drum_and_bass", "liquid_dnb", "neurofunk", "jungle")
kids("edm", "big_room", "trap_edm", "future_bass")
kids("future_bass", "kawaii_bass")
kids("synthwave", "outrun", "darksynth")
kids("trap", "russian_trap", "latin_trap", "emo_rap", "cloud_rap")
kids("drill", "uk_drill", "russian_drill")
kids("boom_bap", "old_school_hiphop", "east_coast", "conscious_hiphop", "underground_hiphop")
kids("lofi_hiphop", "chillhop")
kids("west_coast", "g_funk")
kids("black_metal", "atmospheric_black")
kids("death_metal", "deathcore", "grindcore")
kids("doom_metal", "sludge")
kids("alt_rock", "grunge", "shoegaze")
kids("prog_rock", "art_rock", "space_rock")
kids("punk_rock", "pop_punk", "hardcore_punk", "oi", "folk_punk", "russian_punk")
kids("hardcore_punk", "crust", "post_hardcore")
kids("post_punk", "goth_rock")
kids("jazz_piano", "ecm_jazz")
kids("bebop", "hard_bop")
kids("swing", "big_band", "dixieland")
kids("salsa", "mambo")
kids("tango", "nuevo_tango")
kids("samba", "pagode")
kids("neoclassical", "ambient_piano")
kids("string_quartet", "violin", "cello")
kids("indian_classical", "sitar", "qawwali")
GEN_EQ = {"lofi_hiphop": "chill", "chillhop": "chill", "trap": "bass", "phonk": "bass", "dubstep": "bass", "drum_and_bass": "bass", "lofi_beats": "chill",
          "sleep_music": "ambient", "dark_ambient": "ambient", "k_pop": "pop", "deep_house": "chill", "lounge": "chill", "downtempo": "chill", "trip_hop": "chill",
          "doom_metal": "metal", "grindcore": "metal", "bossa_nova": "chill", "smooth_jazz": "chill", "piano_solo": "classical", "opera": "classical"}

nodes, cur_fam, cur_kind = [], None, None
for ln in (root / "tools/taxonomy.src.txt").read_text(encoding="utf8").splitlines():
    if not ln.strip() or ln.startswith("#"): continue
    if ln.startswith("@fam "):
        i, ru, en, eq, tags = (ln[5:].split("|") + [""] * 5)[:5]
        nodes.append(dict(id=i, kind="family", parent=None, ru=ru, en=en, eq=EQ[eq], tags=tags.split(",") if tags else [], alias=[], near=[], lang=None))
        cur_fam, cur_kind = i, "genre"
    elif ln.startswith("@grp "):
        kind, i, ru, en = (ln[5:].split("|") + [""] * 4)[:4]
        # формат: "@grp kind id|ru|en" -> первое поле содержит "kind id"
        k, gid = kind.split()
        nodes.append(dict(id=gid, kind="group", group=k, parent=None, ru=i, en=ru, eq=None, tags=[], alias=[], near=[], lang=None))
        cur_fam, cur_kind = gid, k
    elif ln.startswith("- "):
        f = (ln[2:].split("|") + [""] * 7)[:7]
        i, ru, en, tags, alias, near, lang = f
        nodes.append(dict(id=i, kind=cur_kind, parent=PARENT.get(i, cur_fam), ru=ru, en=en, tags=[x for x in tags.split(",") if x],
                          alias=[x for x in alias.split(",") if x], near=[x for x in near.split(",") if x], lang=lang or None,
                          eq=EQ[GEN_EQ[i]] if i in GEN_EQ else None, fam=cur_fam))
FAM_NEAR = {"rock": "indie,punk,blues,metal,folk", "metal": "rock,punk,electronic", "punk": "rock,indie,metal", "indie": "rock,folk,pop,punk",
    "hiphop": "rnb,funk,electronic,pop", "electronic": "ambient,pop,funk,hiphop", "pop": "indie,electronic,rnb,funk", "rnb": "hiphop,pop,funk,jazz",
    "funk": "rnb,jazz,reggae,hiphop", "jazz": "blues,rnb,ambient,classical,funk", "blues": "rock,jazz,folk,rnb,country", "classical": "ambient,jazz,soundtrack,folk",
    "folk": "indie,blues,country,classical,world", "country": "folk,blues,rock", "reggae": "funk,hiphop,pop,latin", "latin": "reggae,pop,world,jazz",
    "world": "folk,latin,ambient", "ambient": "electronic,classical,jazz,soundtrack", "soundtrack": "classical,ambient,electronic", "experimental": "ambient,electronic,rock"}
for n in nodes:
    if n["kind"] == "family": n["near"] = FAM_NEAR.get(n["id"], "").split(",")
ids = [n["id"] for n in nodes]
errs = [f"дубликат id {i}" for i in set(ids) if ids.count(i) > 1]
idset = set(ids)
for n in nodes:
    if n["parent"] and n["parent"] not in idset: errs.append(f"{n['id']}: нет родителя {n['parent']}")
    for x in n["near"]:
        if x not in idset: errs.append(f"{n['id']}: нет близкого {x}")
if errs:
    print("\n".join(errs)); sys.exit(1)
for n in nodes: n.pop("fam", None); n["near"] = [x for x in n["near"] if x != n["id"]]
(root / "taxonomy").mkdir(exist_ok=True)
(root / "taxonomy/taxonomy.json").write_text(json.dumps({"version": 1, "nodes": nodes}, ensure_ascii=False, indent=0, separators=(",", ":")), encoding="utf8")
print(len(nodes), "узлов;", sum(n["kind"] == "genre" for n in nodes), "жанров;", sum(n["kind"] == "family" for n in nodes), "семейств")
