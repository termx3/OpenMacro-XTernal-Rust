// SPDX-License-Identifier: AGPL-3.0-only
//! Pointer offsets loaded from `resources/offsets.json` (port target: the
//! offsets handling in `Memory.ahk` / `OffsetsRemote.ahk`).

use std::collections::HashMap;

use thiserror::Error;

/// `(category, field, legacy_key)` — the Rust port of `Memory.ahk`'s
/// `OffsetRenameMap`. Flattens the dumper's nested `Offsets` section into the
/// flat legacy keys the pointer-chains address. Several legacy keys may draw
/// from the same `(category, field)` source — e.g. `FakeDataModel.RealDataModel`
/// feeds both `FakeDataModelToDataModel` and `VisualEngineToDataModel2`, and
/// `GuiObject.Visible` feeds both `TextLabelVisible` and `FrameVisible`.
pub const OFFSET_RENAME_MAP: &[(&str, &str, &str)] = &[
    ("FakeDataModel", "Pointer", "FakeDataModelPointer"),
    ("FakeDataModel", "RealDataModel", "FakeDataModelToDataModel"),
    ("VisualEngine", "Pointer", "VisualEnginePointer"),
    ("VisualEngine", "FakeDataModel", "VisualEngineToDataModel1"),
    ("FakeDataModel", "RealDataModel", "VisualEngineToDataModel2"),
    ("Player", "LocalPlayer", "LocalPlayer"),
    ("Instance", "Name", "Name"),
    ("Instance", "ClassDescriptor", "ClassDescriptor"),
    ("Instance", "ClassName", "ClassDescriptorToClassName"),
    ("Instance", "ChildrenStart", "Children"),
    ("Instance", "Parent", "Parent"),
    ("Misc", "StringLength", "StringLength"),
    ("Misc", "Value", "Value"),
    ("GuiObject", "Text", "TextLabelText"),
    ("GuiObject", "Visible", "TextLabelVisible"),
    ("GuiObject", "Visible", "FrameVisible"),
    ("GuiObject", "ScreenGui_Enabled", "ScreenGuiEnabled"),
    ("GuiObject", "Position", "FramePositionX"),
    ("GuiObject", "Size", "FrameSizeX"),
];

/// Why loading `offsets.json` failed — mirrors the throw-sites in
/// `ApplyParsedOffsets`.
#[derive(Debug, Error)]
pub enum OffsetError {
    #[error("offsets JSON could not be parsed: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("'Offsets' section not found in offsets.json")]
    MissingOffsetsSection,

    #[error("required offset {key:?} not found in offsets.json")]
    MissingRequired { key: &'static str },
}

/// The flattened offset table, keyed by the legacy names used in the pointer
/// chains.
#[derive(Debug, Default, Clone)]
pub struct Offsets {
    pub roblox_version: String,
    pub entries: HashMap<String, i64>,
}

impl Offsets {
    /// Look up an offset by name.
    pub fn get(&self, name: &str) -> Option<i64> {
        self.entries.get(name).copied()
    }

    /// Parse a dumper `offsets.json` and flatten its nested `Offsets` section
    /// through [`OFFSET_RENAME_MAP`]. Port of `LoadOffsets` + `ApplyParsedOffsets`:
    /// captures `"Roblox Version"` (optional, defaults to empty), requires the
    /// `"Offsets"` section, ignores unmapped fields, and requires the one offset
    /// the whole attach chain pivots on — `FakeDataModelPointer`.
    pub fn from_json(json: &str) -> Result<Self, OffsetError> {
        let root: serde_json::Value = serde_json::from_str(json)?;

        let roblox_version = root
            .get("Roblox Version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let section: &serde_json::Map<String, serde_json::Value> = root
            .get("Offsets")
            .and_then(|v| v.as_object())
            .ok_or(OffsetError::MissingOffsetsSection)?;

        let mut entries: HashMap<String, i64> = HashMap::new();
        for (category, field, legacy) in OFFSET_RENAME_MAP {
            if let Some(value) = section
                .get(*category)
                .and_then(|c: &serde_json::Value| c.get(*field))
                .and_then(|v: &serde_json::Value| v.as_i64())
            {
                entries.insert((*legacy).to_owned(), value);
            }
        }

        let offsets = Offsets { roblox_version, entries };
        if !offsets.entries.contains_key("FakeDataModelPointer") {
            return Err(OffsetError::MissingRequired { key: "FakeDataModelPointer" });
        }
        Ok(offsets)
    }

    /// Whether a usable offset table is loaded — port of `AreOffsetsLoaded`:
    /// non-empty and carrying the required `FakeDataModelPointer`.
    pub fn is_valid(&self) -> bool {
        !self.entries.is_empty() && self.entries.contains_key("FakeDataModelPointer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed offsets.json exercising every flattening case: a required
    /// pointer, a renamed field (ChildrenStart → Children), the two
    /// one-source-many-keys cases (RealDataModel, Visible), an absent mapped
    /// category (VisualEngine), and an unmapped category (Junk).
    const FIXTURE: &str = r#"{
        "Roblox Version": "version-abc123",
        "Total Offsets": 7,
        "Offsets": {
            "FakeDataModel": { "Pointer": 100, "RealDataModel": 464 },
            "Instance":      { "Name": 176, "ChildrenStart": 120 },
            "Misc":          { "StringLength": 16 },
            "GuiObject":     { "Visible": 1461 },
            "UnmappedCategory": { "Junk": 999 }
        }
    }"#;

    #[test]
    fn from_json_flattens_nested_offsets_via_the_rename_map() {
        let o = Offsets::from_json(FIXTURE).unwrap();
        assert_eq!(o.roblox_version, "version-abc123");
        assert_eq!(o.get("FakeDataModelPointer"), Some(100));
        assert_eq!(o.get("Name"), Some(176));
        assert_eq!(o.get("Children"), Some(120)); // ChildrenStart → Children
        assert_eq!(o.get("StringLength"), Some(16));
    }

    #[test]
    fn one_nested_source_can_populate_multiple_legacy_keys() {
        let o = Offsets::from_json(FIXTURE).unwrap();
        // FakeDataModel.RealDataModel feeds two legacy names…
        assert_eq!(o.get("FakeDataModelToDataModel"), Some(464));
        assert_eq!(o.get("VisualEngineToDataModel2"), Some(464));
        // …and GuiObject.Visible feeds two more.
        assert_eq!(o.get("TextLabelVisible"), Some(1461));
        assert_eq!(o.get("FrameVisible"), Some(1461));
    }

    #[test]
    fn unmapped_and_absent_offsets_are_simply_missing() {
        let o = Offsets::from_json(FIXTURE).unwrap();
        assert_eq!(o.get("VisualEnginePointer"), None); // VisualEngine category absent
        assert_eq!(o.get("Junk"), None); // unmapped field ignored
    }

    #[test]
    fn missing_offsets_section_is_an_error() {
        let err = Offsets::from_json(r#"{"Roblox Version":"v"}"#).unwrap_err();
        assert!(matches!(err, OffsetError::MissingOffsetsSection));
    }

    #[test]
    fn missing_fake_data_model_pointer_is_an_error() {
        let json = r#"{"Offsets":{"Instance":{"Name":176}}}"#;
        let err = Offsets::from_json(json).unwrap_err();
        assert!(matches!(err, OffsetError::MissingRequired { key } if key == "FakeDataModelPointer"));
    }

    #[test]
    fn invalid_json_is_a_parse_error() {
        let err = Offsets::from_json("{ not json").unwrap_err();
        assert!(matches!(err, OffsetError::Parse(_)));
    }

    #[test]
    fn roblox_version_defaults_to_empty_when_absent() {
        let json = r#"{"Offsets":{"FakeDataModel":{"Pointer":1}}}"#;
        let o = Offsets::from_json(json).unwrap();
        assert_eq!(o.roblox_version, "");
    }

    #[test]
    fn is_valid_reflects_presence_of_the_required_pointer() {
        assert!(Offsets::from_json(FIXTURE).unwrap().is_valid());
        assert!(!Offsets::default().is_valid());
    }
}
