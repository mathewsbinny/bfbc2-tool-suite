//! The text rules against Project OPFOR's cases (`src\tools\tests\test_ww2_install.py`, the Python they replace).

use super::*;

const MFT_STOCK: &str = "Signature 0123\r\nAuthorative\r\n\r\nBundleSource game:levels/mp_001/level-00.fbrb\r\n\
                         BundleTarget package:levels/mp_001/level-00.fbrb\r\n\r\nBundleSource game:levels/mp_002/level-00.fbrb\r\n";
const INI_STOCK: &str =
    "[dev]\r\n; F9 starts the [dev] level\r\nlevel=Levels/MP_005GR\r\nautostart=0\r\n[debug]\r\nprobes=0\r\n";
const MAPS_STOCK: &str = "; catalogue\r\n\r\n[MP_001]\r\nname=Panama Canal\r\nConquest=Levels/MP_001\r\n\r\n\
                          [NAM_MP_002]\r\nname=Vietnam: Vantage Point\r\nConquest=Levels/NAM_MP_002CQ\r\n";
const MARK: &str =
    "; --- Pacific War: the installed WW2 levels (wake_install.py writes this part; --revert takes it out) ---";

fn kinds() -> Vec<String> {
    ["level", "terrain", "ingame", "loader", "vo/en"]
        .map(str::to_owned)
        .to_vec()
}

fn blocks(lv: &str) -> Vec<String> {
    kinds()
        .iter()
        .map(|k| {
            let own = format!("levels/{lv}/{k}-00.fbrb");
            let mut l = vec![format!("BundleSource game:{own}")];
            if !k.starts_with("vo/") {
                l.push(format!("BundleTarget package:{own}"));
            }
            l.join("\r\n")
        })
        .collect()
}

fn levels() -> Vec<MapsLevel> {
    [
        ("WAKE_ISLAND_S", "Wake Island", "Conquest"),
        ("GUADAL_CANAL", "Guadalcanal", "Conquest"),
        ("CORAL_SEA", "Coral Sea", "Air Superiority"),
    ]
    .iter()
    .map(|(n, t, m)| MapsLevel {
        name: (*n).to_owned(),
        title: (*t).to_owned(),
        browser_mode: (*m).to_owned(),
    })
    .collect()
}

fn names() -> Vec<String> {
    ["wake_island_s", "guadal_canal", "iwo_jima_s", "coral_sea"]
        .map(str::to_owned)
        .to_vec()
}

#[test]
fn mft_parse_round_trips_a_stock_file() {
    let (h, b) = mft_parse(MFT_STOCK);
    assert_eq!(mft_text(&h, &b), MFT_STOCK);
}

#[test]
fn mft_adds_once_and_keeps_the_rest() {
    let (new, line) = mft_with_level(MFT_STOCK, "wake_island_s", &blocks("wake_island_s"));
    let new = new.unwrap_or_default();
    assert_eq!(line, "package.mft: the level's blocks written (0 there before, 5 now)");
    assert!(new.starts_with(MFT_STOCK.trim_end_matches(['\r', '\n'])));
    assert_eq!(new.matches("BundleSource game:levels/wake_island_s/").count(), 5);
    let (again, line) = mft_with_level(&new, "wake_island_s", &blocks("wake_island_s"));
    assert!(again.is_none());
    assert_eq!(line, "package.mft: the level's 5 blocks there");
    // a block appended without the blank line is a block of its own (audit M7)
    let t = new.trim_end_matches(['\r', '\n']).to_owned() + "\r\nBundleSource game:levels/other/level-00.fbrb\r\n";
    let (h, b) = mft_parse(&t);
    assert_eq!(
        b.last().and_then(|x| x.first()).map(String::as_str),
        Some("BundleSource game:levels/other/level-00.fbrb")
    );
    assert!(mft_text(&h, &b).contains("other/level-00.fbrb"));
}

#[test]
fn bm_adds_and_repairs() {
    let stock = "levels/mp_001/level\t00000000-0000-0000-0000-000000000000\t1\n\
                 levels/ons_mp_005gr/terrain\t00000000-0000-0000-0000-000000000000\n";
    let (new, add, broken) = bm_with_level(stock, "wake_island_s", &kinds());
    assert_eq!(add.len(), 5);
    assert_eq!(
        broken,
        ["levels/ons_mp_005gr/terrain\t00000000-0000-0000-0000-000000000000"]
    );
    let new = new.unwrap_or_default();
    assert!(new.contains("levels/ons_mp_005gr/terrain\t00000000-0000-0000-0000-000000000000\t1\n"));
    assert_eq!(new.matches("levels/wake_island_s/level\t").count(), 1);
    assert_eq!(bm_with_level(&new, "wake_island_s", &kinds()).0, None);
    assert_eq!(
        py_list_repr(&broken),
        "['levels/ons_mp_005gr/terrain\\t00000000-0000-0000-0000-000000000000']"
    );
}

#[test]
fn set_ini_cases() {
    let v = "Levels/WAKE_ISLAND_S";
    // a [ inside a comment of the section is not a section header
    assert_eq!(
        ini_with_level(INI_STOCK, v).unwrap_or_default(),
        INI_STOCK.replace("level=Levels/MP_005GR", &format!("level={v}"))
    );
    // spaces around the =, the key's own spelling and the line ending kept
    assert_eq!(
        ini_with_level("[dev]\r\nLevel = Levels/MP_005\r\n", v).unwrap_or_default(),
        format!("[dev]\r\nLevel = {v}\r\n")
    );
    // the first [dev] level line only; another section's level untouched
    assert_eq!(
        ini_with_level("[other]\nlevel=A\n[dev]\nlevel=B\n", v).unwrap_or_default(),
        format!("[other]\nlevel=A\n[dev]\nlevel={v}\n")
    );
    // a commented-out level line is not the key
    assert_eq!(
        ini_with_level("[dev]\n;level=X\nlevel=Y", v).unwrap_or_default(),
        format!("[dev]\n;level=X\nlevel={v}")
    );
    assert_eq!(
        ini_with_level("[dev]\nautostart=1\n[x]\nlevel=A\n", v),
        Err(TextError("opfor.ini: no [dev] level= line".to_owned()))
    );
}

#[test]
fn ini_with_key_cases() {
    assert_eq!(
        ini_with_key("[dev]\r\nlevel=A\r\n", "gamemode", "pacific_levels", "L"),
        "[dev]\r\nlevel=A\r\n\r\n[gamemode]\r\npacific_levels=L\r\n"
    );
    let t = "[gamemode]\n; c\npacific_war=1\nPacific_Levels = old\n[x]\nk=1\n";
    assert_eq!(
        ini_with_key(t, "gamemode", "pacific_levels", "L"),
        t.replace("= old", "= L")
    );
    let t = "[gamemode]\npacific_war=1\n\n[x]\nk=1\n";
    assert_eq!(
        ini_with_key(t, "gamemode", "pacific_levels", "L"),
        "[gamemode]\npacific_war=1\npacific_levels=L\n\n[x]\nk=1\n"
    );
    // a commented-out key or another section's key is not it
    let t = "[x]\npacific_levels=X\n[gamemode]\n;pacific_levels=Y\n";
    assert_eq!(
        ini_with_key(t, "gamemode", "pacific_levels", "L"),
        "[x]\npacific_levels=X\n[gamemode]\npacific_levels=L\n;pacific_levels=Y\n"
    );
    assert_eq!(ini_with_key("", "a", "k", "v"), "\n[a]\nk=v\n");
    assert_eq!(ini_with_key("[a]", "a", "k", "v"), "[a]\nk=v\n");
    assert_eq!(
        ini_without_key(
            "[gamemode]\r\npacific_levels=X\r\nk=1\r\n",
            "gamemode",
            "pacific_levels"
        ),
        "[gamemode]\r\nk=1\r\n"
    );
    assert_eq!(
        ini_value("[dev]\r\n level = A \r\n", "DEV", "Level"),
        Some("A".to_owned())
    );
    assert_eq!(ini_value("[dev]\r\n;level=A\r\n", "dev", "level"), None);
}

#[test]
fn maps_with_levels_adds_replaces_and_takes_out() {
    let all = levels();
    let one = maps_with_levels(MAPS_STOCK, &all, &names(), MARK);
    let want: Vec<String> = all
        .iter()
        .map(|l| {
            format!(
                "[{n}]\r\nname={t}\r\n{m}=Levels/{n}\r\n",
                n = l.name,
                t = l.title,
                m = l.browser_mode
            )
        })
        .collect();
    assert_eq!(one, format!("{MAPS_STOCK}\r\n{MARK}\r\n{}", want.join("\r\n")));
    assert_eq!(maps_with_levels(&one, &all, &names(), MARK), one);
    assert_eq!(maps_with_levels(&one, &[], &names(), MARK), MAPS_STOCK);
    let two = all.get(..2).unwrap_or_default();
    assert!(!maps_with_levels(&one, two, &names(), MARK).contains("[CORAL_SEA]"));
    assert_eq!(
        levels_value(&["WAKE_ISLAND_S".to_owned(), "CORAL_SEA".to_owned()]),
        "Levels/WAKE_ISLAND_S,Levels/CORAL_SEA"
    );
}
