//! Font families installed on this computer, for the appearance settings to offer.

use std::collections::BTreeMap;

use serde::Serialize;
use ttf_parser::{name_id, Face, Language, PlatformId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFamily {
    pub name: String,
    /// The family's older per-style names (`Segoe UI Variable Text`), for web views that do not
    /// know it by its main name.
    pub alternates: Vec<String>,
    /// Every face of the family has fixed-width glyphs.
    pub monospaced: bool,
}

struct FaceNames {
    family: String,
    legacy_families: Vec<String>,
    monospaced: bool,
}

pub fn installed_families() -> Vec<FontFamily> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let faces = database.faces().filter_map(|face| {
        let family = face.families.first()?.0.clone();
        let legacy_families = database
            .with_face_data(face.id, |data, index| {
                let face = Face::parse(data, index).ok()?;
                (!is_symbol_font(&face)).then(|| legacy_family_names(&face))
            })
            .flatten()?;
        Some(FaceNames {
            family,
            legacy_families,
            monospaced: face.monospaced,
        })
    });
    group_families(faces)
}

/// Fonts that draw pictures where letters belong, which would make the interface unreadable:
/// Windows symbol fonts (Wingdings) use the symbol encoding, and clones of the PostScript Symbol
/// and Dingbats fonts name the glyph for "A" `Alpha` or `a` and a number.
fn is_symbol_font(face: &Face) -> bool {
    let symbol_encoding = face.tables().cmap.is_some_and(|map| {
        map.subtables.into_iter().any(|subtable| {
            subtable.platform_id == PlatformId::Windows && subtable.encoding_id == 0
        })
    });
    let letter_name = face
        .glyph_index('A')
        .and_then(|glyph| face.glyph_name(glyph));
    let symbol_glyph_name = |name: &str| {
        name == "Alpha"
            || name.strip_prefix('a').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
    };
    symbol_encoding || letter_name.is_some_and(symbol_glyph_name)
}

/// The English "Font Family" names (name ID 1), which fontdb drops when a face also has a
/// typographic family name.
fn legacy_family_names(face: &Face) -> Vec<String> {
    face.names()
        .into_iter()
        .filter(|name| {
            name.name_id == name_id::FAMILY && name.language() == Language::English_UnitedStates
        })
        .filter_map(|name| name.to_string())
        .collect()
}

/// One entry per family name, ignoring case, sorted by name. Names starting with a dot are
/// private system faces on macOS that web views refuse to use.
fn group_families(faces: impl Iterator<Item = FaceNames>) -> Vec<FontFamily> {
    let mut families = BTreeMap::<String, FontFamily>::new();
    for face in faces {
        let name = face.family.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let family = families
            .entry(name.to_lowercase())
            .or_insert_with(|| FontFamily {
                name: name.to_string(),
                alternates: Vec::new(),
                monospaced: face.monospaced,
            });
        family.monospaced &= face.monospaced;
        for alternate in face.legacy_families {
            let alternate = alternate.trim();
            let known = family.name.eq_ignore_ascii_case(alternate)
                || family
                    .alternates
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(alternate));
            if !alternate.is_empty() && !known {
                family.alternates.push(alternate.to_string());
            }
        }
    }
    families.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(family: &str, legacy_families: &[&str], monospaced: bool) -> FaceNames {
        FaceNames {
            family: family.to_string(),
            legacy_families: legacy_families
                .iter()
                .map(|name| name.to_string())
                .collect(),
            monospaced,
        }
    }

    #[test]
    fn merges_faces_into_sorted_families() {
        let faces = vec![
            face("Noto Sans", &["Noto Sans"], false),
            face("Fira Code", &[], true),
            face("fira code", &["Fira Code Light"], true),
            face("Mixed", &[], true),
            face("Mixed", &[], false),
            face(".SF NS", &[], false),
            face("  ", &[], false),
            face(
                "Segoe UI Variable",
                &["Segoe UI Variable Text", "Segoe UI Variable Display"],
                false,
            ),
            face("Segoe UI Variable", &["segoe ui variable text"], false),
        ];
        let family = |name: &str, alternates: &[&str], monospaced: bool| FontFamily {
            name: name.to_string(),
            alternates: alternates.iter().map(|name| name.to_string()).collect(),
            monospaced,
        };
        assert_eq!(
            group_families(faces.into_iter()),
            vec![
                family("Fira Code", &["Fira Code Light"], true),
                family("Mixed", &[], false),
                family("Noto Sans", &[], false),
                family(
                    "Segoe UI Variable",
                    &["Segoe UI Variable Text", "Segoe UI Variable Display"],
                    false
                ),
            ]
        );
    }

    #[test]
    fn lists_installed_families_without_failing() {
        let families = installed_families();
        let names: Vec<_> = families
            .iter()
            .map(|family| family.name.to_lowercase())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }
}
