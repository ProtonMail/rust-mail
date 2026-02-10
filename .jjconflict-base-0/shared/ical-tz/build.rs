use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::Path;

const WORLD_TERRITORY_ID: &str = "001";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=data/windowsZones.xml");

    // ---

    // https://github.com/unicode-org/cldr/blob/394eff4b41857a515706dd0ecc403a506c841bf9/common/supplemental/windowsZones.xml
    let zones = fs::read_to_string("data/windowsZones.xml").unwrap();
    let zones: SupplementalData = quick_xml::de::from_str(&zones).unwrap();

    let zones: HashMap<_, _> = zones
        .windows_zones
        .map_timezones
        .map_zone
        .into_iter()
        .filter(|zone| zone.territory == WORLD_TERRITORY_ID)
        .map(|zone| (zone.other, zone.ty))
        .collect();

    // ---

    let out = env::var_os("OUT_DIR").unwrap();
    let out = Path::new(&out).join("windows_zones.rs");

    let zones: String = zones
        .into_iter()
        .map(|(lhs, rhs)| format!(r#"("{lhs}", "{rhs}")"#))
        .fold(String::new(), |mut out, line| {
            _ = writeln!(out, "{line},");
            out
        });

    fs::write(
        &out,
        format!(
            "
            use std::collections::HashMap;
            use std::sync::LazyLock;

            pub static MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {{
                HashMap::from_iter(vec![{zones}])
            }});
        "
        ),
    )
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
struct SupplementalData {
    #[serde(rename = "windowsZones")]
    windows_zones: WindowsZones,
}

#[derive(Clone, Debug, Deserialize)]
struct WindowsZones {
    #[serde(rename = "mapTimezones")]
    map_timezones: MapTimezones,
}

#[derive(Clone, Debug, Deserialize)]
struct MapTimezones {
    #[serde(rename = "mapZone")]
    map_zone: Vec<MapZone>,
}

#[derive(Clone, Debug, Deserialize)]
struct MapZone {
    #[serde(rename = "@other")]
    other: String,

    #[serde(rename = "@territory")]
    territory: String,

    #[serde(rename = "@type")]
    ty: String,
}
