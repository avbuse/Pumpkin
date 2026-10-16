pub mod armor_stand;
pub mod arrow;
pub mod axe;
pub mod boat;
pub mod bone_meal;
pub mod bow;
pub mod brush;
pub mod bucket;
pub mod bundle;
pub mod clock;
pub mod compass;
pub mod crossbow;
pub mod cushion;
pub mod debug_stick;
pub mod dye;
pub mod egg;
pub mod end_crystal;
pub mod ender_eye;
pub mod ender_pearl;
pub mod experience_bottle;
pub mod firework_rocket;
pub mod fishing_rod;
pub mod glass_bottle;
pub mod glowing_ink_sac;
pub mod goat_horn;
pub mod hanging_entity;
pub mod hoe;
pub mod honeycomb;
pub mod ignite;
pub mod ink_sac;
pub mod knowledge_book;
pub mod lead;
pub mod mace;
pub mod map;
pub mod minecart;
pub mod name_tag;
pub mod on_a_stick;
pub mod place_on_water;
pub mod potions;
pub mod projectile_weapon;
pub mod saddle;
pub mod shears;
pub mod shield;
pub mod shovel;
pub mod snowball;
pub mod spawn_egg;
pub mod spear;
pub mod spyglass;
pub mod swords;
pub mod trident;
pub mod wind_charge;
pub mod writable_book;

use crate::item::items::armor_stand::ArmorStandItem;
use crate::item::items::boat::BoatItem;
use crate::item::items::bone_meal::BoneMealItem;
use crate::item::items::brush::BrushItem;
use crate::item::items::bundle::BundleItem;
use crate::item::items::clock::ClockItem;
use crate::item::items::compass::CompassItem;
use crate::item::items::cushion::CushionItem;
use crate::item::items::debug_stick::DebugStickItem;
use crate::item::items::end_crystal::EndCrystalItem;
use crate::item::items::experience_bottle::ExperienceBottleItem;
use crate::item::items::glass_bottle::GlassBottleItem;
use crate::item::items::goat_horn::GoatHornItem;
use crate::item::items::hanging_entity::HangingEntityItem;
use crate::item::items::knowledge_book::{DiscFragmentItem, KnowledgeBookItem};
use crate::item::items::lead::LeadItem;
use crate::item::items::map::MapItem;

use crate::item::items::minecart::MinecartItem;
use crate::item::items::name_tag::NameTagItem;
use crate::item::items::on_a_stick::{CarrotOnAStickItem, WarpedFungusOnAStickItem};
use crate::item::items::place_on_water::PlaceOnWaterBlockItem;
use crate::item::items::saddle::SaddleItem;
use crate::item::items::shears::ShearsItem;
use crate::item::items::shield::ShieldItem;
use crate::item::items::spawn_egg::SpawnEggItem;
use crate::item::items::spyglass::SpyglassItem;
use crate::item::items::wind_charge::WindChargeItem;
use crate::item::items::writable_book::WritableBookItem;
use firework_rocket::FireworkRocketItem;
use fishing_rod::FishingRodItem;
use glowing_ink_sac::GlowingInkSacItem;

use super::registry::ItemRegistry;
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::item::items::potions::{LingeringPotionItem, PotionItem, SplashPotionItem};
use crate::world::World;
use arrow::ArrowItem;
use axe::AxeItem;
use bow::BowItem;
use bucket::{EmptyBucketItem, FilledBucketItem, MilkBucketItem};
use crossbow::CrossbowItem;
use dye::DyeItem;
use egg::EggItem;
use ender_eye::EnderEyeItem;
use ender_pearl::EnderPearlItem;
use hoe::HoeItem;
use honeycomb::HoneyCombItem;
use ignite::fire_charge::FireChargeItem;
use ignite::flint_and_steel::FlintAndSteelItem;
use ink_sac::InkSacItem;
use mace::MaceItem;
use pumpkin_data::data_component_impl::BlocksAttacksImpl;
use pumpkin_data::game_event::GameEvent;
use pumpkin_util::Hand;
use pumpkin_util::math::position::BlockPos;
use shovel::ShovelItem;
use snowball::SnowBallItem;
use spear::SpearItem;
use std::sync::Arc;
use swords::SwordItem;
use trident::TridentItem;

#[must_use]
pub fn default_registry() -> Arc<ItemRegistry> {
    let mut manager = ItemRegistry::default();

    manager.register(ArrowItem);
    manager.register(BowItem);
    manager.register(CrossbowItem);
    manager.register(SnowBallItem);
    manager.register(HoeItem);
    manager.register(EggItem);
    manager.register(FlintAndSteelItem);
    manager.register(SwordItem);
    manager.register(MaceItem);
    manager.register(SpearItem);
    manager.register(TridentItem);
    manager.register(FishingRodItem);
    manager.register(BrushItem);
    manager.register(CarrotOnAStickItem);
    manager.register(WarpedFungusOnAStickItem);
    manager.register(ShearsItem);
    manager.register(SaddleItem);
    manager.register(BoneMealItem);
    manager.register(GlassBottleItem);
    manager.register(ExperienceBottleItem);
    manager.register(CompassItem);
    manager.register(ClockItem);
    manager.register(WritableBookItem);
    manager.register(SpyglassItem);
    manager.register(GoatHornItem);
    manager.register(EmptyBucketItem);
    manager.register(FilledBucketItem);
    manager.register(MilkBucketItem);
    manager.register(ShovelItem);
    manager.register(SpawnEggItem);
    manager.register(AxeItem);
    manager.register(EndCrystalItem);
    manager.register(MinecartItem);
    manager.register(HoneyCombItem);
    manager.register(NameTagItem);
    manager.register(EnderEyeItem);
    manager.register(EnderPearlItem);
    manager.register(FireChargeItem);
    manager.register(DyeItem);
    manager.register(MapItem);
    manager.register(FireworkRocketItem);
    manager.register(InkSacItem);
    manager.register(GlowingInkSacItem);
    manager.register(ArmorStandItem);
    manager.register(CushionItem);
    manager.register(WindChargeItem);
    manager.register(BoatItem);
    manager.register(PotionItem);
    manager.register(SplashPotionItem);
    manager.register(LingeringPotionItem);
    manager.register(BundleItem);
    manager.register(LeadItem);
    manager.register(HangingEntityItem);
    manager.register(PlaceOnWaterBlockItem);
    manager.register(KnowledgeBookItem);
    manager.register(DiscFragmentItem);
    manager.register(ShieldItem);
    manager.register(DebugStickItem);

    Arc::new(manager)
}

/// Whether a main-hand tool use should pass so the player can raise a shield.
///
/// True when the use is the main hand, the off hand has `blocks_attacks`, and the player is not sneaking.
#[must_use]
pub const fn player_has_blocking_item_use_intent(
    main_hand: bool,
    off_hand_blocks_attacks: bool,
    sneaking: bool,
) -> bool {
    main_hand && off_hand_blocks_attacks && !sneaking
}

#[must_use]
pub fn blocking_item_use_intent(player: &Player, hand: Hand) -> bool {
    player_has_blocking_item_use_intent(
        hand == Hand::Right,
        player
            .inventory()
            .off_hand_item()
            .get_data_component::<BlocksAttacksImpl>()
            .is_some(),
        player.get_entity().is_sneaking(),
    )
}

pub fn emit_block_change(world: &World, position: BlockPos) {
    world.emit_game_event(GameEvent::BlockChange.name(), position.to_centered_f64());
}

#[cfg(test)]
mod tests {
    use super::player_has_blocking_item_use_intent;
    use pumpkin_data::data_component_impl::BlocksAttacksImpl;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;

    #[test]
    fn shield_in_the_off_hand_passes_a_main_hand_tool_use() {
        assert!(player_has_blocking_item_use_intent(true, true, false));
        assert!(!player_has_blocking_item_use_intent(true, true, true));
        assert!(!player_has_blocking_item_use_intent(false, true, false));
        assert!(!player_has_blocking_item_use_intent(true, false, false));
    }

    #[test]
    fn shield_stack_has_blocks_attacks() {
        let shield = ItemStack::new(1, &Item::SHIELD);
        assert!(shield.get_data_component::<BlocksAttacksImpl>().is_some());
        let stick = ItemStack::new(1, &Item::STICK);
        assert!(stick.get_data_component::<BlocksAttacksImpl>().is_none());
    }
}
