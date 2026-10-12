//! Whether a bed click starts sleep, and whether that sleep records a respawn point.
//!
//! The stored spawn changes only when the player actually enters the bed. A refused
//! click (daytime, a monster in the rest box, range, or a blocked bed) leaves it alone.

use pumpkin_data::entity::MobCategory;
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
    /// This dimension allows neither sleep nor a new spawn.
    NotPossibleHere,
    /// The space above the bed is blocked.
    Obstructed,
    /// Someone is already using the bed.
    Occupied,
    /// The player is too far from both halves.
    TooFar,
    /// Sleep is not allowed right now, such as daytime in the overworld.
    NotPossibleNow,
    /// A monster overlaps the rest box around the bed head.
    NotSafe,
}

/// Result of a bed click after the dimension's explode rule has been handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedSleepOutcome {
    /// The player enters the bed. `set_spawn` is set when the dimension allows it.
    Sleep {
        set_spawn: bool,
    },
    Refuse(BedSleepRefusal),
}

/// Facts the bed block has already measured for this click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BedSleepQuery {
    /// `BedRule::can_sleep` for the current light level.
    pub can_sleep: bool,
    /// `BedRule::can_set_spawn` for the current light level.
    pub can_set_spawn: bool,
    /// A solid block sits above the head or the foot.
    pub obstructed: bool,
    /// The bed's occupied property is already set.
    pub occupied: bool,
    /// The player is within reach of the head or the foot.
    pub in_reach: bool,
    /// A monster's box overlaps the rest box. Creative players pass `false`.
    pub monster_nearby: bool,
}

/// Decides whether this click starts sleep and records the bed as the spawn.
///
/// Spawn is recorded only on [`BedSleepOutcome::Sleep`] with `set_spawn`. Daytime
/// and a nearby monster both refuse the click, so they do not move the spawn.
#[must_use]
pub const fn decide_bed_sleep(query: BedSleepQuery) -> BedSleepOutcome {
    if !query.can_sleep && !query.can_set_spawn {
        return BedSleepOutcome::Refuse(BedSleepRefusal::NotPossibleHere);
    }
    if query.obstructed {
        return BedSleepOutcome::Refuse(BedSleepRefusal::Obstructed);
    }
    if query.occupied {
        return BedSleepOutcome::Refuse(BedSleepRefusal::Occupied);
    }
    if !query.in_reach {
        return BedSleepOutcome::Refuse(BedSleepRefusal::TooFar);
    }
    if !query.can_sleep {
        return BedSleepOutcome::Refuse(BedSleepRefusal::NotPossibleNow);
    }
    if query.monster_nearby {
        return BedSleepOutcome::Refuse(BedSleepRefusal::NotSafe);
    }
    BedSleepOutcome::Sleep {
        set_spawn: query.can_set_spawn,
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

/// Monster-category mobs block sleep. Animals and other categories do not.
#[must_use]
pub const fn entity_category_prevents_bed_rest(category: &MobCategory) -> bool {
    category.id == MobCategory::MONSTER.id
}

#[cfg(test)]
mod tests {
    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::boundingbox::BoundingBox;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector3::Vector3;

    use super::{
        BedSleepOutcome, BedSleepQuery, BedSleepRefusal, bed_half_in_reach, decide_bed_sleep,
        entity_box_blocks_bed_rest, entity_category_prevents_bed_rest, monster_rest_box,
    };

    fn night_query() -> BedSleepQuery {
        BedSleepQuery {
            can_sleep: true,
            can_set_spawn: true,
            obstructed: false,
            occupied: false,
            in_reach: true,
            monster_nearby: false,
        }
    }

    #[test]
    fn valid_night_sleep_sets_the_spawn() {
        assert_eq!(
            decide_bed_sleep(night_query()),
            BedSleepOutcome::Sleep { set_spawn: true }
        );
    }

    #[test]
    fn daytime_click_does_not_set_the_spawn() {
        let mut query = night_query();
        query.can_sleep = false;
        assert_eq!(
            decide_bed_sleep(query),
            BedSleepOutcome::Refuse(BedSleepRefusal::NotPossibleNow)
        );
    }

    #[test]
    fn monster_nearby_does_not_start_sleep_or_move_spawn() {
        let mut query = night_query();
        query.monster_nearby = true;
        assert_eq!(
            decide_bed_sleep(query),
            BedSleepOutcome::Refuse(BedSleepRefusal::NotSafe)
        );
    }

    #[test]
    fn sleep_without_spawn_permission_still_enters_the_bed() {
        let mut query = night_query();
        query.can_set_spawn = false;
        assert_eq!(
            decide_bed_sleep(query),
            BedSleepOutcome::Sleep { set_spawn: false }
        );
    }

    #[test]
    fn neither_rule_refuses_before_the_other_checks() {
        let query = BedSleepQuery {
            can_sleep: false,
            can_set_spawn: false,
            obstructed: true,
            occupied: true,
            in_reach: false,
            monster_nearby: true,
        };
        assert_eq!(
            decide_bed_sleep(query),
            BedSleepOutcome::Refuse(BedSleepRefusal::NotPossibleHere)
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
        // A mob by the foot, outside the head box, does not count.
        let foot_only = BoundingBox::new(
            Vector3::new(center.x, center.y, center.z + 9.0),
            Vector3::new(center.x + 0.6, center.y + 1.8, center.z + 9.6),
        );
        assert!(entity_box_blocks_bed_rest(head, &inside));
        assert!(!entity_box_blocks_bed_rest(head, &outside));
        assert!(!entity_box_blocks_bed_rest(head, &foot_only));
    }

    #[test]
    fn only_the_monster_category_blocks_sleep() {
        assert!(entity_category_prevents_bed_rest(
            EntityType::ZOMBIE.category
        ));
        assert!(entity_category_prevents_bed_rest(
            EntityType::GHAST.category
        ));
        assert!(entity_category_prevents_bed_rest(
            EntityType::SLIME.category
        ));
        assert!(!entity_category_prevents_bed_rest(
            EntityType::OCELOT.category
        ));
        assert!(!entity_category_prevents_bed_rest(EntityType::COW.category));
    }
}
