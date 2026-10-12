//! Loads datapack predicates and evaluates the checks story advancements and
//! `minecraft:tool/can_silk_touch` actually use.
//!
//! Unknown predicate types are logged and left out of the registry.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use pumpkin_data::tag::{RegistryKey, get_tag_values};
use serde_json::{Map, Value};
use tracing::warn;

const EMBEDDED_PREDICATES: &[(&str, &str)] = &[
    (
        "minecraft:tool/can_silk_touch",
        include_str!(
            "../../../../../assets/datapack/data/minecraft/predicate/tool/can_silk_touch.json"
        ),
    ),
    (
        "minecraft:tool/can_shear",
        include_str!("../../../../../assets/datapack/data/minecraft/predicate/tool/can_shear.json"),
    ),
    (
        "minecraft:block/fast_cooking",
        include_str!(
            "../../../../../assets/datapack/data/minecraft/predicate/block/fast_cooking.json"
        ),
    ),
];

/// A predicate built from the supported item, location, and entity checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Predicate {
    /// `minecraft:match_tool`. Tests [`PredicateContext::item`].
    MatchTool(ItemPredicate),
    /// `minecraft:entity_properties` for the `this` entity.
    EntityProperties(EntityPredicate),
}

/// Item id, item tag, and enchantment checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPredicate {
    /// `None` accepts any item. An empty set accepts none.
    pub items: Option<NameSet>,
    pub enchantments: Vec<EnchantmentPredicate>,
}

/// Enchantment id or tag, plus an inclusive level range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnchantmentPredicate {
    pub enchantments: NameSet,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
}

/// Entity sub-predicates. Story location criteria only set [`Self::location`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityPredicate {
    pub location: Option<LocationPredicate>,
}

/// Location check. Story advancements only name a structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationPredicate {
    pub structures: Option<NameSet>,
}

/// One id, one tag, or a list of either. Any entry matching is enough.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameSet {
    pub entries: Vec<NameEntry>,
}

/// A registry id or a tag (`#namespace:path` in JSON, stored without `#`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameEntry {
    Id(String),
    Tag(String),
}

/// One item stack the predicate can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSample {
    pub id: String,
    pub enchantments: Vec<ItemEnchantment>,
}

/// One enchantment on an [`ItemSample`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemEnchantment {
    pub id: String,
    pub level: i32,
}

/// Facts a predicate is tested against. The predicate does not read the world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PredicateContext {
    /// Tool for [`Predicate::MatchTool`].
    pub item: Option<ItemSample>,
    /// Structures that contain the tested position.
    pub structures: Vec<String>,
}

enum ParseError {
    UnknownType(String),
    Unsupported(String),
    Invalid(String),
}

impl ParseError {
    fn reason(&self) -> String {
        match self {
            Self::UnknownType(type_name) => format!("unknown type '{type_name}'"),
            Self::Unsupported(check) => format!("unsupported check '{check}'"),
            Self::Invalid(detail) => format!("invalid predicate ({detail})"),
        }
    }
}

impl ItemSample {
    /// An item with no enchantments.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            enchantments: Vec::new(),
        }
    }

    /// Adds one enchantment and returns the sample.
    #[must_use]
    pub fn with_enchantment(mut self, id: impl Into<String>, level: i32) -> Self {
        self.enchantments.push(ItemEnchantment {
            id: id.into(),
            level,
        });
        self
    }
}

impl Predicate {
    /// Returns whether this predicate holds for `ctx`.
    #[must_use]
    pub fn test(&self, ctx: &PredicateContext) -> bool {
        match self {
            Self::MatchTool(item) => ctx.item.as_ref().is_some_and(|tool| item.test(tool)),
            Self::EntityProperties(entity) => entity.test(&ctx.structures),
        }
    }
}

impl ItemPredicate {
    /// Returns whether `item` matches this item predicate.
    #[must_use]
    pub fn test(&self, item: &ItemSample) -> bool {
        if let Some(items) = &self.items
            && !items.matches(&item.id, RegistryKey::Item)
        {
            return false;
        }
        self.enchantments
            .iter()
            .all(|predicate| predicate.test(item))
    }
}

impl EnchantmentPredicate {
    fn test(&self, item: &ItemSample) -> bool {
        item.enchantments.iter().any(|enchantment| {
            self.enchantments
                .matches(&enchantment.id, RegistryKey::Enchantment)
                && level_in_range(enchantment.level, self.min_level, self.max_level)
        })
    }
}

impl EntityPredicate {
    fn test(&self, structures: &[String]) -> bool {
        self.location
            .as_ref()
            .is_none_or(|location| location.test(structures))
    }
}

impl LocationPredicate {
    fn test(&self, present: &[String]) -> bool {
        self.structures.as_ref().is_none_or(|structures| {
            present
                .iter()
                .any(|id| structures.matches(id, RegistryKey::WorldgenStructure))
        })
    }
}

impl NameSet {
    fn matches(&self, id: &str, category: RegistryKey) -> bool {
        self.entries.iter().any(|entry| entry.matches(id, category))
    }
}

impl NameEntry {
    fn matches(&self, id: &str, category: RegistryKey) -> bool {
        match self {
            Self::Id(expected) => same_id(expected, id),
            Self::Tag(tag) => tag_contains(category, tag, id),
        }
    }
}

/// Every predicate must match some stack. One stack may satisfy more than one predicate.
#[must_use]
pub fn test_item_predicates(predicates: &[ItemPredicate], items: &[ItemSample]) -> bool {
    let mut pending: Vec<&ItemPredicate> = predicates.iter().collect();
    for item in items {
        pending.retain(|predicate| !predicate.test(item));
        if pending.is_empty() {
            return true;
        }
    }
    pending.is_empty()
}

/// Parses a predicate JSON document. Unknown types are logged and return `None`.
#[must_use]
pub fn parse_predicate_json(id: Option<&str>, json: &str) -> Option<Predicate> {
    match serde_json::from_str::<Value>(json) {
        Ok(value) => parse_predicate_value(id, &value),
        Err(error) => {
            report(id, &format!("invalid JSON ({error})"));
            None
        }
    }
}

/// Parses a predicate object. Unknown types are logged and return `None`.
#[must_use]
pub fn parse_predicate_value(id: Option<&str>, value: &Value) -> Option<Predicate> {
    finish(id, parse_predicate_inner(value))
}

/// Parses one item predicate object, such as an entry in an inventory criterion.
#[must_use]
pub fn parse_item_predicate(id: Option<&str>, value: &Value) -> Option<ItemPredicate> {
    finish(id, parse_item_predicate_inner(value))
}

/// Parses the `items` array of an `inventory_changed` criterion.
#[must_use]
pub fn parse_item_predicates(id: Option<&str>, items: &Value) -> Option<Vec<ItemPredicate>> {
    finish(id, parse_item_predicates_inner(items))
}

/// Loads the vanilla predicate files compiled into the server.
///
/// A file whose type is not supported is reported and omitted, so a later pack
/// can still define that id.
pub fn load_embedded_predicates<S: std::hash::BuildHasher>(
    registry: &mut HashMap<String, Arc<Predicate>, S>,
) -> usize {
    let mut loaded = 0;
    for &(id, json) in EMBEDDED_PREDICATES {
        if insert_parsed(registry, id, parse_predicate_json(Some(id), json)) {
            loaded += 1;
        }
    }
    loaded
}

/// Loads `data/<namespace>/predicate/**/*.json` into `registry`.
pub fn load_predicates_from_dir<S: std::hash::BuildHasher>(
    namespace: &str,
    dir: &Path,
    registry: &mut HashMap<String, Arc<Predicate>, S>,
) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let mut loaded = 0;
    load_predicates_recursive(namespace, dir, dir, registry, &mut loaded);
    loaded
}

fn load_predicates_recursive<S: std::hash::BuildHasher>(
    namespace: &str,
    base_dir: &Path,
    current_dir: &Path,
    registry: &mut HashMap<String, Arc<Predicate>, S>,
    loaded: &mut usize,
) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_predicates_recursive(namespace, base_dir, &path, registry, loaded);
            continue;
        }
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let Ok(rel_path) = path.strip_prefix(base_dir) else {
            continue;
        };
        let mut stem_path = rel_path.to_string_lossy().to_string();
        if let Some(stem) = stem_path.strip_suffix(".json") {
            stem_path = stem.to_string();
        }
        let stem_path = stem_path.replace('\\', "/");
        let id = format!("{namespace}:{stem_path}");

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                report(Some(&id), &error.to_string());
                registry.remove(&id);
                continue;
            }
        };
        if insert_parsed(registry, &id, parse_predicate_json(Some(&id), &content)) {
            *loaded += 1;
        }
    }
}

fn insert_parsed<S: std::hash::BuildHasher>(
    registry: &mut HashMap<String, Arc<Predicate>, S>,
    id: &str,
    predicate: Option<Predicate>,
) -> bool {
    if let Some(predicate) = predicate {
        registry.insert(id.to_string(), Arc::new(predicate));
        true
    } else {
        registry.remove(id);
        false
    }
}

fn finish<T>(id: Option<&str>, result: Result<T, ParseError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            report(id, &error.reason());
            None
        }
    }
}

fn report(id: Option<&str>, reason: &str) {
    if let Some(id) = id {
        warn!("Skipping predicate '{id}': {reason}");
    } else {
        warn!("Skipping predicate: {reason}");
    }
}

fn parse_predicate_inner(value: &Value) -> Result<Predicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    let type_name = obj
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| obj.get("condition").and_then(Value::as_str))
        .ok_or_else(|| ParseError::Invalid("missing type".to_string()))?;
    match type_path(type_name) {
        "match_tool" => parse_match_tool(obj),
        "entity_properties" => parse_entity_properties(obj),
        _ => Err(ParseError::UnknownType(type_name.to_string())),
    }
}

fn parse_match_tool(obj: &Map<String, Value>) -> Result<Predicate, ParseError> {
    require_keys(obj, &["type", "condition", "predicate"])?;
    let item = match obj.get("predicate") {
        Some(predicate) => parse_item_predicate_inner(predicate)?,
        None => ItemPredicate {
            items: None,
            enchantments: Vec::new(),
        },
    };
    Ok(Predicate::MatchTool(item))
}

fn parse_entity_properties(obj: &Map<String, Value>) -> Result<Predicate, ParseError> {
    require_keys(obj, &["type", "condition", "entity", "predicate"])?;
    let entity = obj
        .get("entity")
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::Invalid("missing entity".to_string()))?;
    // Story location criteria test the player. Vanilla names that entity `this`.
    if entity != "this" {
        return Err(ParseError::Unsupported(format!("entity target {entity}")));
    }
    let predicate = obj
        .get("predicate")
        .ok_or_else(|| ParseError::Invalid("missing predicate".to_string()))?;
    Ok(Predicate::EntityProperties(parse_entity_predicate(
        predicate,
    )?))
}

fn parse_entity_predicate(value: &Value) -> Result<EntityPredicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["minecraft:location"])?;
    let location = match obj.get("minecraft:location") {
        Some(location) => Some(parse_location(location)?),
        None => None,
    };
    Ok(EntityPredicate { location })
}

fn parse_location(value: &Value) -> Result<LocationPredicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["structures"])?;
    let structures = match obj.get("structures") {
        Some(structures) => Some(parse_name_set(structures)?),
        None => None,
    };
    Ok(LocationPredicate { structures })
}

fn parse_item_predicates_inner(items: &Value) -> Result<Vec<ItemPredicate>, ParseError> {
    let Some(entries) = items.as_array() else {
        return Err(ParseError::Invalid("expected an item list".to_string()));
    };
    let mut predicates = Vec::with_capacity(entries.len());
    for entry in entries {
        predicates.push(parse_item_predicate_inner(entry)?);
    }
    Ok(predicates)
}

fn parse_item_predicate_inner(value: &Value) -> Result<ItemPredicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["items", "predicates"])?;
    let items = match obj.get("items") {
        Some(items) => Some(parse_name_set(items)?),
        None => None,
    };
    let enchantments = match obj.get("predicates") {
        Some(predicates) => parse_enchantment_predicates(predicates)?,
        None => Vec::new(),
    };
    Ok(ItemPredicate {
        items,
        enchantments,
    })
}

fn parse_enchantment_predicates(value: &Value) -> Result<Vec<EnchantmentPredicate>, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["minecraft:enchantments"])?;
    match obj.get("minecraft:enchantments") {
        Some(Value::Array(entries)) => {
            let mut predicates = Vec::with_capacity(entries.len());
            for entry in entries {
                predicates.push(parse_enchantment_predicate(entry)?);
            }
            Ok(predicates)
        }
        Some(_) => Err(ParseError::Unsupported(
            "minecraft:enchantments".to_string(),
        )),
        None => Ok(Vec::new()),
    }
}

fn parse_enchantment_predicate(value: &Value) -> Result<EnchantmentPredicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["enchantments", "levels"])?;
    let enchantments = parse_name_set(
        obj.get("enchantments")
            .ok_or_else(|| ParseError::Invalid("missing enchantments".to_string()))?,
    )?;
    let (min_level, max_level) = match obj.get("levels") {
        Some(levels) => parse_level_bounds(levels)?,
        None => (None, None),
    };
    Ok(EnchantmentPredicate {
        enchantments,
        min_level,
        max_level,
    })
}

fn parse_level_bounds(value: &Value) -> Result<(Option<i32>, Option<i32>), ParseError> {
    if let Some(exact) = json_i32(value) {
        return Ok((Some(exact), Some(exact)));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Unsupported("levels".to_string()))?;
    require_keys(obj, &["min", "max"])?;
    let min_level = optional_i32(obj.get("min"), "levels.min")?;
    let max_level = optional_i32(obj.get("max"), "levels.max")?;
    Ok((min_level, max_level))
}

fn optional_i32(value: Option<&Value>, label: &str) -> Result<Option<i32>, ParseError> {
    value.map_or(Ok(None), |value| {
        json_i32(value)
            .map(Some)
            .ok_or_else(|| ParseError::Invalid(label.to_string()))
    })
}

fn parse_name_set(value: &Value) -> Result<NameSet, ParseError> {
    match value {
        Value::String(raw) => Ok(NameSet {
            entries: vec![parse_name(raw)?],
        }),
        Value::Array(values) => {
            let mut entries = Vec::with_capacity(values.len());
            for value in values {
                let Some(raw) = value.as_str() else {
                    return Err(ParseError::Unsupported("id list entry".to_string()));
                };
                entries.push(parse_name(raw)?);
            }
            Ok(NameSet { entries })
        }
        _ => Err(ParseError::Unsupported("id".to_string())),
    }
}

fn parse_name(raw: &str) -> Result<NameEntry, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::Invalid("empty id".to_string()));
    }
    if let Some(tag) = raw.strip_prefix('#') {
        if tag.is_empty() {
            return Err(ParseError::Invalid("empty tag".to_string()));
        }
        return Ok(NameEntry::Tag(tag.to_string()));
    }
    Ok(NameEntry::Id(raw.to_string()))
}

fn require_keys(obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), ParseError> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ParseError::Unsupported(key.clone()));
        }
    }
    Ok(())
}

fn json_i32(value: &Value) -> Option<i32> {
    value.as_i64().and_then(|number| i32::try_from(number).ok())
}

fn type_path(raw: &str) -> &str {
    raw.strip_prefix("minecraft:").unwrap_or(raw)
}

/// Missing bounds are unbounded. Both ends are inclusive.
fn level_in_range(level: i32, min_level: Option<i32>, max_level: Option<i32>) -> bool {
    min_level.is_none_or(|min_level| level >= min_level)
        && max_level.is_none_or(|max_level| level <= max_level)
}

fn same_id(left: &str, right: &str) -> bool {
    split_id(left) == split_id(right)
}

fn split_id(id: &str) -> (&str, &str) {
    match id.split_once(':') {
        Some((namespace, path)) => (namespace, path),
        None => ("minecraft", id),
    }
}

fn tag_contains(category: RegistryKey, tag: &str, id: &str) -> bool {
    tag_values(category, tag).is_some_and(|values| values.iter().any(|value| same_id(value, id)))
}

fn tag_values(category: RegistryKey, tag: &str) -> Option<&'static [&'static str]> {
    let tag = tag.strip_prefix('#').unwrap_or(tag);
    if let Some(values) = get_tag_values(category, tag) {
        return Some(values);
    }
    if let Some((namespace, path)) = tag.split_once(':') {
        if namespace == "minecraft" {
            return get_tag_values(category, path);
        }
        return None;
    }
    get_tag_values(category, &format!("minecraft:{tag}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILK_TOUCH: &str = include_str!(
        "../../../../../assets/datapack/data/minecraft/predicate/tool/can_silk_touch.json"
    );
    const SHEARS: &str =
        include_str!("../../../../../assets/datapack/data/minecraft/predicate/tool/can_shear.json");
    const FAST_COOKING: &str = include_str!(
        "../../../../../assets/datapack/data/minecraft/predicate/block/fast_cooking.json"
    );
    const STORY_ROOT: &str =
        include_str!("../../../../../assets/datapack/data/minecraft/advancement/story/root.json");
    const MINE_STONE: &str = include_str!(
        "../../../../../assets/datapack/data/minecraft/advancement/story/mine_stone.json"
    );
    const FOLLOW_ENDER_EYE: &str = include_str!(
        "../../../../../assets/datapack/data/minecraft/advancement/story/follow_ender_eye.json"
    );

    #[test]
    fn silk_touch_predicate_matches_only_silk_touch_tools() {
        let predicate = parse_predicate_json(Some("minecraft:tool/can_silk_touch"), SILK_TOUCH)
            .expect("silk touch predicate");
        let with_silk = PredicateContext {
            item: Some(
                ItemSample::new("minecraft:diamond_pickaxe").with_enchantment("silk_touch", 1),
            ),
            ..PredicateContext::default()
        };
        let with_fortune = PredicateContext {
            item: Some(ItemSample::new("minecraft:diamond_pickaxe").with_enchantment("fortune", 3)),
            ..PredicateContext::default()
        };
        let bare = PredicateContext {
            item: Some(ItemSample::new("minecraft:diamond_pickaxe")),
            ..PredicateContext::default()
        };

        assert!(predicate.test(&with_silk));
        assert!(!predicate.test(&with_fortune));
        assert!(!predicate.test(&bare));
        assert!(!predicate.test(&PredicateContext::default()));
    }

    #[test]
    fn shears_predicate_matches_only_shears() {
        let predicate =
            parse_predicate_json(Some("minecraft:tool/can_shear"), SHEARS).expect("shears");
        assert!(predicate.test(&PredicateContext {
            item: Some(ItemSample::new("shears")),
            ..PredicateContext::default()
        }));
        assert!(!predicate.test(&PredicateContext {
            item: Some(ItemSample::new("minecraft:diamond_pickaxe")),
            ..PredicateContext::default()
        }));
    }

    #[test]
    fn unknown_predicate_type_is_skipped() {
        assert!(parse_predicate_json(Some("minecraft:block/fast_cooking"), FAST_COOKING).is_none());
    }

    #[test]
    fn story_item_predicates_match_id_and_tag() {
        let root: Value = serde_json::from_str(STORY_ROOT).expect("story root");
        let crafting = parse_item_predicates(
            Some("minecraft:story/root"),
            &root["criteria"]["crafting_table"]["conditions"]["items"],
        )
        .expect("crafting table item check");
        assert!(test_item_predicates(
            &crafting,
            &[ItemSample::new("minecraft:crafting_table")]
        ));
        assert!(!test_item_predicates(
            &crafting,
            &[ItemSample::new("minecraft:stick")]
        ));

        let stone: Value = serde_json::from_str(MINE_STONE).expect("mine stone");
        let materials = parse_item_predicates(
            Some("minecraft:story/mine_stone"),
            &stone["criteria"]["get_stone"]["conditions"]["items"],
        )
        .expect("stone tool materials");
        assert!(test_item_predicates(
            &materials,
            &[ItemSample::new("minecraft:cobblestone")]
        ));
        assert!(!test_item_predicates(
            &materials,
            &[ItemSample::new("minecraft:stone")]
        ));
    }

    #[test]
    fn story_location_predicate_matches_stronghold() {
        let advancement: Value = serde_json::from_str(FOLLOW_ENDER_EYE).expect("follow ender eye");
        let predicate = parse_predicate_value(
            Some("minecraft:story/follow_ender_eye"),
            &advancement["criteria"]["in_stronghold"]["conditions"]["player"],
        )
        .expect("stronghold location");

        assert!(predicate.test(&PredicateContext {
            structures: vec!["minecraft:stronghold".to_string()],
            ..PredicateContext::default()
        }));
        assert!(!predicate.test(&PredicateContext {
            structures: vec!["minecraft:village_plains".to_string()],
            ..PredicateContext::default()
        }));
        assert!(!predicate.test(&PredicateContext::default()));
    }

    #[test]
    fn embedded_vanilla_predicates_skip_unknown_types() {
        let mut registry = HashMap::new();
        let loaded = load_embedded_predicates(&mut registry);
        assert_eq!(loaded, 2);
        assert!(registry.contains_key("minecraft:tool/can_silk_touch"));
        assert!(registry.contains_key("minecraft:tool/can_shear"));
        assert!(!registry.contains_key("minecraft:block/fast_cooking"));
    }
}
