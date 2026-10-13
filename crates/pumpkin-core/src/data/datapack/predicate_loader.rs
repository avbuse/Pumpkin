//! Loads datapack predicates as [`DynamicLootCondition`].
//!
//! Predicate files and inline loot conditions are the same JSON. Unknown predicate
//! types are logged and left out of the registry. `get_predicate` is how loot tables
//! and advancements resolve a `minecraft:reference`. A-43 and A-44 are the callers.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use pumpkin_data::data_component_impl::EnchantmentsImpl;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{RegistryKey, get_tag_values};
use pumpkin_util::loot_table::{
    DynamicLootCondition, EnchantmentPredicate, EntityPredicate, ItemPredicate, LocationPredicate,
    NameEntry, NameSet,
};
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

/// How many nested `minecraft:reference` conditions are followed.
const MAX_PREDICATE_REFERENCE_DEPTH: u8 = 8;

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
    /// Tool for [`DynamicLootCondition::MatchTool`].
    pub item: Option<ItemSample>,
    /// Block for [`DynamicLootCondition::MatchBlock`].
    pub block_id: Option<String>,
    /// `this` entity on-fire flag.
    pub on_fire: bool,
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

/// Copies the id and enchantments off a tool stack.
#[must_use]
pub fn item_sample_from_stack(stack: &ItemStack) -> ItemSample {
    let enchantments = stack
        .get_data_component::<EnchantmentsImpl>()
        .map(|data| {
            data.enchantment
                .iter()
                .map(|(enchantment, level)| ItemEnchantment {
                    id: enchantment.name.to_string(),
                    level: *level,
                })
                .collect()
        })
        .unwrap_or_default();
    ItemSample {
        id: stack.item.registry_key.to_string(),
        enchantments,
    }
}

/// Returns whether `condition` holds for `ctx`.
///
/// `lookup` resolves [`DynamicLootCondition::Reference`]. Pass `get_predicate`.
/// Loot-only conditions such as `random_chance` return false here.
#[must_use]
pub fn test_predicate(
    condition: &DynamicLootCondition,
    ctx: &PredicateContext,
    lookup: &dyn Fn(&str) -> Option<Arc<DynamicLootCondition>>,
) -> bool {
    test_predicate_inner(condition, ctx, lookup, 0)
}

fn test_predicate_inner(
    condition: &DynamicLootCondition,
    ctx: &PredicateContext,
    lookup: &dyn Fn(&str) -> Option<Arc<DynamicLootCondition>>,
    depth: u8,
) -> bool {
    if depth > MAX_PREDICATE_REFERENCE_DEPTH {
        return false;
    }
    match condition {
        DynamicLootCondition::MatchTool(predicate) => ctx
            .item
            .as_ref()
            .is_some_and(|item| item_predicate_matches(predicate, item)),
        DynamicLootCondition::MatchBlock(blocks) => ctx
            .block_id
            .as_deref()
            .is_some_and(|id| name_matches(blocks, id, RegistryKey::Block)),
        DynamicLootCondition::EntityProperties(entity) => {
            entity_matches(entity, ctx.on_fire, &ctx.structures)
        }
        DynamicLootCondition::Reference(id) => lookup(id)
            .is_some_and(|resolved| test_predicate_inner(&resolved, ctx, lookup, depth + 1)),
        DynamicLootCondition::AllOf(terms) => terms
            .iter()
            .all(|term| test_predicate_inner(term, ctx, lookup, depth)),
        DynamicLootCondition::AnyOf(terms) => terms
            .iter()
            .any(|term| test_predicate_inner(term, ctx, lookup, depth)),
        DynamicLootCondition::Inverted(term) => !test_predicate_inner(term, ctx, lookup, depth),
        _ => false,
    }
}

fn item_predicate_matches(predicate: &ItemPredicate, item: &ItemSample) -> bool {
    if let Some(items) = &predicate.items
        && !name_matches(items, &item.id, RegistryKey::Item)
    {
        return false;
    }
    predicate
        .enchantments
        .iter()
        .all(|enchantment| enchantment_matches(enchantment, item))
}

fn enchantment_matches(predicate: &EnchantmentPredicate, item: &ItemSample) -> bool {
    item.enchantments.iter().any(|enchantment| {
        name_matches(
            &predicate.enchantments,
            &enchantment.id,
            RegistryKey::Enchantment,
        ) && level_in_range(enchantment.level, predicate.min_level, predicate.max_level)
    })
}

fn entity_matches(entity: &EntityPredicate, on_fire: bool, structures: &[String]) -> bool {
    if entity.on_fire.is_some_and(|expected| expected != on_fire) {
        return false;
    }
    entity
        .location
        .as_ref()
        .is_none_or(|location| location_matches(location, structures))
}

fn location_matches(location: &LocationPredicate, present: &[String]) -> bool {
    location.structures.as_ref().is_none_or(|structures| {
        present
            .iter()
            .any(|id| name_matches(structures, id, RegistryKey::WorldgenStructure))
    })
}

fn name_matches(set: &NameSet, id: &str, category: RegistryKey) -> bool {
    set.entries.iter().any(|entry| match entry {
        NameEntry::Id(expected) => same_id(expected, id),
        NameEntry::Tag(tag) => tag_contains(category, tag, id),
    })
}

/// Every predicate must match some stack. One stack may satisfy more than one predicate.
#[must_use]
pub fn test_item_predicates(predicates: &[ItemPredicate], items: &[ItemSample]) -> bool {
    let mut pending: Vec<&ItemPredicate> = predicates.iter().collect();
    for item in items {
        pending.retain(|predicate| !item_predicate_matches(predicate, item));
        if pending.is_empty() {
            return true;
        }
    }
    pending.is_empty()
}

/// Parses a predicate JSON document. Unknown types are logged and return `None`.
#[must_use]
pub fn parse_predicate_json(id: Option<&str>, json: &str) -> Option<DynamicLootCondition> {
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
pub fn parse_predicate_value(id: Option<&str>, value: &Value) -> Option<DynamicLootCondition> {
    finish(id, parse_condition_inner(value))
}

/// Parses one loot condition or predicate reference without logging.
///
/// Unknown checks become [`DynamicLootCondition::None`], which passes. Predicate
/// files use [`parse_predicate_value`] so the same JSON is skipped and logged instead.
#[must_use]
pub fn parse_loot_condition(value: &Value) -> DynamicLootCondition {
    if let Some(id) = value.as_str() {
        return DynamicLootCondition::Reference(canonical_predicate_id(id));
    }
    parse_condition_inner(value).unwrap_or_default()
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
    registry: &mut HashMap<String, Arc<DynamicLootCondition>, S>,
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
    registry: &mut HashMap<String, Arc<DynamicLootCondition>, S>,
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
    registry: &mut HashMap<String, Arc<DynamicLootCondition>, S>,
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
    registry: &mut HashMap<String, Arc<DynamicLootCondition>, S>,
    id: &str,
    predicate: Option<DynamicLootCondition>,
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

fn parse_condition_inner(value: &Value) -> Result<DynamicLootCondition, ParseError> {
    if let Some(id) = value.as_str() {
        return Ok(DynamicLootCondition::Reference(canonical_predicate_id(id)));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    let type_name = obj
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| obj.get("condition").and_then(Value::as_str))
        .ok_or_else(|| ParseError::Invalid("missing type".to_string()))?;
    match type_path(type_name) {
        "survives_explosion" => Ok(DynamicLootCondition::SurvivesExplosion),
        "killed_by_player" => Ok(DynamicLootCondition::KilledByPlayer),
        "random_chance" => Ok(DynamicLootCondition::RandomChance {
            chance: json_f32(obj.get("chance")).unwrap_or(1.0),
        }),
        "random_chance_with_enchanted_bonus" => Ok(parse_enchanted_bonus(obj)),
        "table_bonus" => Ok(DynamicLootCondition::TableBonus {
            chances: parse_f32_list(obj.get("chances")).into_boxed_slice(),
        }),
        "inverted" => obj
            .get("term")
            .map_or(Ok(DynamicLootCondition::None), |term| {
                Ok(DynamicLootCondition::Inverted(Box::new(
                    parse_condition_inner(term)?,
                )))
            }),
        "any_of" => parse_term_list(obj, DynamicLootCondition::AnyOf),
        "all_of" => parse_term_list(obj, DynamicLootCondition::AllOf),
        "weather_check" => Ok(DynamicLootCondition::WeatherCheck {
            raining: obj.get("raining").and_then(Value::as_bool),
            thundering: obj.get("thundering").and_then(Value::as_bool),
        }),
        "match_tool" => parse_match_tool(obj),
        "match_block" => parse_match_block(obj),
        "reference" => parse_reference(obj),
        "entity_properties" => parse_entity_properties(obj),
        _ => Err(ParseError::UnknownType(type_name.to_string())),
    }
}

fn parse_enchanted_bonus(obj: &Map<String, Value>) -> DynamicLootCondition {
    let unenchanted = json_f32(obj.get("unenchanted_chance")).unwrap_or(0.0);
    let enchanted_base = json_f32(
        obj.get("enchanted_chance")
            .or_else(|| obj.get("enchanted_chance_base")),
    )
    .unwrap_or(unenchanted);
    let per_level = json_f32(obj.get("enchanted_chance_per_level_above_first")).unwrap_or(0.0);
    DynamicLootCondition::RandomChanceWithEnchantedBonus {
        unenchanted_chance: unenchanted,
        enchanted_chance_base: enchanted_base,
        enchanted_chance_per_level_above_first: per_level,
    }
}

fn parse_term_list(
    obj: &Map<String, Value>,
    wrap: fn(Vec<DynamicLootCondition>) -> DynamicLootCondition,
) -> Result<DynamicLootCondition, ParseError> {
    let Some(terms) = obj.get("terms").and_then(Value::as_array) else {
        return Ok(DynamicLootCondition::None);
    };
    let mut parsed = Vec::with_capacity(terms.len());
    for term in terms {
        parsed.push(parse_condition_inner(term)?);
    }
    Ok(wrap(parsed))
}

fn parse_match_tool(obj: &Map<String, Value>) -> Result<DynamicLootCondition, ParseError> {
    require_keys(obj, &["type", "condition", "predicate"])?;
    let item = match obj.get("predicate") {
        Some(predicate) => parse_item_predicate_inner(predicate)?,
        None => ItemPredicate::default(),
    };
    Ok(DynamicLootCondition::MatchTool(item))
}

fn parse_match_block(obj: &Map<String, Value>) -> Result<DynamicLootCondition, ParseError> {
    require_keys(obj, &["type", "condition", "blocks"])?;
    let blocks = parse_name_set(
        obj.get("blocks")
            .ok_or_else(|| ParseError::Invalid("missing blocks".to_string()))?,
    )?;
    Ok(DynamicLootCondition::MatchBlock(blocks))
}

fn parse_reference(obj: &Map<String, Value>) -> Result<DynamicLootCondition, ParseError> {
    require_keys(obj, &["type", "condition", "name"])?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::Invalid("missing name".to_string()))?;
    Ok(DynamicLootCondition::Reference(canonical_predicate_id(
        name,
    )))
}

fn parse_entity_properties(obj: &Map<String, Value>) -> Result<DynamicLootCondition, ParseError> {
    require_keys(obj, &["type", "condition", "entity", "predicate"])?;
    if let Some(entity) = obj.get("entity").and_then(Value::as_str)
        && entity != "this"
    {
        return Err(ParseError::Unsupported(format!("entity target {entity}")));
    }
    let predicate = obj
        .get("predicate")
        .ok_or_else(|| ParseError::Invalid("missing predicate".to_string()))?;
    Ok(DynamicLootCondition::EntityProperties(
        parse_entity_predicate(predicate)?,
    ))
}

fn parse_entity_predicate(value: &Value) -> Result<EntityPredicate, ParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(obj, &["minecraft:location", "minecraft:flags", "flags"])?;
    let location = match obj.get("minecraft:location") {
        Some(location) => Some(parse_location(location)?),
        None => None,
    };
    let on_fire = parse_on_fire(obj)?;
    Ok(EntityPredicate { on_fire, location })
}

fn parse_on_fire(obj: &Map<String, Value>) -> Result<Option<bool>, ParseError> {
    let Some(flags) = obj.get("minecraft:flags").or_else(|| obj.get("flags")) else {
        return Ok(None);
    };
    let flags = flags
        .as_object()
        .ok_or_else(|| ParseError::Invalid("expected an object".to_string()))?;
    require_keys(flags, &["is_on_fire"])?;
    Ok(flags.get("is_on_fire").and_then(Value::as_bool))
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

fn canonical_predicate_id(id: &str) -> String {
    if id.contains(':') {
        id.to_string()
    } else {
        format!("minecraft:{id}")
    }
}

fn json_f32(value: Option<&Value>) -> Option<f32> {
    value.and_then(Value::as_f64).map(|number| number as f32)
}

fn parse_f32_list(value: Option<&Value>) -> Vec<f32> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| json_f32(Some(value)))
                .collect()
        })
        .unwrap_or_default()
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

    fn no_lookup(_id: &str) -> Option<Arc<DynamicLootCondition>> {
        None
    }

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

        assert!(test_predicate(&predicate, &with_silk, &no_lookup));
        assert!(!test_predicate(&predicate, &with_fortune, &no_lookup));
        assert!(!test_predicate(&predicate, &bare, &no_lookup));
        assert!(!test_predicate(
            &predicate,
            &PredicateContext::default(),
            &no_lookup
        ));
    }

    #[test]
    fn shears_predicate_matches_only_shears() {
        let predicate =
            parse_predicate_json(Some("minecraft:tool/can_shear"), SHEARS).expect("shears");
        assert!(test_predicate(
            &predicate,
            &PredicateContext {
                item: Some(ItemSample::new("shears")),
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(!test_predicate(
            &predicate,
            &PredicateContext {
                item: Some(ItemSample::new("minecraft:diamond_pickaxe")),
                ..PredicateContext::default()
            },
            &no_lookup
        ));
    }

    #[test]
    fn fast_cooking_matches_smoker_and_blast_furnace() {
        let predicate = parse_predicate_json(Some("minecraft:block/fast_cooking"), FAST_COOKING)
            .expect("fast cooking");
        assert!(matches!(predicate, DynamicLootCondition::MatchBlock(_)));
        assert!(test_predicate(
            &predicate,
            &PredicateContext {
                block_id: Some("minecraft:smoker".to_string()),
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(test_predicate(
            &predicate,
            &PredicateContext {
                block_id: Some("blast_furnace".to_string()),
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(!test_predicate(
            &predicate,
            &PredicateContext {
                block_id: Some("minecraft:furnace".to_string()),
                ..PredicateContext::default()
            },
            &no_lookup
        ));
    }

    #[test]
    fn unknown_predicate_type_is_skipped() {
        assert!(
            parse_predicate_json(
                Some("test:unknown"),
                r#"{ "type": "minecraft:not_a_real_predicate" }"#,
            )
            .is_none()
        );
    }

    #[test]
    fn reference_resolves_through_the_predicate_registry() {
        let mut registry = HashMap::new();
        load_embedded_predicates(&mut registry);
        let reference =
            DynamicLootCondition::Reference("minecraft:tool/can_silk_touch".to_string());
        let lookup = |id: &str| registry.get(id).cloned();
        let with_silk = PredicateContext {
            item: Some(
                ItemSample::new("minecraft:diamond_pickaxe").with_enchantment("silk_touch", 1),
            ),
            ..PredicateContext::default()
        };
        let bare = PredicateContext {
            item: Some(ItemSample::new("minecraft:diamond_pickaxe")),
            ..PredicateContext::default()
        };

        assert!(test_predicate(&reference, &with_silk, &lookup));
        assert!(!test_predicate(&reference, &bare, &lookup));
        assert!(!test_predicate(
            &DynamicLootCondition::Reference("minecraft:missing".to_string()),
            &with_silk,
            &lookup
        ));
    }

    #[test]
    fn entity_on_fire_flag_is_a_loot_condition() {
        let json = r#"{
            "condition": "minecraft:entity_properties",
            "entity": "this",
            "predicate": { "flags": { "is_on_fire": true } }
        }"#;
        let condition = parse_predicate_json(Some("test:on_fire"), json).expect("on fire");
        assert!(test_predicate(
            &condition,
            &PredicateContext {
                on_fire: true,
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(!test_predicate(
            &condition,
            &PredicateContext::default(),
            &no_lookup
        ));
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

        assert!(test_predicate(
            &predicate,
            &PredicateContext {
                structures: vec!["minecraft:stronghold".to_string()],
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(!test_predicate(
            &predicate,
            &PredicateContext {
                structures: vec!["minecraft:village_plains".to_string()],
                ..PredicateContext::default()
            },
            &no_lookup
        ));
        assert!(!test_predicate(
            &predicate,
            &PredicateContext::default(),
            &no_lookup
        ));
    }

    #[test]
    fn embedded_vanilla_predicates_load() {
        let mut registry = HashMap::new();
        let loaded = load_embedded_predicates(&mut registry);
        assert_eq!(loaded, 3);
        assert!(registry.contains_key("minecraft:tool/can_silk_touch"));
        assert!(registry.contains_key("minecraft:tool/can_shear"));
        assert!(matches!(
            registry
                .get("minecraft:block/fast_cooking")
                .map(Arc::as_ref),
            Some(DynamicLootCondition::MatchBlock(_))
        ));
    }
}
