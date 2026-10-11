use pumpkin_protocol::java::client::play::{
    CInitializeWorldBorder, CSetBorderCenter, CSetBorderLerpSize, CSetBorderSize,
    CSetBorderWarningDelay, CSetBorderWarningDistance,
};

use crate::net::java::JavaClient;

use super::World;

/// Vanilla `WorldBorder.MAX_SIZE`. This is the diameter a world used before a
/// saved border was applied.
pub const DEFAULT_DIAMETER: f64 = 5.999_996_8E7;
pub const DEFAULT_WARNING_BLOCKS: i32 = 5;
/// Persisted default from `WorldBorder.Settings`. A brand-new live border uses 15.
pub const DEFAULT_WARNING_TIME: i32 = 300;
/// Client border packets measure the lerp in milliseconds. One tick is 50ms.
const MILLIS_PER_TICK: i64 = 50;

#[derive(Clone)]
pub struct Worldborder {
    pub center_x: f64,
    pub center_z: f64,
    /// Diameter the current lerp started from. Vanilla `MovingBorderExtent.from`.
    pub old_diameter: f64,
    /// Diameter the lerp is moving toward. Vanilla `MovingBorderExtent.to`.
    pub new_diameter: f64,
    /// Ticks left in the lerp. Vanilla `MovingBorderExtent.lerpProgress`, which
    /// `update` decrements by one each tick. Zero means the border is still.
    pub lerp_remaining: i64,
    /// Length of this lerp in ticks. Vanilla stores only the remaining time;
    /// a loaded border starts a new lerp of that length from the saved size.
    pub lerp_duration: i64,
    pub portal_teleport_boundary: i32,
    pub warning_blocks: i32,
    pub warning_time: i32,
    pub damage_per_block: f32,
    pub buffer: f32,
}

impl Worldborder {
    #[must_use]
    pub const fn new(
        x: f64,
        z: f64,
        diameter: f64,
        warning_blocks: i32,
        warning_time: i32,
    ) -> Self {
        Self {
            center_x: x,
            center_z: z,
            old_diameter: diameter,
            new_diameter: diameter,
            lerp_remaining: 0,
            lerp_duration: 0,
            portal_teleport_boundary: 29_999_984,
            warning_blocks,
            warning_time,
            damage_per_block: 0.0,
            buffer: 0.0,
        }
    }

    /// The border `World::load` used when the save has no border section.
    #[must_use]
    pub const fn vanilla_default() -> Self {
        Self::new(
            0.0,
            0.0,
            DEFAULT_DIAMETER,
            DEFAULT_WARNING_BLOCKS,
            DEFAULT_WARNING_TIME,
        )
    }

    /// Rebuilds a border from the saved centre, size, and lerp.
    ///
    /// A stationary border stores the same value for size and lerp target.
    /// A moving one stores the current diameter as the size and the destination
    /// as the lerp target, with the remaining time in milliseconds.
    #[expect(
        clippy::too_many_arguments,
        reason = "one argument per field stored in the border save"
    )]
    #[must_use]
    pub const fn from_saved(
        center_x: f64,
        center_z: f64,
        size: f64,
        lerp_target: f64,
        lerp_time: i64,
        warning_blocks: i32,
        warning_time: i32,
        damage_per_block: f32,
        safe_zone: f32,
    ) -> Self {
        let moving = lerp_time != 0;
        let mut border = Self::new(center_x, center_z, size, warning_blocks, warning_time);
        if moving {
            // `applyInitialSettings` starts a new lerp from the saved size toward
            // the target, lasting the saved remaining time.
            border.new_diameter = lerp_target;
            border.lerp_duration = lerp_time;
            border.lerp_remaining = lerp_time;
        }
        border.damage_per_block = damage_per_block;
        border.buffer = safe_zone;
        border
    }

    /// Vanilla `MovingBorderExtent.calculateSize`.
    #[must_use]
    pub fn current_size(&self) -> f64 {
        if self.lerp_remaining <= 0 || self.lerp_duration <= 0 {
            return self.new_diameter;
        }
        let elapsed = self.lerp_duration - self.lerp_remaining;
        let progress = ticks_to_f64(elapsed) / ticks_to_f64(self.lerp_duration);
        if progress >= 1.0 {
            self.new_diameter
        } else {
            self.old_diameter + (self.new_diameter - self.old_diameter) * progress
        }
    }

    /// Diameter written as `size`: the interpolated diameter, not the start.
    #[must_use]
    pub fn saved_size(&self) -> f64 {
        self.current_size()
    }

    /// `lerp_time` written to disk. Vanilla `getLerpTime` is the remaining ticks.
    #[must_use]
    pub const fn saved_lerp_time(&self) -> i64 {
        self.lerp_remaining
    }

    /// Vanilla `WorldBorder.tick`: one tick consumes one unit of lerp time.
    pub const fn tick(&mut self) {
        if self.lerp_remaining <= 0 {
            return;
        }
        self.lerp_remaining -= 1;
        if self.lerp_remaining <= 0 {
            self.old_diameter = self.new_diameter;
            self.lerp_duration = 0;
            self.lerp_remaining = 0;
        }
    }

    pub fn init_client(&self, client: &JavaClient) {
        let from = self.current_size();
        let millis = self.lerp_remaining.saturating_mul(MILLIS_PER_TICK);
        if let Ok(data) = client.serialize_packet(&CInitializeWorldBorder::new(
            self.center_x,
            self.center_z,
            from,
            self.new_diameter,
            millis.into(),
            self.portal_teleport_boundary.into(),
            self.warning_blocks.into(),
            self.warning_time.into(),
        )) {
            client.try_enqueue_packet(data);
        }
    }

    pub fn set_center(&mut self, world: &World, x: f64, z: f64) {
        self.center_x = x;
        self.center_z = z;

        world.broadcast_packet_all(&CSetBorderCenter::new(self.center_x, self.center_z));
    }

    pub fn set_diameter(&mut self, world: &World, diameter: f64, speed: Option<i64>) {
        match speed {
            Some(millis) if millis > 0 => {
                let ticks = (millis / MILLIS_PER_TICK).max(1);
                self.old_diameter = self.current_size();
                self.new_diameter = diameter;
                self.lerp_duration = ticks;
                self.lerp_remaining = ticks;
                world.broadcast_packet_all(&CSetBorderLerpSize::new(
                    self.old_diameter,
                    self.new_diameter,
                    ticks.saturating_mul(MILLIS_PER_TICK).into(),
                ));
            }
            _ => {
                self.old_diameter = diameter;
                self.new_diameter = diameter;
                self.lerp_duration = 0;
                self.lerp_remaining = 0;
                world.broadcast_packet_all(&CSetBorderSize::new(self.new_diameter));
            }
        }
    }

    pub fn add_diameter(&mut self, world: &World, offset: f64, speed: Option<i64>) {
        self.set_diameter(world, self.new_diameter + offset, speed);
    }

    pub fn set_warning_delay(&mut self, world: &World, delay: i32) {
        self.warning_time = delay;

        world.broadcast_packet_all(&CSetBorderWarningDelay::new(self.warning_time.into()));
    }

    pub fn set_warning_distance(&mut self, world: &World, distance: i32) {
        self.warning_blocks = distance;

        world.broadcast_packet_all(&CSetBorderWarningDistance::new(self.warning_blocks.into()));
    }

    pub const fn set_damage_buffer(&mut self, buffer: f32) {
        self.buffer = buffer;
    }

    pub const fn set_damage_per_block(&mut self, damage: f32) {
        self.damage_per_block = damage;
    }

    pub fn reset(&mut self, world: &World) {
        self.center_x = 0.0;
        self.center_z = 0.0;
        self.old_diameter = 29_999_984.0;
        self.new_diameter = 29_999_984.0;
        self.lerp_remaining = 0;
        self.lerp_duration = 0;
        self.portal_teleport_boundary = 29_999_984;
        self.warning_blocks = 5;
        self.warning_time = 15;
        self.damage_per_block = 0.2;
        self.buffer = 5.0;

        world.broadcast_packet_all(&CInitializeWorldBorder::new(
            self.center_x,
            self.center_z,
            self.old_diameter,
            self.new_diameter,
            0i64.into(),
            self.portal_teleport_boundary.into(),
            self.warning_blocks.into(),
            self.warning_time.into(),
        ));
    }

    #[must_use]
    pub fn contains(&self, x: f64, z: f64) -> bool {
        let half = self.current_size() / 2.0;
        let min_x = self.center_x - half;
        let max_x = self.center_x + half;
        let min_z = self.center_z - half;
        let max_z = self.center_z + half;
        x >= min_x && x < max_x && z >= min_z && z < max_z
    }

    #[must_use]
    pub fn contains_block(&self, x: i32, z: i32) -> bool {
        self.contains(f64::from(x), f64::from(z))
            && self.contains(f64::from(x + 1), f64::from(z + 1))
    }

    #[must_use]
    pub fn clamp_block(&self, x: i32, z: i32) -> (i32, i32) {
        let half = self.current_size() / 2.0;
        // A border narrower than one block spans no block boundary, leaving `max`
        // below `min`. `Ord::clamp` panics on an inverted range, so collapse the
        // range onto the single block that holds the centre instead.
        let min_x = (self.center_x - half).floor() as i32;
        let max_x = ((self.center_x + half).floor() as i32 - 1).max(min_x);
        let min_z = (self.center_z - half).floor() as i32;
        let max_z = ((self.center_z + half).floor() as i32 - 1).max(min_z);
        (x.clamp(min_x, max_x), z.clamp(min_z, max_z))
    }
}

const fn ticks_to_f64(ticks: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a border lerp lasts seconds, not long enough to lose tick precision"
    )]
    {
        ticks as f64
    }
}

#[cfg(test)]
mod tests {
    use super::Worldborder;

    fn centered_border(diameter: f64) -> Worldborder {
        Worldborder::new(0.0, 0.0, diameter, 5, 300)
    }

    /// `find_safe_location` only falls through to the `clamp_block` fallback
    /// because a zero-width border rejects every candidate it scans: `contains`
    /// reduces to `x >= center && x < center`, which no coordinate satisfies.
    #[test]
    fn a_zero_width_border_contains_no_block() {
        let border = centered_border(0.0);

        assert!(!border.contains_block(0, 0));
        assert!(
            (-32..=32).all(|x| (-32..=32).all(|z| !border.contains_block(x, z))),
            "a zero-width border should contain no block in the portal search area"
        );
    }

    /// `/worldborder set 0` is accepted, because the command's consumer is
    /// `BoundedNumArgumentConsumer::new().min(0.0)` and that bound is inclusive.
    #[test]
    fn clamp_block_handles_a_zero_width_border() {
        let border = centered_border(0.0);

        assert_eq!(border.clamp_block(0, 0), (0, 0));
        assert_eq!(border.clamp_block(100, -100), (0, 0));
    }

    /// A centre off the block grid puts both edges inside the same block, so the
    /// border spans no block boundary even though its diameter is not zero.
    #[test]
    fn clamp_block_handles_a_border_narrower_than_one_block() {
        let border = Worldborder::new(0.5, 0.5, 0.5, 5, 300);

        assert_eq!(border.clamp_block(100, -100), (0, 0));
    }

    #[test]
    fn clamp_block_still_clamps_a_normal_border() {
        let border = centered_border(10.0);

        assert_eq!(border.clamp_block(100, 100), (4, 4));
        assert_eq!(border.clamp_block(-100, -100), (-5, -5));
        assert_eq!(border.clamp_block(2, -3), (2, -3));
    }

    /// A saved moving border counts down. Saving again writes the interpolated
    /// size and the ticks still left, so the next load does not replay the move.
    #[test]
    fn moving_border_counts_down() {
        let mut border = Worldborder::from_saved(0.0, 0.0, 0.0, 100.0, 4, 5, 300, 0.0, 0.0);
        assert_eq!(border.saved_lerp_time(), 4);
        assert_eq!(border.saved_size(), 0.0);

        border.tick();
        assert_eq!(border.saved_lerp_time(), 3);
        assert!((border.saved_size() - 25.0).abs() < 1e-9);

        border.tick();
        border.tick();
        border.tick();
        assert_eq!(border.saved_lerp_time(), 0);
        assert_eq!(border.saved_size(), 100.0);
        assert_eq!(border.new_diameter, 100.0);
    }
}
