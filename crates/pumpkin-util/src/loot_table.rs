/// Conditions required for an entry or pool to be eligible for loot generation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LootCondition {
    #[default]
    None,
    SilkTouch,
    NoSilkTouch,
    Shears,
    SilkTouchOrShears,
    NoSilkTouchOrShears,
    SurvivesExplosion,
    KilledByPlayer,
    RandomChance {
        chance: f32,
    },
    RandomChanceWithEnchantedBonus {
        unenchanted_chance: f32,
        enchanted_chance_base: f32,
        enchanted_chance_per_level_above_first: f32,
    },
    TableBonus {
        chances: &'static [f32],
    },
    AllOf(&'static [Self]),
    AnyOf(&'static [Self]),
    EntityOnFire,
    DirectAttackerWeaponEnchantment(&'static str),
}

/// Bonus count formulas when tools have fortune or looting enchantments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LootBonusFormula {
    OreDrops,
    UniformBonusCount(i32),
    BinomialWithBonusCount { extra: i32, probability: f32 },
}

/// A single item entry inside a loot pool.
#[derive(Clone, Copy, Debug)]
pub struct LootEntry {
    /// Registry name of the item (e.g. `"minecraft:diamond"`).
    pub item: &'static str,
    /// Relative probability weight; higher values are more likely.
    pub weight: i32,
    /// Minimum stack size (inclusive).
    pub min_count: i32,
    /// Maximum stack size (inclusive).
    pub max_count: i32,
    /// Condition required for this entry to be eligible.
    pub condition: LootCondition,
    /// Bonus formula to apply with fortune / looting (if any).
    pub bonus_formula: Option<LootBonusFormula>,
    pub furnace_smelt: Option<LootCondition>,
}

/// One roll pool inside a loot table.
#[derive(Clone, Copy, Debug)]
pub struct LootPool {
    /// Item entries eligible for selection each roll.
    pub entries: &'static [LootEntry],
    /// Minimum number of roll attempts (inclusive).
    pub min_rolls: i32,
    /// Maximum number of roll attempts (inclusive).
    pub max_rolls: i32,
    /// Weight of the implicit "empty" (no item) outcome per roll.
    /// In vanilla this is modelled as a `minecraft:empty` entry with the given weight.
    pub empty_weight: i32,
    /// Condition required for this entire pool to run.
    pub condition: LootCondition,
}

/// A complete loot table consisting of one or more pools.
#[derive(Clone, Copy, Debug)]
pub struct LootTable {
    /// All pools to roll when generating loot for this table.
    pub pools: &'static [LootPool],
}

pub type ChestLootEntry = LootEntry;
pub type ChestLootPool = LootPool;
pub type ChestLootTable = LootTable;

/// One registry id, or a tag stored without the leading `#`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameEntry {
    Id(String),
    Tag(String),
}

/// A single id, a tag, or a list of either. Any entry matching is enough.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameSet {
    pub entries: Vec<NameEntry>,
}

/// An enchantment id or tag, plus an inclusive level range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnchantmentPredicate {
    pub enchantments: NameSet,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
}

/// Item id, item tag, and enchantment checks from an item predicate.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ItemPredicate {
    /// `None` accepts any item.
    pub items: Option<NameSet>,
    pub enchantments: Vec<EnchantmentPredicate>,
}

/// Location check. Story advancements only name a structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationPredicate {
    pub structures: Option<NameSet>,
}

/// Entity sub-predicates used by loot conditions and predicate files.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EntityPredicate {
    pub on_fire: Option<bool>,
    pub location: Option<LocationPredicate>,
}

/// Conditions required for an entry or pool to be eligible for dynamic loot generation.
///
/// Predicate files parse into this type. `Reference` holds a predicate id. `get_predicate`
/// resolves it. A-43 and A-44 are the callers of that lookup.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum DynamicLootCondition {
    #[default]
    None,
    SilkTouch,
    NoSilkTouch,
    Shears,
    SilkTouchOrShears,
    NoSilkTouchOrShears,
    SurvivesExplosion,
    KilledByPlayer,
    RandomChance {
        chance: f32,
    },
    RandomChanceWithEnchantedBonus {
        unenchanted_chance: f32,
        enchanted_chance_base: f32,
        enchanted_chance_per_level_above_first: f32,
    },
    TableBonus {
        chances: Box<[f32]>,
    },
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
    Inverted(Box<Self>),
    EntityProperties(EntityPredicate),
    EntityOnFire,
    DirectAttackerWeaponEnchantment(String),
    WeatherCheck {
        raining: Option<bool>,
        thundering: Option<bool>,
    },
    MatchTool(ItemPredicate),
    MatchBlock(NameSet),
    /// A predicate id, such as `minecraft:tool/can_silk_touch`.
    Reference(String),
}

impl From<LootCondition> for DynamicLootCondition {
    fn from(cond: LootCondition) -> Self {
        match cond {
            LootCondition::None => Self::None,
            LootCondition::SilkTouch => Self::SilkTouch,
            LootCondition::NoSilkTouch => Self::NoSilkTouch,
            LootCondition::Shears => Self::Shears,
            LootCondition::SilkTouchOrShears => Self::SilkTouchOrShears,
            LootCondition::NoSilkTouchOrShears => Self::NoSilkTouchOrShears,
            LootCondition::SurvivesExplosion => Self::SurvivesExplosion,
            LootCondition::KilledByPlayer => Self::KilledByPlayer,
            LootCondition::RandomChance { chance } => Self::RandomChance { chance },
            LootCondition::RandomChanceWithEnchantedBonus {
                unenchanted_chance,
                enchanted_chance_base,
                enchanted_chance_per_level_above_first,
            } => Self::RandomChanceWithEnchantedBonus {
                unenchanted_chance,
                enchanted_chance_base,
                enchanted_chance_per_level_above_first,
            },
            LootCondition::TableBonus { chances } => Self::TableBonus {
                chances: chances.to_vec().into_boxed_slice(),
            },
            LootCondition::AllOf(conditions) => {
                Self::AllOf(conditions.iter().copied().map(Self::from).collect())
            }
            LootCondition::AnyOf(conditions) => {
                Self::AnyOf(conditions.iter().copied().map(Self::from).collect())
            }
            LootCondition::EntityOnFire => Self::EntityOnFire,
            LootCondition::DirectAttackerWeaponEnchantment(tag) => {
                Self::DirectAttackerWeaponEnchantment(tag.to_string())
            }
        }
    }
}

/// A single item entry inside a dynamic loot pool.
#[derive(Clone, Debug)]
pub struct DynamicLootEntry {
    /// Registry name of the item (e.g. `"minecraft:diamond"`).
    pub item: String,
    /// Relative probability weight; higher values are more likely.
    pub weight: i32,
    /// Minimum stack size (inclusive).
    pub min_count: i32,
    /// Maximum stack size (inclusive).
    pub max_count: i32,
    /// Condition required for this entry to be eligible.
    pub condition: DynamicLootCondition,
    /// Bonus formula to apply with fortune / looting (if any).
    pub bonus_formula: Option<LootBonusFormula>,
}

impl From<LootEntry> for DynamicLootEntry {
    fn from(e: LootEntry) -> Self {
        Self {
            item: e.item.to_string(),
            weight: e.weight,
            min_count: e.min_count,
            max_count: e.max_count,
            condition: DynamicLootCondition::from(e.condition),
            bonus_formula: e.bonus_formula,
        }
    }
}

/// One roll pool inside a dynamic loot table.
#[derive(Clone, Debug)]
pub struct DynamicLootPool {
    /// Item entries eligible for selection each roll.
    pub entries: Vec<DynamicLootEntry>,
    /// Minimum number of roll attempts (inclusive).
    pub min_rolls: i32,
    /// Maximum number of roll attempts (inclusive).
    pub max_rolls: i32,
    /// Weight of the implicit "empty" (no item) outcome per roll.
    pub empty_weight: i32,
    /// Condition required for this entire pool to run.
    pub condition: DynamicLootCondition,
}

impl From<LootPool> for DynamicLootPool {
    fn from(p: LootPool) -> Self {
        Self {
            entries: p
                .entries
                .iter()
                .copied()
                .map(DynamicLootEntry::from)
                .collect(),
            min_rolls: p.min_rolls,
            max_rolls: p.max_rolls,
            empty_weight: p.empty_weight,
            condition: DynamicLootCondition::from(p.condition),
        }
    }
}

/// A complete dynamic loot table consisting of one or more pools.
#[derive(Clone, Debug, Default)]
pub struct DynamicLootTable {
    /// All pools to roll when generating loot for this table.
    pub pools: Vec<DynamicLootPool>,
}

impl From<&LootTable> for DynamicLootTable {
    fn from(t: &LootTable) -> Self {
        Self {
            pools: t.pools.iter().copied().map(DynamicLootPool::from).collect(),
        }
    }
}
