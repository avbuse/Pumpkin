use crate::block_transformer::{AXE, DropStrategy, HOE, SHOVEL};
use crate::item::Item;
use crate::sound::Sound;
use crate::tag::Item as ItemTag;
use crate::world::WorldEvent;
use crate::{Block, BlockDirection};

fn world_of(
    clicked: &'static Block,
    above: &'static Block,
) -> impl Fn(i8, i8, i8) -> &'static Block {
    move |dx, dy, dz| {
        if (dx, dy, dz) == (0, 1, 0) {
            above
        } else if (dx, dy, dz) == (0, 0, 0) {
            clicked
        } else {
            &Block::AIR
        }
    }
}

fn property(block: &Block, state: crate::BlockStateId, name: &str) -> Option<&'static str> {
    block
        .properties(state)?
        .to_props()
        .into_iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

#[test]
fn axe_strips_oak_log_and_keeps_axis() {
    let state = Block::OAK_LOG
        .from_properties(&[("axis", "x")])
        .to_state_id(&Block::OAK_LOG);
    let result = AXE
        .transform(
            &Block::OAK_LOG,
            state,
            BlockDirection::North,
            &world_of(&Block::OAK_LOG, &Block::STONE),
        )
        .expect("oak log strips");
    assert_eq!(result.target_block, &Block::STRIPPED_OAK_LOG);
    assert_eq!(
        property(result.target_block, result.new_state_id, "axis"),
        Some("x")
    );
    assert_eq!(result.entry.sound, Some(Sound::ItemAxeStrip));
    assert_eq!(result.entry.item_damage_per_use, 1);
}

#[test]
fn axe_scrapes_one_oxidation_stage_and_wax() {
    let scraped = AXE
        .transform(
            &Block::EXPOSED_COPPER,
            Block::EXPOSED_COPPER.default_state.id,
            BlockDirection::Up,
            &world_of(&Block::EXPOSED_COPPER, &Block::AIR),
        )
        .expect("exposed copper scrapes");
    assert_eq!(scraped.target_block, &Block::COPPER_BLOCK);
    assert_eq!(scraped.entry.sound, Some(Sound::ItemAxeScrape));
    assert_eq!(scraped.entry.particle, Some(WorldEvent::ParticlesScrape));

    let oxidized = AXE
        .transform(
            &Block::OXIDIZED_COPPER,
            Block::OXIDIZED_COPPER.default_state.id,
            BlockDirection::Up,
            &world_of(&Block::OXIDIZED_COPPER, &Block::AIR),
        )
        .expect("oxidized copper scrapes one stage");
    assert_eq!(oxidized.target_block, &Block::WEATHERED_COPPER);

    let unwaxed = AXE
        .transform(
            &Block::WAXED_OXIDIZED_COPPER,
            Block::WAXED_OXIDIZED_COPPER.default_state.id,
            BlockDirection::Up,
            &world_of(&Block::WAXED_OXIDIZED_COPPER, &Block::AIR),
        )
        .expect("wax scrapes off");
    assert_eq!(unwaxed.target_block, &Block::OXIDIZED_COPPER);
    assert_eq!(unwaxed.entry.sound, Some(Sound::ItemAxeWaxOff));
    assert_eq!(unwaxed.entry.particle, Some(WorldEvent::ParticlesWaxOff));
}

#[test]
fn axe_scrapes_copper_door_and_keeps_half() {
    let state = Block::EXPOSED_COPPER_DOOR
        .from_properties(&[
            ("facing", "west"),
            ("half", "upper"),
            ("hinge", "right"),
            ("open", "true"),
            ("powered", "false"),
        ])
        .to_state_id(&Block::EXPOSED_COPPER_DOOR);
    let result = AXE
        .transform(
            &Block::EXPOSED_COPPER_DOOR,
            state,
            BlockDirection::Up,
            &world_of(&Block::EXPOSED_COPPER_DOOR, &Block::AIR),
        )
        .expect("copper door scrapes");
    assert_eq!(result.target_block, &Block::COPPER_DOOR);
    assert_eq!(
        property(result.target_block, result.new_state_id, "half"),
        Some("upper")
    );
    assert_eq!(
        property(result.target_block, result.new_state_id, "facing"),
        Some("west")
    );
    assert_eq!(
        property(result.target_block, result.new_state_id, "open"),
        Some("true")
    );
}

#[test]
fn shovel_makes_a_path_only_with_air_above_and_not_from_below() {
    let grass = Block::GRASS_BLOCK.default_state.id;
    let above_air = world_of(&Block::GRASS_BLOCK, &Block::AIR);
    let made = SHOVEL
        .transform(&Block::GRASS_BLOCK, grass, BlockDirection::Up, &above_air)
        .expect("grass under air becomes a path");
    assert_eq!(made.target_block, &Block::DIRT_PATH);
    assert_eq!(made.entry.sound, Some(Sound::ItemShovelFlatten));
    assert_eq!(made.entry.item_damage_per_use, 1);

    let cave_air = world_of(&Block::GRASS_BLOCK, &Block::CAVE_AIR);
    assert!(
        SHOVEL
            .transform(&Block::GRASS_BLOCK, grass, BlockDirection::Up, &cave_air)
            .is_some(),
        "cave air is air"
    );

    let blocked = world_of(&Block::GRASS_BLOCK, &Block::STONE);
    assert!(
        SHOVEL
            .transform(&Block::GRASS_BLOCK, grass, BlockDirection::Up, &blocked)
            .is_none()
    );
    let water = world_of(&Block::GRASS_BLOCK, &Block::WATER);
    assert!(
        SHOVEL
            .transform(&Block::GRASS_BLOCK, grass, BlockDirection::Up, &water)
            .is_none()
    );
    assert!(
        SHOVEL
            .transform(&Block::GRASS_BLOCK, grass, BlockDirection::Down, &above_air)
            .is_none()
    );
}

#[test]
fn hoe_tills_dirt_only_with_air_above_and_not_from_below() {
    let dirt = Block::DIRT.default_state.id;
    let above_air = world_of(&Block::DIRT, &Block::AIR);
    let tilled = HOE
        .transform(&Block::DIRT, dirt, BlockDirection::Up, &above_air)
        .expect("dirt under air becomes farmland");
    assert_eq!(tilled.target_block, &Block::FARMLAND);
    assert_eq!(
        property(tilled.target_block, tilled.new_state_id, "moisture"),
        Some("0")
    );
    assert_eq!(tilled.entry.sound, Some(Sound::ItemHoeTill));
    assert_eq!(tilled.entry.item_damage_per_use, 1);

    let blocked = world_of(&Block::DIRT, &Block::STONE);
    assert!(
        HOE.transform(&Block::DIRT, dirt, BlockDirection::Up, &blocked)
            .is_none()
    );
    assert!(
        HOE.transform(&Block::DIRT, dirt, BlockDirection::Down, &above_air)
            .is_none()
    );

    let grass = world_of(&Block::GRASS_BLOCK, &Block::AIR);
    let from_grass = HOE
        .transform(
            &Block::GRASS_BLOCK,
            Block::GRASS_BLOCK.default_state.id,
            BlockDirection::Up,
            &grass,
        )
        .expect("grass tills");
    assert_eq!(from_grass.target_block, &Block::FARMLAND);
}

#[test]
fn hoe_turns_coarse_dirt_into_dirt_and_rooted_dirt_drops_roots() {
    let coarse = world_of(&Block::COARSE_DIRT, &Block::AIR);
    let result = HOE
        .transform(
            &Block::COARSE_DIRT,
            Block::COARSE_DIRT.default_state.id,
            BlockDirection::Up,
            &coarse,
        )
        .expect("coarse dirt becomes dirt");
    assert_eq!(result.target_block, &Block::DIRT);

    let covered = world_of(&Block::COARSE_DIRT, &Block::STONE);
    assert!(
        HOE.transform(
            &Block::COARSE_DIRT,
            Block::COARSE_DIRT.default_state.id,
            BlockDirection::Up,
            &covered,
        )
        .is_none()
    );

    let rooted = world_of(&Block::ROOTED_DIRT, &Block::STONE);
    let roots = HOE
        .transform(
            &Block::ROOTED_DIRT,
            Block::ROOTED_DIRT.default_state.id,
            BlockDirection::Down,
            &rooted,
        )
        .expect("rooted dirt tills from below without air");
    assert_eq!(roots.target_block, &Block::DIRT);
    assert_eq!(roots.entry.loot, Some("minecraft:till/rooted_dirt"));
    assert_eq!(roots.entry.drop_strategy, Some(DropStrategy::ClickedFace));
}

#[test]
fn stick_is_not_an_axe_shovel_or_hoe() {
    let stick = Item::STICK.id;
    assert!(!ItemTag::MINECRAFT_AXES.1.contains(&stick));
    assert!(!ItemTag::MINECRAFT_SHOVELS.1.contains(&stick));
    assert!(!ItemTag::MINECRAFT_HOES.1.contains(&stick));
    assert!(
        AXE.transform(
            &Block::STONE,
            Block::STONE.default_state.id,
            BlockDirection::Up,
            &world_of(&Block::STONE, &Block::AIR),
        )
        .is_none()
    );
}
