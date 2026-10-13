//! Whether a bed click starts sleep, and whether it records a respawn point.
//!
//! The order follows the dedicated server: range, then the space above the bed,
//! then the respawn write when the dimension allows it, then the night check,
//! then monsters. A daytime click and a click with a monster nearby still store
//! the bed when the bed rule can set a spawn.

use pumpkin_data::entity::EntityType;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

/// Horizontal reach from a bed half's bottom centre.
pub const BED_REACH_HORIZONTAL: f64 = 3.0;
/// Vertical reach from a bed half's bottom centre.
pub const BED_REACH_VERTICAL: f64 = 2.0;
/// Horizontal half-extent of the monster rest box around the bed head.
pub const MONSTER_REST_HORIZONTAL: f64 = 8.0;
/// Vertical half-extent of the monster rest box around the bed head.
pub const MONSTER_REST_VERTICAL: f64 = 5.0;

/// Why a bed click did not start sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedSleepRefusal {
    /// The space above the bed is blocked.
    Obstructed,
    /// Someone is already using the bed.
    Occupied,
    /// The player is too far from both halves.
    TooFar,
    /// Sleep is not allowed right now, such as daytime in the overworld.
    NotPossibleNow,
    /// A monster that prevents rest overlaps the box around the bed head.
    NotSafe,
}

/// Result of the sleep checks that run after the occupied-bed check.
///
/// `set_spawn` is independent of `refusal`. Range and obstruction refuse the
/// click before any spawn write. Daytime and a nearby monster refuse sleep
/// after the write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BedSleepDecision {
    /// Store this bed as the respawn point.
    pub set_spawn: bool,
    /// `None` means the player enters the bed.
    pub refusal: Option<BedSleepRefusal>,
}

/// Facts already measured for this click, after the occupied check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BedSleepQuery {
    /// `BedRule::can_sleep` for the current light level.
    pub can_sleep: bool,
    /// `BedRule::can_set_spawn` for the current light level.
    pub can_set_spawn: bool,
    /// A solid block sits above the head or the foot.
    pub obstructed: bool,
    /// The player is within reach of the head or the foot.
    pub in_reach: bool,
    /// A monster that prevents rest overlaps the rest box. Creative players pass `false`.
    pub monster_nearby: bool,
}

/// Decides spawn and sleep in the dedicated server's order.
///
/// Range, then obstruction, then the spawn write, then the night check, then
/// monsters. The spawn write happens when `can_set_spawn` is true even if the
/// night or monster check then refuses sleep.
#[must_use]
pub const fn decide_bed_sleep(query: BedSleepQuery) -> BedSleepDecision {
    if !query.in_reach {
        return BedSleepDecision {
            set_spawn: false,
            refusal: Some(BedSleepRefusal::TooFar),
        };
    }
    if query.obstructed {
        return BedSleepDecision {
            set_spawn: false,
            refusal: Some(BedSleepRefusal::Obstructed),
        };
    }
    if !query.can_sleep {
        return BedSleepDecision {
            set_spawn: query.can_set_spawn,
            refusal: Some(BedSleepRefusal::NotPossibleNow),
        };
    }
    if query.monster_nearby {
        return BedSleepDecision {
            set_spawn: query.can_set_spawn,
            refusal: Some(BedSleepRefusal::NotSafe),
        };
    }
    BedSleepDecision {
        set_spawn: query.can_set_spawn,
        refusal: None,
    }
}

/// Whether the player is close enough to click this bed half.
///
/// The anchor is the bottom centre of the half. Horizontal limit is 3 blocks
/// and vertical limit is 2, matching the dedicated server's bed reach check.
#[must_use]
pub fn bed_half_in_reach(player_pos: Vector3<f64>, bed_half: BlockPos) -> bool {
    let center = bed_half.to_f64();
    (player_pos.x - center.x).abs() <= BED_REACH_HORIZONTAL
        && (player_pos.y - center.y).abs() <= BED_REACH_VERTICAL
        && (player_pos.z - center.z).abs() <= BED_REACH_HORIZONTAL
}

/// Axis-aligned box around the bed head used to find monsters that block sleep.
///
/// The box is centred on the head's bottom centre and extends 8 blocks
/// horizontally and 5 blocks vertically.
#[must_use]
pub fn monster_rest_box(bed_head: BlockPos) -> BoundingBox {
    let center = bed_head.to_f64();
    BoundingBox::new(
        Vector3::new(
            center.x - MONSTER_REST_HORIZONTAL,
            center.y - MONSTER_REST_VERTICAL,
            center.z - MONSTER_REST_HORIZONTAL,
        ),
        Vector3::new(
            center.x + MONSTER_REST_HORIZONTAL,
            center.y + MONSTER_REST_VERTICAL,
            center.z + MONSTER_REST_HORIZONTAL,
        ),
    )
}

/// Whether this entity's box overlaps the monster rest box of `bed_head`.
#[must_use]
pub fn entity_box_blocks_bed_rest(bed_head: BlockPos, entity_box: &BoundingBox) -> bool {
    entity_box.intersects(&monster_rest_box(bed_head))
}

/// How this entity type answers the rest check.
///
/// The dedicated server asks `Monster` subclasses, then `isPreventingPlayerRest`.
/// That method is true for every subclass except the zombified piglin, which
/// answers with whether it is angry at this player. Slimes, magma cubes, ghasts,
/// phantoms, and shulkers are hostile by spawn category and are not `Monster`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedRestRule {
    /// Not a `Monster` subclass.
    AllowsRest,
    /// `isPreventingPlayerRest` is true.
    PreventsRest,
    /// Blocks only while this zombified piglin is angry at the clicking player.
    PreventsRestWhileAngry,
}

/// Class-hierarchy rest rule for `entity_type`.
#[must_use]
pub const fn bed_rest_rule(entity_type: &EntityType) -> BedRestRule {
    if entity_type.id == EntityType::ZOMBIFIED_PIGLIN.id {
        return BedRestRule::PreventsRestWhileAngry;
    }
    if is_monster_subclass(entity_type.id) {
        BedRestRule::PreventsRest
    } else {
        BedRestRule::AllowsRest
    }
}

/// Applies a [`BedRestRule`]. `angry_at_player` is read only for zombified piglins.
#[must_use]
pub const fn prevents_bed_rest(rule: BedRestRule, angry_at_player: bool) -> bool {
    match rule {
        BedRestRule::AllowsRest => false,
        BedRestRule::PreventsRest => true,
        BedRestRule::PreventsRestWhileAngry => angry_at_player,
    }
}

/// Concrete entity types whose class extends `Monster` in 26.3.
///
/// Abstract parents such as `Raider` and `AbstractSkeleton` have no entity type.
/// The zombified piglin extends `Zombie` and is listed here; [`bed_rest_rule`]
/// applies its anger override.
const fn is_monster_subclass(id: u16) -> bool {
    id == EntityType::BLAZE.id
        || id == EntityType::BOGGED.id
        || id == EntityType::BREEZE.id
        || id == EntityType::CAVE_SPIDER.id
        || id == EntityType::CREAKING.id
        || id == EntityType::CREEPER.id
        || id == EntityType::DROWNED.id
        || id == EntityType::ELDER_GUARDIAN.id
        || id == EntityType::ENDERMAN.id
        || id == EntityType::ENDERMITE.id
        || id == EntityType::EVOKER.id
        || id == EntityType::GIANT.id
        || id == EntityType::GUARDIAN.id
        || id == EntityType::HUSK.id
        || id == EntityType::ILLUSIONER.id
        || id == EntityType::PARCHED.id
        || id == EntityType::PIGLIN.id
        || id == EntityType::PIGLIN_BRUTE.id
        || id == EntityType::PILLAGER.id
        || id == EntityType::RAVAGER.id
        || id == EntityType::SILVERFISH.id
        || id == EntityType::SKELETON.id
        || id == EntityType::SPIDER.id
        || id == EntityType::STRAY.id
        || id == EntityType::VEX.id
        || id == EntityType::VINDICATOR.id
        || id == EntityType::WARDEN.id
        || id == EntityType::WITCH.id
        || id == EntityType::WITHER.id
        || id == EntityType::WITHER_SKELETON.id
        || id == EntityType::ZOGLIN.id
        || id == EntityType::ZOMBIE.id
        || id == EntityType::ZOMBIE_VILLAGER.id
        || id == EntityType::ZOMBIFIED_PIGLIN.id
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::boundingbox::BoundingBox;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector3::Vector3;

    use super::{
        BedRestRule, BedSleepDecision, BedSleepQuery, BedSleepRefusal, bed_half_in_reach,
        bed_rest_rule, decide_bed_sleep, entity_box_blocks_bed_rest, monster_rest_box,
        prevents_bed_rest,
    };

    fn night_query() -> BedSleepQuery {
        BedSleepQuery {
            can_sleep: true,
            can_set_spawn: true,
            obstructed: false,
            in_reach: true,
            monster_nearby: false,
        }
    }

    fn decision(set_spawn: bool, refusal: Option<BedSleepRefusal>) -> BedSleepDecision {
        BedSleepDecision { set_spawn, refusal }
    }

    #[test]
    fn valid_night_sleep_sets_the_spawn() {
        assert_eq!(decide_bed_sleep(night_query()), decision(true, None));
    }

    #[test]
    fn daytime_click_sets_the_spawn_and_refuses_sleep() {
        let mut query = night_query();
        query.can_sleep = false;
        assert_eq!(
            decide_bed_sleep(query),
            decision(true, Some(BedSleepRefusal::NotPossibleNow))
        );
    }

    #[test]
    fn daytime_is_checked_before_a_nearby_monster() {
        let mut query = night_query();
        query.can_sleep = false;
        query.monster_nearby = true;
        assert_eq!(
            decide_bed_sleep(query),
            decision(true, Some(BedSleepRefusal::NotPossibleNow))
        );
    }

    #[test]
    fn monster_nearby_sets_the_spawn_and_refuses_sleep() {
        let mut query = night_query();
        query.monster_nearby = true;
        assert_eq!(
            decide_bed_sleep(query),
            decision(true, Some(BedSleepRefusal::NotSafe))
        );
    }

    #[test]
    fn out_of_range_wins_over_obstruction_and_does_not_set_spawn() {
        let mut query = night_query();
        query.in_reach = false;
        query.obstructed = true;
        query.can_sleep = false;
        assert_eq!(
            decide_bed_sleep(query),
            decision(false, Some(BedSleepRefusal::TooFar))
        );
    }

    #[test]
    fn obstruction_refuses_before_the_spawn_write() {
        let mut query = night_query();
        query.obstructed = true;
        assert_eq!(
            decide_bed_sleep(query),
            decision(false, Some(BedSleepRefusal::Obstructed))
        );
    }

    #[test]
    fn sleep_without_spawn_permission_still_enters_the_bed() {
        let mut query = night_query();
        query.can_set_spawn = false;
        assert_eq!(decide_bed_sleep(query), decision(false, None));
    }

    #[test]
    fn daytime_without_spawn_permission_does_not_write_a_spawn() {
        let mut query = night_query();
        query.can_sleep = false;
        query.can_set_spawn = false;
        assert_eq!(
            decide_bed_sleep(query),
            decision(false, Some(BedSleepRefusal::NotPossibleNow))
        );
    }

    #[test]
    fn reach_uses_bottom_centre_and_a_shorter_vertical_limit() {
        let bed = BlockPos::new(10, 64, -4);
        let center = bed.to_f64();
        assert!(bed_half_in_reach(center, bed));
        assert!(bed_half_in_reach(
            Vector3::new(center.x + 3.0, center.y + 2.0, center.z),
            bed
        ));
        assert!(!bed_half_in_reach(
            Vector3::new(center.x + 3.01, center.y, center.z),
            bed
        ));
        assert!(!bed_half_in_reach(
            Vector3::new(center.x, center.y + 2.01, center.z),
            bed
        ));
    }

    #[test]
    fn monster_rest_box_is_around_the_head_only() {
        let head = BlockPos::new(0, 64, 0);
        let rest = monster_rest_box(head);
        let center = head.to_f64();
        assert_eq!(rest.min.x, center.x - 8.0);
        assert_eq!(rest.min.y, center.y - 5.0);
        assert_eq!(rest.max.x, center.x + 8.0);
        assert_eq!(rest.max.y, center.y + 5.0);

        let inside = BoundingBox::new(
            Vector3::new(center.x + 7.0, center.y, center.z),
            Vector3::new(center.x + 7.6, center.y + 1.8, center.z + 0.6),
        );
        let outside = BoundingBox::new(
            Vector3::new(center.x + 8.5, center.y, center.z),
            Vector3::new(center.x + 9.1, center.y + 1.8, center.z + 0.6),
        );
        let foot_only = BoundingBox::new(
            Vector3::new(center.x, center.y, center.z + 9.0),
            Vector3::new(center.x + 0.6, center.y + 1.8, center.z + 9.6),
        );
        assert!(entity_box_blocks_bed_rest(head, &inside));
        assert!(!entity_box_blocks_bed_rest(head, &outside));
        assert!(!entity_box_blocks_bed_rest(head, &foot_only));
    }

    #[test]
    fn monster_subclasses_block_sleep_and_the_named_exceptions_do_not() {
        assert_eq!(
            bed_rest_rule(&EntityType::ZOMBIE),
            BedRestRule::PreventsRest
        );
        assert_eq!(
            bed_rest_rule(&EntityType::SPIDER),
            BedRestRule::PreventsRest
        );
        assert_eq!(
            bed_rest_rule(&EntityType::ENDERMAN),
            BedRestRule::PreventsRest
        );
        assert_eq!(
            bed_rest_rule(&EntityType::PIGLIN),
            BedRestRule::PreventsRest
        );
        assert_eq!(
            bed_rest_rule(&EntityType::WITHER),
            BedRestRule::PreventsRest
        );

        for entity_type in [
            &EntityType::SLIME,
            &EntityType::MAGMA_CUBE,
            &EntityType::SULFUR_CUBE,
            &EntityType::GHAST,
            &EntityType::PHANTOM,
            &EntityType::SHULKER,
            &EntityType::HOGLIN,
            &EntityType::OCELOT,
            &EntityType::COW,
        ] {
            let name = entity_type.resource_name;
            assert_eq!(
                bed_rest_rule(entity_type),
                BedRestRule::AllowsRest,
                "{name}"
            );
        }

        assert_eq!(
            bed_rest_rule(&EntityType::ZOMBIFIED_PIGLIN),
            BedRestRule::PreventsRestWhileAngry
        );
        assert!(!prevents_bed_rest(
            BedRestRule::PreventsRestWhileAngry,
            false
        ));
        assert!(prevents_bed_rest(BedRestRule::PreventsRestWhileAngry, true));
        assert!(prevents_bed_rest(BedRestRule::PreventsRest, false));
        assert!(!prevents_bed_rest(BedRestRule::AllowsRest, true));
    }
}
