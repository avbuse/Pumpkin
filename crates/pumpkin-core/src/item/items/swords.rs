use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag;
use pumpkin_util::GameMode;

pub struct SwordItem;

impl ItemMetadata for SwordItem {
    fn ids() -> Box<[u16]> {
        tag::Item::MINECRAFT_SWORDS.1.into()
    }
}

impl ItemBehaviour for SwordItem {
    fn can_mine(&self, player: &Player) -> bool {
        player.gamemode.load() != GameMode::Creative
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SwordItem {
    /// Swords are the held items that can sweep. Damage still comes from the
    /// attack damage attribute and the cooldown scale, not from this type.
    #[must_use]
    pub fn enables_sweep(stack: &ItemStack) -> bool {
        stack.is_sword()
    }
}
