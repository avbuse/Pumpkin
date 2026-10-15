use crate::entity::EntityBase;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{
    attributes::Attributes,
    particle::Particle,
    sound::{Sound, SoundCategory},
};
use pumpkin_util::math::vector3::Vector3;

use crate::{entity::Entity, world::World};

/// Game ticks in one vanilla second. Attack cooldown is counted in ticks.
/// Vanilla multiplies by this literal, not by the server's configured tick rate.
pub const ATTACK_STRENGTH_TICKS_PER_SECOND: f64 = 20.0;
/// Added to the attack-strength ticker when a hit is resolved, so a click
/// lands part-way through the current tick.
pub const ATTACK_STRENGTH_ADJUST_TICKS: f32 = 0.5;
/// A hit counts as fully charged when its strength scale is above this.
pub const FULL_STRENGTH_ATTACK_THRESHOLD: f32 = 0.9;
/// Constant term of the cooldown damage scale.
pub const BASE_DAMAGE_SCALE_CONSTANT: f32 = 0.2;
/// Quadratic term of the cooldown damage scale.
pub const BASE_DAMAGE_SCALE_QUADRATIC: f32 = 0.8;
/// Critical hits multiply the cooldown-scaled base damage by this.
pub const CRITICAL_HIT_MULTIPLIER: f32 = 1.5;
/// Added to extra knockback after the enchantment bonus has been halved.
pub const SPRINT_KNOCKBACK_BONUS: f64 = 0.5;
/// Sweep damage before the sweeping-damage ratio is applied.
pub const SWEEP_BASE_DAMAGE: f32 = 1.0;
/// Squared distance from the attacker inside which a sweep can connect.
pub const SWEEP_REACH_DISTANCE_SQ: f64 = 9.0;
/// Horizontal speed must stay under `movement_speed *` this to sweep.
pub const SWEEP_SPEED_FACTOR: f64 = 2.5;
/// Knockback power a sweep applies to each extra target.
pub const SWEEP_KNOCKBACK: f64 = 0.4;
/// Fall distance past which a mace adds smash damage. Elytra flight blocks it.
pub const MACE_SMASH_FALL_THRESHOLD: f32 = 1.5;
/// Fall distance past which a smash uses the heavy ground sound.
pub const MACE_SMASH_HEAVY_FALL: f32 = 5.0;
/// Blocks of fall that each add 4 smash damage.
const MACE_SMASH_FIRST_FALL_BAND: f64 = 3.0;
/// Blocks of fall, after the first band, that each add 2 smash damage.
const MACE_SMASH_SECOND_FALL_BAND: f64 = 8.0;
const MACE_SMASH_FIRST_BAND_PER_BLOCK: f64 = 4.0;
const MACE_SMASH_SECOND_BAND_PER_BLOCK: f64 = 2.0;
/// Damage added for each block of fall past the second band.
const MACE_SMASH_REMAINING_PER_BLOCK: f64 = 1.0;
/// Ticks of hurt invulnerability set by a full hit.
pub const HURT_COOLDOWN_TICKS: i32 = 20;
/// While the cooldown is above this, a smaller hit is ignored.
pub const HURT_COOLDOWN_PARTIAL_THRESHOLD: i32 = 10;
/// Hunger exhaustion applied by a landed melee hit.
pub const MELEE_HIT_EXHAUSTION: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    Knockback,
    Critical,
    Sweeping,
    Strong,
    Weak,
    MaceSmash,
}

/// Ticks required to fully charge, `(float)(1.0 / attack_speed * ticks_per_second)`.
#[must_use]
pub fn attack_strength_delay(attack_speed: f64, ticks_per_second: f64) -> f32 {
    (1.0 / attack_speed * ticks_per_second) as f32
}

/// `(ticks + adjust) / delay`, clamped to `0..=1`.
#[must_use]
pub fn attack_strength_scale(
    ticks_since_attack: u32,
    adjust_ticks: f32,
    attack_speed: f64,
    ticks_per_second: f64,
) -> f32 {
    if attack_speed <= 0.0 {
        return 0.0;
    }
    let delay = attack_strength_delay(attack_speed, ticks_per_second);
    if delay <= 0.0 {
        return 1.0;
    }
    ((ticks_since_attack as f32 + adjust_ticks) / delay).clamp(0.0, 1.0)
}

/// Attack charge using vanilla's fixed 20 ticks per second.
#[must_use]
pub fn attack_cooldown_scale(ticks_since_attack: u32, adjust_ticks: f32, attack_speed: f64) -> f32 {
    attack_strength_scale(
        ticks_since_attack,
        adjust_ticks,
        attack_speed,
        ATTACK_STRENGTH_TICKS_PER_SECOND,
    )
}

/// `0.2 + scale² * 0.8`. A full charge returns 1.
#[must_use]
pub fn base_damage_scale_factor(scale: f32) -> f32 {
    BASE_DAMAGE_SCALE_CONSTANT + scale * scale * BASE_DAMAGE_SCALE_QUADRATIC
}

/// Inputs for one player melee swing, after attributes and enchantments are read.
#[derive(Debug, Clone, Copy)]
pub struct MeleeHitInput {
    /// `ATTACK_DAMAGE`, including the held weapon and strength or weakness.
    pub attack_damage: f64,
    /// Result of [`attack_strength_scale`]. Bedrock passes 1.
    pub attack_strength_scale: f32,
    /// Extra damage from sharpness, smite, and bane, before cooldown scaling.
    pub enchantment_bonus: f32,
    /// Added to the cooldown-scaled base before a critical hit multiplies it.
    /// A mace smash uses this. Enchantment damage does not.
    pub bonus_before_crit: f32,
    /// Physical crit conditions already hold: falling, not climbing, not wet,
    /// not blind, not riding, the target is living, and the attacker is not sprinting.
    pub critical_allowed: bool,
    pub sprinting: bool,
}

/// Damage split the way a player swing applies it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeHit {
    /// Cooldown-scaled attribute damage, after the critical multiplier.
    pub base_damage: f32,
    /// Enchantment bonus multiplied by the strength scale, not by a crit.
    pub magic_boost: f32,
    pub total_damage: f32,
    pub attack_strength_scale: f32,
    pub full_strength: bool,
    pub critical: bool,
    pub sprint_knockback: bool,
}

/// Scales attribute damage by the attack cooldown, then crits that portion
/// and adds the enchantment bonus scaled only by the strength scale.
#[must_use]
pub fn resolve_melee_hit(input: &MeleeHitInput) -> MeleeHit {
    let scale = input.attack_strength_scale.clamp(0.0, 1.0);
    let mut base_damage = (input.attack_damage as f32) * base_damage_scale_factor(scale);
    let magic_boost = scale * input.enchantment_bonus;
    let full_strength = scale > FULL_STRENGTH_ATTACK_THRESHOLD;
    let critical = input.critical_allowed && full_strength;
    base_damage += input.bonus_before_crit;
    if critical {
        base_damage *= CRITICAL_HIT_MULTIPLIER;
    }
    MeleeHit {
        base_damage,
        magic_boost,
        total_damage: base_damage + magic_boost,
        attack_strength_scale: scale,
        full_strength,
        critical,
        sprint_knockback: input.sprinting && full_strength,
    }
}

/// State checked for a falling melee crit.
#[derive(Debug, Clone, Copy)]
pub struct CriticalAttack {
    pub fall_distance: f32,
    pub on_ground: bool,
    pub on_climbable: bool,
    pub in_water: bool,
    pub blindness: bool,
    pub passenger: bool,
    pub target_is_living: bool,
    pub sprinting: bool,
}

/// Falling melee crit. Sprinting, blindness, water, climbing, and riding block it.
#[must_use]
pub const fn can_critical_attack(attack: &CriticalAttack) -> bool {
    attack.fall_distance > 0.0
        && !attack.on_ground
        && !attack.on_climbable
        && !attack.in_water
        && !attack.blindness
        && !attack.passenger
        && attack.target_is_living
        && !attack.sprinting
}

/// State checked for a sword sweep.
#[derive(Debug, Clone, Copy)]
pub struct SweepAttack {
    pub full_strength: bool,
    pub critical: bool,
    pub sprint_knockback: bool,
    pub on_ground: bool,
    pub horizontal_speed_sq: f64,
    pub movement_speed: f64,
    pub holding_sword: bool,
}

/// A sword sweep: full charge, on the ground, not a crit, not a sprint hit,
/// and horizontal speed under `movement_speed * 2.5`.
#[must_use]
pub fn is_sweep_attack(attack: &SweepAttack) -> bool {
    if attack.full_strength
        && !attack.critical
        && !attack.sprint_knockback
        && attack.on_ground
        && attack.holding_sword
    {
        let max_speed = attack.movement_speed * SWEEP_SPEED_FACTOR;
        attack.horizontal_speed_sq < max_speed * max_speed
    } else {
        false
    }
}

/// `1 + sweeping_damage_ratio * base_damage`. `base_damage` is post-crit and
/// does not include the enchantment bonus.
#[must_use]
pub fn sweep_damage(base_damage: f32, sweeping_ratio: f64) -> f32 {
    SWEEP_BASE_DAMAGE + (sweeping_ratio as f32) * base_damage
}

/// Enchantment bonus on a sweep is added to the sweep base, then multiplied
/// by the attack strength scale.
#[must_use]
pub fn enchanted_sweep_damage(sweep_base: f32, enchantment_bonus: f32, scale: f32) -> f32 {
    (sweep_base + enchantment_bonus) * scale
}

/// Whether a nearby living entity is inside the sweep. The attacker and the
/// primary target are never hit.
#[must_use]
pub fn sweep_hits_entity(
    attacker_id: i32,
    primary_id: i32,
    other_id: i32,
    allied: bool,
    marker_armor_stand: bool,
    distance_sq: f64,
) -> bool {
    other_id != attacker_id
        && other_id != primary_id
        && !allied
        && !marker_armor_stand
        && distance_sq < SWEEP_REACH_DISTANCE_SQ
}

/// Power passed to living knockback. The attack-knockback attribute and the
/// enchantment bonus are halved together; a sprint hit then adds `0.5`.
#[must_use]
pub fn extra_knockback_power(attack_knockback: f64, enchant_knockback: f64, sprint: bool) -> f64 {
    let mut power = f64::midpoint(attack_knockback, enchant_knockback);
    if sprint {
        power += SPRINT_KNOCKBACK_BONUS;
    }
    power
}

/// Result of comparing a new hit with the hurt cooldown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HurtCooldownResult {
    /// Pre-armor damage to run through armor and magic. On a partial hit this
    /// is only the amount above the previous hit.
    pub damage_to_apply: f32,
    /// New stored hit, still the pre-armor incoming amount.
    pub last_hurt: f32,
    pub cooldown_ticks: i32,
    /// A full hit plays the hurt sound and the default knockback.
    pub full_hit: bool,
}

/// Hurt invulnerability. `None` means the hit is no stronger than the one
/// still in the cooldown window, so it deals nothing.
#[must_use]
pub fn resolve_hurt_cooldown(
    incoming: f32,
    cooldown_ticks: i32,
    last_hurt: f32,
    bypasses_cooldown: bool,
) -> Option<HurtCooldownResult> {
    if cooldown_ticks > HURT_COOLDOWN_PARTIAL_THRESHOLD && !bypasses_cooldown {
        if incoming <= last_hurt {
            return None;
        }
        return Some(HurtCooldownResult {
            damage_to_apply: incoming - last_hurt,
            last_hurt: incoming,
            cooldown_ticks,
            full_hit: false,
        });
    }
    Some(HurtCooldownResult {
        damage_to_apply: incoming,
        last_hurt: incoming,
        cooldown_ticks: HURT_COOLDOWN_TICKS,
        full_hit: true,
    })
}

/// Creative and spectator abilities block ordinary damage. Kill and void still land.
#[must_use]
pub const fn invulnerable_ability_blocks(invulnerable: bool, is_kill: bool, is_void: bool) -> bool {
    invulnerable && !is_kill && !is_void
}

/// Scales a knockback `strength` by a living entity's knockback resistance,
/// mirroring vanilla `LivingEntity.knockback`: `strength *= 1.0 - resistance`.
/// A resistance of 1.0 (iron golem, warden, ...) cancels the knockback entirely.
pub fn knockback_after_resistance(strength: f64, resistance: f64) -> f64 {
    strength * (1.0 - resistance)
}

pub fn handle_knockback(attacker: &Entity, victim: &dyn EntityBase, strength: f64) {
    let resistance = victim.get_living_entity().map_or(0.0, |living| {
        living.get_attribute_value(&Attributes::KNOCKBACK_RESISTANCE)
    });
    let strength = knockback_after_resistance(strength * 0.5, resistance);

    if strength > 0.0 {
        let yaw = attacker.yaw.load();
        victim.get_entity().knockback(
            strength,
            f64::from((yaw.to_radians()).sin()),
            f64::from(-(yaw.to_radians()).cos()),
        );
    }

    let velocity = attacker.velocity.load();
    attacker.velocity.store(velocity.multiply(0.6, 1.0, 0.6));
}

pub fn spawn_sweep_particle(attacker_entity: &Entity, world: &World) {
    let yaw = attacker_entity.yaw.load();
    let dx = -f64::from(yaw.to_radians().sin());
    let dz = f64::from(yaw.to_radians().cos());
    let pos = attacker_entity.pos.load();
    let body_y = f64::from(attacker_entity.height()).mul_add(0.5, pos.y);

    world.spawn_particle(
        Vector3::new(pos.x + dx, body_y, pos.z + dz),
        Vector3::new(dx as f32, 0.0, dz as f32),
        0.0,
        0,
        Particle::SweepAttack,
    );
}

/// A mace smash connects when the fall is past the threshold and the attacker
/// is not gliding.
#[must_use]
pub fn can_mace_smash(fall_distance: f32, fall_flying: bool) -> bool {
    fall_distance > MACE_SMASH_FALL_THRESHOLD && !fall_flying
}

/// Smash damage from fall distance, plus a per-block density enchantment.
/// The first three blocks add 4 each, the next five add 2, and the rest add 1.
#[must_use]
pub fn mace_smash_bonus(fall_distance: f64, density_per_block: f64) -> f32 {
    let first_band = MACE_SMASH_FIRST_BAND_PER_BLOCK * MACE_SMASH_FIRST_FALL_BAND;
    let height = if fall_distance <= MACE_SMASH_FIRST_FALL_BAND {
        MACE_SMASH_FIRST_BAND_PER_BLOCK * fall_distance
    } else if fall_distance <= MACE_SMASH_SECOND_FALL_BAND {
        first_band + MACE_SMASH_SECOND_BAND_PER_BLOCK * (fall_distance - MACE_SMASH_FIRST_FALL_BAND)
    } else {
        let second_band = MACE_SMASH_SECOND_BAND_PER_BLOCK
            * (MACE_SMASH_SECOND_FALL_BAND - MACE_SMASH_FIRST_FALL_BAND);
        first_band
            + second_band
            + MACE_SMASH_REMAINING_PER_BLOCK * (fall_distance - MACE_SMASH_SECOND_FALL_BAND)
    };
    (height + density_per_block * fall_distance) as f32
}

pub fn player_attack_sound(pos: &Vector3<f64>, world: &World, attack_type: AttackType) {
    match attack_type {
        AttackType::Knockback => {
            world.play_sound(
                Sound::EntityPlayerAttackKnockback,
                SoundCategory::Players,
                pos,
            );
        }
        AttackType::Critical => {
            world.play_sound(Sound::EntityPlayerAttackCrit, SoundCategory::Players, pos);
        }
        AttackType::Sweeping => {
            world.play_sound(Sound::EntityPlayerAttackSweep, SoundCategory::Players, pos);
        }
        AttackType::Strong => {
            world.play_sound(Sound::EntityPlayerAttackStrong, SoundCategory::Players, pos);
        }
        AttackType::Weak => {
            world.play_sound(Sound::EntityPlayerAttackWeak, SoundCategory::Players, pos);
        }
        AttackType::MaceSmash => {
            world.play_sound(Sound::ItemMaceSmashAir, SoundCategory::Players, pos);
        }
    }
}

/// Combat calculation rules mirroring vanilla `net.minecraft.world.damagesource.CombatRules`.
pub struct CombatRules;

impl CombatRules {
    pub const MAX_ARMOR: f32 = 20.0;
    pub const ARMOR_PROTECTION_DIVIDER: f32 = 25.0;
    pub const BASE_ARMOR_TOUGHNESS: f32 = 2.0;
    pub const MIN_ARMOR_RATIO: f32 = 0.2;

    /// Calculates damage after armor reduction, mirroring vanilla `CombatRules.getDamageAfterAbsorb`.
    #[must_use]
    pub fn get_damage_after_absorb(
        damage: f32,
        total_armor: f32,
        armor_toughness: f32,
        breach_level: u32,
    ) -> f32 {
        let toughness = Self::BASE_ARMOR_TOUGHNESS + armor_toughness / 4.0;
        let real_armor = (total_armor - damage / toughness)
            .clamp(total_armor * Self::MIN_ARMOR_RATIO, Self::MAX_ARMOR);
        let mut armor_fraction = real_armor / Self::ARMOR_PROTECTION_DIVIDER;

        if breach_level > 0 {
            let mut effectiveness = 1.0f32;
            pumpkin_data::Enchantment::BREACH
                .modify_armor_effectiveness(breach_level as i32, &mut effectiveness);
            armor_fraction = (armor_fraction * effectiveness.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        }

        let damage_multiplier = 1.0 - armor_fraction;
        damage * damage_multiplier
    }

    /// Calculates damage after magic/enchantment armor reduction, mirroring vanilla `CombatRules.getDamageAfterMagicAbsorb`.
    #[must_use]
    pub fn get_damage_after_magic_absorb(damage: f32, total_magic_armor: f32) -> f32 {
        let real_armor = total_magic_armor.clamp(0.0, Self::MAX_ARMOR);
        damage * (1.0 - real_armor / Self::ARMOR_PROTECTION_DIVIDER)
    }
}

pub const RESET_DAMAGE_STATUS_TIME: i64 = 100;
pub const RESET_COMBAT_STATUS_TIME: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallLocation {
    Generic,
    Ladder,
    Vines,
    WeepingVines,
    TwistingVines,
    Scaffolding,
    OtherClimbable,
    Water,
}

impl FallLocation {
    #[must_use]
    pub const fn language_key(self) -> &'static str {
        match self {
            Self::Generic => "death.fell.accident.generic",
            Self::Ladder => "death.fell.accident.ladder",
            Self::Vines => "death.fell.accident.vines",
            Self::WeepingVines => "death.fell.accident.weeping_vines",
            Self::TwistingVines => "death.fell.accident.twisting_vines",
            Self::Scaffolding => "death.fell.accident.scaffolding",
            Self::OtherClimbable => "death.fell.accident.other_climbable",
            Self::Water => "death.fell.accident.water",
        }
    }

    pub fn get_current_fall_location(
        living: &crate::entity::living::LivingEntity,
        world: &World,
    ) -> Self {
        let pos = living
            .climbing_pos
            .load()
            .unwrap_or_else(|| living.entity.block_pos.load());
        let block = world.get_block(&pos);
        if block == &pumpkin_data::Block::LADDER
            || block.has_tag(&pumpkin_data::tag::Block::MINECRAFT_TRAPDOORS)
        {
            Self::Ladder
        } else if block == &pumpkin_data::Block::VINE {
            Self::Vines
        } else if block == &pumpkin_data::Block::WEEPING_VINES
            || block == &pumpkin_data::Block::WEEPING_VINES_PLANT
        {
            Self::WeepingVines
        } else if block == &pumpkin_data::Block::TWISTING_VINES
            || block == &pumpkin_data::Block::TWISTING_VINES_PLANT
        {
            Self::TwistingVines
        } else if block == &pumpkin_data::Block::SCAFFOLDING {
            Self::Scaffolding
        } else if living.entity.is_in_water() {
            Self::Water
        } else if block.has_tag(&pumpkin_data::tag::Block::MINECRAFT_CLIMBABLE) {
            Self::OtherClimbable
        } else {
            Self::Generic
        }
    }
}

#[derive(Clone)]
pub struct CombatEntry {
    pub damage_type: pumpkin_data::damage::DamageType,
    pub damage: f32,
    pub fall_location: Option<FallLocation>,
    pub fall_distance: f32,
    pub timestamp: i64,
    pub source_id: Option<i32>,
    pub attacker_id: Option<i32>,
    pub attacker_name: Option<pumpkin_util::text::TextComponent>,
    pub attacker_item_name: Option<pumpkin_util::text::TextComponent>,
    pub attacker_is_living: bool,
    pub attacker_is_player: bool,
}

#[derive(Default)]
pub struct CombatTracker {
    entries: Vec<CombatEntry>,
    last_damage_time: i64,
    combat_start_time: i64,
    combat_end_time: i64,
    in_combat: bool,
    taking_damage: bool,
}

impl CombatTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_damage(
        &mut self,
        current_tick: i64,
        is_alive: bool,
        fall_distance: f32,
        fall_location: FallLocation,
        damage_type: pumpkin_data::damage::DamageType,
        damage: f32,
        source: Option<&dyn EntityBase>,
        cause: Option<&dyn EntityBase>,
    ) {
        self.recheck_status(current_tick, is_alive);

        let attacker = cause.or(source);
        let attacker_id = attacker.map(|e| e.get_entity().entity_id);
        let attacker_name = attacker.map(super::EntityBase::get_display_name);
        let attacker_is_living = attacker.is_some_and(|e| e.get_living_entity().is_some());
        let attacker_is_player = attacker.is_some_and(|e| {
            e.get_entity().entity_type == &pumpkin_data::entity::EntityType::PLAYER
        });

        // Check for custom named item in attacker's main hand
        let attacker_item_name =
            attacker.and_then(|e| {
                if let Some(player) = e.get_player() {
                    let hand_stack = player
                        .inventory()
                        .get_stack_in_hand(pumpkin_util::Hand::Right);
                    if !hand_stack.is_empty()
                    && let Some(custom_name) = hand_stack
                        .get_data_component::<pumpkin_data::data_component_impl::CustomNameImpl>()
                {
                    return Some(custom_name.name.clone());
                }
                } else if let Some(living) = e.get_living_entity() {
                    let equip_guard = living
                        .entity_equipment
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(stack) = equip_guard
                    .equipment
                    .get(&pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND)
                    && !stack.is_empty()
                    && let Some(custom_name) = stack
                        .get_data_component::<pumpkin_data::data_component_impl::CustomNameImpl>()
                {
                    return Some(custom_name.name.clone());
                }
                }
                None
            });

        let entry = CombatEntry {
            damage_type,
            damage,
            fall_location: Some(fall_location),
            fall_distance,
            timestamp: current_tick,
            source_id: source.map(|e| e.get_entity().entity_id),
            attacker_id,
            attacker_name,
            attacker_item_name,
            attacker_is_living,
            attacker_is_player,
        };

        self.entries.push(entry);
        self.last_damage_time = current_tick;
        self.taking_damage = true;

        if !self.in_combat && is_alive && attacker_is_living {
            self.in_combat = true;
            self.combat_start_time = current_tick;
            self.combat_end_time = self.combat_start_time;
        }
    }

    pub fn recheck_status(&mut self, current_tick: i64, is_alive: bool) {
        let reset = if self.in_combat {
            RESET_COMBAT_STATUS_TIME
        } else {
            RESET_DAMAGE_STATUS_TIME
        };
        if self.taking_damage && (!is_alive || current_tick - self.last_damage_time > reset) {
            self.taking_damage = false;
            self.in_combat = false;
            self.combat_end_time = current_tick;
            self.entries.clear();
        }
    }

    #[must_use]
    pub const fn get_combat_duration(&self, current_tick: i64) -> i64 {
        if self.in_combat {
            current_tick - self.combat_start_time
        } else {
            self.combat_end_time - self.combat_start_time
        }
    }

    #[must_use]
    pub const fn is_in_combat(&self) -> bool {
        self.in_combat
    }

    #[must_use]
    pub fn has_player_attacker(&self) -> bool {
        self.entries.iter().any(|e| e.attacker_is_player)
    }

    #[must_use]
    pub fn get_killer_entry(&self) -> Option<&CombatEntry> {
        let mut best_living: Option<&CombatEntry> = None;
        let mut best_player: Option<&CombatEntry> = None;
        let mut max_living_dmg = 0.0f32;
        let mut max_player_dmg = 0.0f32;

        for entry in &self.entries {
            if entry.attacker_is_player && (best_player.is_none() || entry.damage > max_player_dmg)
            {
                max_player_dmg = entry.damage;
                best_player = Some(entry);
            }
            if entry.attacker_is_living && (best_living.is_none() || entry.damage > max_living_dmg)
            {
                max_living_dmg = entry.damage;
                best_living = Some(entry);
            }
        }

        if best_player.is_some() && max_player_dmg >= max_living_dmg / 3.0 {
            best_player
        } else {
            best_living
        }
    }

    #[must_use]
    pub fn get_death_message(
        &self,
        victim_name: pumpkin_util::text::TextComponent,
        kill_credit_name: Option<pumpkin_util::text::TextComponent>,
    ) -> pumpkin_util::text::TextComponent {
        if self.entries.is_empty() {
            return pumpkin_util::text::TextComponent::translate_cross(
                "death.attack.generic",
                "death.attack.generic",
                [victim_name],
            );
        }

        let killing_blow = &self.entries[self.entries.len() - 1];
        let killing_damage_type = killing_blow.damage_type;
        let knock_off_entry = self.get_most_significant_fall();

        match killing_damage_type.death_message_type {
            pumpkin_data::damage::DeathMessageType::FallVariants => {
                if let Some(knock_off) = knock_off_entry {
                    Self::get_fall_message(victim_name, knock_off, killing_blow)
                } else {
                    let loc = killing_blow.fall_location.unwrap_or(FallLocation::Generic);
                    pumpkin_util::text::TextComponent::translate_cross(
                        loc.language_key(),
                        loc.language_key(),
                        [victim_name],
                    )
                }
            }
            pumpkin_data::damage::DeathMessageType::IntentionalGameDesign => {
                let death_msg = format!("death.attack.{}", killing_damage_type.message_id);
                let link = pumpkin_util::text::TextComponent::text("[")
                    .add_child(pumpkin_util::text::TextComponent::translate_cross(
                        format!("{death_msg}.link"),
                        format!("{death_msg}.link"),
                        [],
                    ))
                    .add_child(pumpkin_util::text::TextComponent::text("]"))
                    .click_event(pumpkin_util::text::click::ClickEvent::OpenUrl {
                        url: "https://bugs.mojang.com/browse/MCPE-28723".into(),
                    })
                    .hover_event(pumpkin_util::text::hover::HoverEvent::show_text(
                        pumpkin_util::text::TextComponent::text("MCPE-28723"),
                    ));

                pumpkin_util::text::TextComponent::translate_cross(
                    format!("{death_msg}.message"),
                    format!("{death_msg}.message"),
                    [victim_name, link],
                )
            }
            pumpkin_data::damage::DeathMessageType::Default => {
                self.get_default_death_message(victim_name, killing_blow, kill_credit_name)
            }
        }
    }

    fn get_most_significant_fall(&self) -> Option<&CombatEntry> {
        let mut result = None;
        let mut alternative = None;
        let mut alt_damage = 0.0f32;
        let mut best_fall = 0.0f32;

        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            let previous = (i > 0).then(|| &self.entries[i - 1]);
            let damage_type = entry.damage_type;
            let is_fake_fall = damage_type
                .has_tag(&pumpkin_data::tag::DamageType::MINECRAFT_ALWAYS_MOST_SIGNIFICANT_FALL);
            let fall_distance = if is_fake_fall {
                f32::MAX
            } else {
                entry.fall_distance
            };

            if (damage_type.has_tag(&pumpkin_data::tag::DamageType::MINECRAFT_IS_FALL)
                || is_fake_fall)
                && fall_distance > 0.0
                && (result.is_none() || fall_distance > best_fall)
            {
                if i > 0 {
                    result = previous;
                } else {
                    result = Some(entry);
                }
                best_fall = fall_distance;
            }

            if entry.fall_location.is_some() && (alternative.is_none() || entry.damage > alt_damage)
            {
                alternative = Some(entry);
                alt_damage = entry.damage;
            }
        }

        if best_fall > 5.0 && result.is_some() {
            result
        } else if alt_damage > 5.0 && alternative.is_some() {
            alternative
        } else {
            None
        }
    }

    fn get_fall_message(
        victim_name: pumpkin_util::text::TextComponent,
        knock_off_entry: &CombatEntry,
        killing_blow: &CombatEntry,
    ) -> pumpkin_util::text::TextComponent {
        let knock_off_type = knock_off_entry.damage_type;
        if !knock_off_type.has_tag(&pumpkin_data::tag::DamageType::MINECRAFT_IS_FALL)
            && !knock_off_type
                .has_tag(&pumpkin_data::tag::DamageType::MINECRAFT_ALWAYS_MOST_SIGNIFICANT_FALL)
        {
            let killer_name = killing_blow.attacker_name.as_ref();
            let attacker_name = knock_off_entry.attacker_name.as_ref();
            let attacker_item = knock_off_entry.attacker_item_name.clone();

            if let Some(attacker_name) = attacker_name
                && (killer_name.is_none() || killer_name != Some(attacker_name))
            {
                Self::get_message_for_assisted_fall(
                    victim_name,
                    attacker_name.clone(),
                    attacker_item,
                    "death.fell.assist.item",
                    "death.fell.assist",
                )
            } else if let Some(killer_name) = killer_name {
                Self::get_message_for_assisted_fall(
                    victim_name,
                    killer_name.clone(),
                    killing_blow.attacker_item_name.clone(),
                    "death.fell.finish.item",
                    "death.fell.finish",
                )
            } else {
                pumpkin_util::text::TextComponent::translate_cross(
                    "death.fell.killer",
                    "death.fell.killer",
                    [victim_name],
                )
            }
        } else {
            let loc = knock_off_entry
                .fall_location
                .unwrap_or(FallLocation::Generic);
            pumpkin_util::text::TextComponent::translate_cross(
                loc.language_key(),
                loc.language_key(),
                [victim_name],
            )
        }
    }

    fn get_message_for_assisted_fall(
        victim_name: pumpkin_util::text::TextComponent,
        attacker_name: pumpkin_util::text::TextComponent,
        attacker_item: Option<pumpkin_util::text::TextComponent>,
        message_with_item: &'static str,
        message_without_item: &'static str,
    ) -> pumpkin_util::text::TextComponent {
        if let Some(item_name) = attacker_item {
            pumpkin_util::text::TextComponent::translate_cross(
                message_with_item,
                message_with_item,
                [victim_name, attacker_name, item_name],
            )
        } else {
            pumpkin_util::text::TextComponent::translate_cross(
                message_without_item,
                message_without_item,
                [victim_name, attacker_name],
            )
        }
    }

    fn get_default_death_message(
        &self,
        victim_name: pumpkin_util::text::TextComponent,
        killing_blow: &CombatEntry,
        kill_credit_name: Option<pumpkin_util::text::TextComponent>,
    ) -> pumpkin_util::text::TextComponent {
        let damage_type = killing_blow.damage_type;
        let msg_id = damage_type.message_id;

        if let Some(attacker_name) = &killing_blow.attacker_name {
            if let Some(item_name) = &killing_blow.attacker_item_name {
                pumpkin_util::text::TextComponent::translate_cross(
                    format!("death.attack.{msg_id}.item"),
                    format!("death.attack.{msg_id}.item"),
                    [victim_name, attacker_name.clone(), item_name.clone()],
                )
            } else {
                pumpkin_util::text::TextComponent::translate_cross(
                    format!("death.attack.{msg_id}"),
                    format!("death.attack.{msg_id}"),
                    [victim_name, attacker_name.clone()],
                )
            }
        } else if let Some(killer_name) = kill_credit_name.or_else(|| {
            self.get_killer_entry()
                .and_then(|k| k.attacker_name.clone())
        }) {
            pumpkin_util::text::TextComponent::translate_cross(
                format!("death.attack.{msg_id}.player"),
                format!("death.attack.{msg_id}.player"),
                [victim_name, killer_name],
            )
        } else {
            pumpkin_util::text::TextComponent::translate_cross(
                format!("death.attack.{msg_id}"),
                format!("death.attack.{msg_id}"),
                [victim_name],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_resistance_keeps_full_strength() {
        assert_eq!(knockback_after_resistance(0.4, 0.0), 0.4);
    }

    #[test]
    fn full_resistance_cancels_knockback() {
        // Iron golem / warden have KNOCKBACK_RESISTANCE == 1.0.
        assert_eq!(knockback_after_resistance(0.4, 1.0), 0.0);
    }

    #[test]
    fn partial_resistance_scales_strength() {
        // Ravager has KNOCKBACK_RESISTANCE == 0.75.
        assert!((knockback_after_resistance(0.4, 0.75) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn over_full_resistance_is_negative_so_callers_skip_it() {
        // Stacked armour modifiers can push resistance above 1.0; the result is
        // negative and callers guard on `strength > 0.0`.
        assert!(knockback_after_resistance(0.4, 1.2) < 0.0);
    }

    /// Player base attack speed 4 plus the wooden sword's `base_attack_speed` modifier.
    fn wooden_sword_attack_speed() -> f64 {
        4.0 + -2.400_000_095_367_431_6
    }

    /// Player base attack damage 1 plus the wooden sword's `base_attack_damage` of 3.
    const WOODEN_SWORD_DAMAGE: f64 = 4.0;

    fn wooden_hit(
        ticks: u32,
        enchantment_bonus: f32,
        critical_allowed: bool,
        sprinting: bool,
    ) -> MeleeHit {
        let scale = attack_cooldown_scale(
            ticks,
            ATTACK_STRENGTH_ADJUST_TICKS,
            wooden_sword_attack_speed(),
        );
        resolve_melee_hit(&MeleeHitInput {
            attack_damage: WOODEN_SWORD_DAMAGE,
            attack_strength_scale: scale,
            enchantment_bonus,
            bonus_before_crit: 0.0,
            critical_allowed,
            sprinting,
        })
    }

    #[test]
    fn wooden_sword_full_charge_deals_attack_damage() {
        // 13 ticks is the first count where `(ticks + 0.5) / delay` reaches 1.
        // The delay is the float cast of `20 / 1.6`, which is slightly over 12.5.
        let hit = wooden_hit(13, 0.0, false, false);
        assert_eq!(hit.attack_strength_scale, 1.0);
        assert_eq!(hit.total_damage, 4.0);
        let health = 20.0 - hit.total_damage;
        assert_eq!(health, 16.0);
    }

    #[test]
    fn wooden_sword_twelve_ticks_is_not_quite_full() {
        let hit = wooden_hit(12, 0.0, false, false);
        assert!(hit.attack_strength_scale < 1.0);
        assert!(hit.total_damage < 4.0);
        assert!(hit.full_strength);
    }

    #[test]
    fn immediate_second_hit_deals_the_reduced_fraction() {
        let hit = wooden_hit(0, 0.0, false, false);
        // (0 + 0.5) / delay ≈ 0.04, then `0.2 + scale² * 0.8` ≈ 0.20128, times 4.
        assert!((hit.total_damage - 0.805_12).abs() < 1e-4);
        assert!(hit.total_damage < WOODEN_SWORD_DAMAGE as f32);
        assert!(!hit.full_strength);
    }

    #[test]
    fn leather_chest_reduces_a_full_wooden_hit() {
        // Leather chestplate is 3 armor and 0 toughness.
        let dealt = CombatRules::get_damage_after_absorb(4.0, 3.0, 0.0, 0);
        assert!((dealt - 3.84).abs() < 1e-5);
        let health = 20.0 - dealt;
        assert!((health - 16.16).abs() < 1e-4);
    }

    #[test]
    fn immediate_second_hit_is_rejected_during_invulnerability() {
        let second = wooden_hit(0, 0.0, false, false);
        let rejected = resolve_hurt_cooldown(second.total_damage, 19, 4.0, false);
        assert!(rejected.is_none());
    }

    #[test]
    fn stronger_hit_during_invulnerability_applies_only_the_excess() {
        let partial = resolve_hurt_cooldown(6.0, 15, 4.0, false).expect("larger hit connects");
        assert!(!partial.full_hit);
        assert_eq!(partial.damage_to_apply, 2.0);
        assert_eq!(partial.last_hurt, 6.0);
        assert_eq!(partial.cooldown_ticks, 15);
        let dealt = CombatRules::get_damage_after_absorb(partial.damage_to_apply, 3.0, 0.0, 0);
        assert!((dealt - 1.84).abs() < 1e-4);
    }

    #[test]
    fn hurt_cooldown_at_ten_ticks_accepts_a_full_hit() {
        let again = resolve_hurt_cooldown(4.0, 10, 4.0, false).expect("cooldown elapsed");
        assert!(again.full_hit);
        assert_eq!(again.damage_to_apply, 4.0);
        assert_eq!(again.cooldown_ticks, HURT_COOLDOWN_TICKS);
    }

    #[test]
    fn kill_damage_bypasses_the_hurt_cooldown() {
        let kill = resolve_hurt_cooldown(1.0, 19, 4.0, true).expect("bypass");
        assert!(kill.full_hit);
        assert_eq!(kill.damage_to_apply, 1.0);
    }

    #[test]
    fn critical_hit_does_not_multiply_sharpness() {
        // Full charge, sharpness I (+1), falling crit. Base 4 * 1.5, then +1.
        let hit = wooden_hit(13, 1.0, true, false);
        assert!(hit.critical);
        assert_eq!(hit.base_damage, 6.0);
        assert_eq!(hit.magic_boost, 1.0);
        assert_eq!(hit.total_damage, 7.0);
    }

    #[test]
    fn sharpness_on_a_half_charge_scales_linearly() {
        let scale = 0.5;
        let hit = resolve_melee_hit(&MeleeHitInput {
            attack_damage: WOODEN_SWORD_DAMAGE,
            attack_strength_scale: scale,
            enchantment_bonus: 1.0,
            bonus_before_crit: 0.0,
            critical_allowed: false,
            sprinting: false,
        });
        assert!((hit.base_damage - 1.6).abs() < 1e-5);
        assert_eq!(hit.magic_boost, 0.5);
        assert!((hit.total_damage - 2.1).abs() < 1e-5);
    }

    #[test]
    fn strength_is_already_inside_attack_damage() {
        // Wooden sword 4 plus strength I (+3) is 7, not 7 plus another 3.
        let scale = attack_cooldown_scale(
            13,
            ATTACK_STRENGTH_ADJUST_TICKS,
            wooden_sword_attack_speed(),
        );
        let hit = resolve_melee_hit(&MeleeHitInput {
            attack_damage: 7.0,
            attack_strength_scale: scale,
            enchantment_bonus: 0.0,
            bonus_before_crit: 0.0,
            critical_allowed: false,
            sprinting: false,
        });
        assert_eq!(hit.total_damage, 7.0);
    }

    fn sweeping_edge_ratio(level: i32) -> f64 {
        let effect = pumpkin_data::Enchantment::SWEEPING_EDGE
            .effects
            .attributes
            .iter()
            .find(|effect| {
                effect.attribute == &pumpkin_data::attributes::Attributes::SWEEPING_DAMAGE_RATIO
            })
            .expect("sweeping edge writes sweeping_damage_ratio");
        f64::from(effect.amount.calculate(level))
    }

    #[test]
    fn sweeping_edge_second_target_damage_follows_the_attribute() {
        // Full-charge wooden sword base is 4. The ratio is the enchantment's
        // attribute amount, level / (level + 1). Second-target damage, before
        // the victim's armor, is 1 + ratio * 4.
        let first = sweeping_edge_ratio(1);
        let second = sweeping_edge_ratio(2);
        let third = sweeping_edge_ratio(3);
        assert_eq!(first as f32, 0.5);
        assert_eq!(second as f32, 2.0f32 / 3.0);
        assert_eq!(third as f32, 0.75);
        assert_eq!(sweep_damage(4.0, first), 3.0);
        assert_eq!(sweep_damage(4.0, second), 1.0 + (2.0f32 / 3.0) * 4.0);
        assert_eq!(sweep_damage(4.0, third), 4.0);
    }

    #[test]
    fn sweep_damage_uses_only_the_sweeping_damage_ratio_attribute() {
        // The hit reads `sweeping_damage_ratio`. It does not add a level formula
        // on top. 0 is an unenchanted sword. 0.5 and 0.75 are the attribute
        // amounts Sweeping Edge writes at levels 1 and 3.
        assert_eq!(sweep_damage(4.0, 0.0), 1.0);
        assert_eq!(sweep_damage(4.0, 0.5), 3.0);
        assert_eq!(sweep_damage(4.0, 0.75), 4.0);
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "each flag is one sweep condition"
    )]
    fn sweep(
        full_strength: bool,
        critical: bool,
        sprint_knockback: bool,
        on_ground: bool,
        horizontal_speed_sq: f64,
        holding_sword: bool,
    ) -> bool {
        is_sweep_attack(&SweepAttack {
            full_strength,
            critical,
            sprint_knockback,
            on_ground,
            horizontal_speed_sq,
            movement_speed: 0.1,
            holding_sword,
        })
    }

    #[test]
    fn sweep_requires_a_grounded_full_sword_hit() {
        assert!(sweep(true, false, false, true, 0.0, true));
        assert!(!sweep(true, true, false, true, 0.0, true));
        assert!(!sweep(true, false, true, true, 0.0, true));
        assert!(!sweep(true, false, false, false, 0.0, true));
        assert!(!sweep(false, false, false, true, 0.0, true));
        assert!(!sweep(true, false, false, true, 0.0, false));
        // movement speed 0.1 * 2.5 = 0.25, squared 0.0625.
        assert!(!sweep(true, false, false, true, 0.07, true));
    }

    #[test]
    fn sweep_does_not_hit_the_attacker() {
        assert!(!sweep_hits_entity(1, 2, 1, false, false, 1.0));
        assert!(!sweep_hits_entity(1, 2, 2, false, false, 1.0));
        assert!(!sweep_hits_entity(1, 2, 3, true, false, 1.0));
        assert!(!sweep_hits_entity(1, 2, 3, false, false, 9.0));
        assert!(sweep_hits_entity(1, 2, 3, false, false, 8.9));
    }

    #[test]
    fn sweep_skips_marker_armor_stands_and_hits_other_armor_stands() {
        assert!(!sweep_hits_entity(1, 2, 3, false, true, 1.0));
        assert!(sweep_hits_entity(1, 2, 3, false, false, 1.0));
    }

    #[test]
    fn sprint_and_knockback_enchant_share_one_power() {
        assert_eq!(extra_knockback_power(0.0, 0.0, false), 0.0);
        assert_eq!(extra_knockback_power(0.0, 1.0, false), 0.5);
        assert_eq!(extra_knockback_power(0.0, 2.0, false), 1.0);
        assert_eq!(extra_knockback_power(0.0, 0.0, true), 0.5);
        assert_eq!(extra_knockback_power(0.0, 1.0, true), 1.0);
        assert_eq!(extra_knockback_power(0.0, 2.0, true), 1.5);
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "each flag is one critical-hit condition"
    )]
    fn crit(
        fall_distance: f32,
        on_ground: bool,
        in_water: bool,
        blindness: bool,
        target_is_living: bool,
        sprinting: bool,
    ) -> bool {
        can_critical_attack(&CriticalAttack {
            fall_distance,
            on_ground,
            on_climbable: false,
            in_water,
            blindness,
            passenger: false,
            target_is_living,
            sprinting,
        })
    }

    #[test]
    fn critical_attack_is_blocked_by_sprint_water_and_blindness() {
        assert!(crit(0.1, false, false, false, true, false));
        assert!(!crit(0.1, false, false, false, true, true));
        assert!(!crit(0.1, true, false, false, true, false));
        assert!(!crit(0.1, false, true, false, true, false));
        assert!(!crit(0.1, false, false, true, true, false));
        assert!(!crit(0.0, false, false, false, true, false));
        assert!(!crit(0.1, false, false, false, false, false));
    }

    #[test]
    fn creative_invulnerability_blocks_ordinary_hits() {
        assert!(invulnerable_ability_blocks(true, false, false));
        assert!(!invulnerable_ability_blocks(true, true, false));
        assert!(!invulnerable_ability_blocks(true, false, true));
        assert!(!invulnerable_ability_blocks(false, false, false));
    }

    #[test]
    fn fist_full_charge_is_one_damage_after_five_ticks() {
        let scale = attack_cooldown_scale(5, ATTACK_STRENGTH_ADJUST_TICKS, 4.0);
        assert_eq!(scale, 1.0);
        let hit = resolve_melee_hit(&MeleeHitInput {
            attack_damage: 1.0,
            attack_strength_scale: scale,
            enchantment_bonus: 0.0,
            bonus_before_crit: 0.0,
            critical_allowed: false,
            sprinting: false,
        });
        assert_eq!(hit.total_damage, 1.0);
    }

    #[test]
    fn attack_cooldown_stays_on_twenty_ticks_per_second() {
        let speed = wooden_sword_attack_speed();
        let vanilla = attack_cooldown_scale(13, ATTACK_STRENGTH_ADJUST_TICKS, speed);
        assert_eq!(vanilla, 1.0);
        // A server running at 40 ticks per second would stretch the delay if the
        // cooldown used that rate. Vanilla keeps the literal 20.
        let at_forty = attack_strength_scale(13, ATTACK_STRENGTH_ADJUST_TICKS, speed, 40.0);
        assert!(at_forty < 1.0);
        assert!(at_forty < vanilla);
        let second = attack_cooldown_scale(0, ATTACK_STRENGTH_ADJUST_TICKS, speed);
        let hit = resolve_melee_hit(&MeleeHitInput {
            attack_damage: WOODEN_SWORD_DAMAGE,
            attack_strength_scale: second,
            enchantment_bonus: 0.0,
            bonus_before_crit: 0.0,
            critical_allowed: false,
            sprinting: false,
        });
        assert!((hit.total_damage - 0.805_12).abs() < 1e-4);
    }

    #[test]
    fn mace_smash_bonus_follows_the_fall_bands() {
        assert_eq!(mace_smash_bonus(2.0, 0.0), 8.0);
        assert_eq!(mace_smash_bonus(3.0, 0.0), 12.0);
        assert_eq!(mace_smash_bonus(4.0, 0.0), 14.0);
        assert_eq!(mace_smash_bonus(8.0, 0.0), 22.0);
        assert_eq!(mace_smash_bonus(9.0, 0.0), 23.0);
        assert_eq!(mace_smash_bonus(2.0, 1.0), 10.0);
        assert!(!can_mace_smash(MACE_SMASH_FALL_THRESHOLD, false));
        assert!(can_mace_smash(1.6, false));
        assert!(!can_mace_smash(4.0, true));
    }

    #[test]
    fn critical_hit_multiplies_the_smash_bonus_and_not_sharpness() {
        let hit = resolve_melee_hit(&MeleeHitInput {
            attack_damage: WOODEN_SWORD_DAMAGE,
            attack_strength_scale: 1.0,
            enchantment_bonus: 1.0,
            bonus_before_crit: 8.0,
            critical_allowed: true,
            sprinting: false,
        });
        assert_eq!(hit.base_damage, 18.0);
        assert_eq!(hit.magic_boost, 1.0);
        assert_eq!(hit.total_damage, 19.0);
    }
}
