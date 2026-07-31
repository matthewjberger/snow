use crate::ecs::{CRYSTAL, Crystal};
use nightshade::prelude::*;

/// Columns in the data texture, and so the most prisms that can be drawn.
///
/// A cap on the draw rather than on the population: the query decides what is
/// alive, and anything past this many simply does not reach the texture, which
/// at two formations' worth never happens.
pub const CRYSTAL_MAX: usize = 96;

/// Vertices per crystal: two rings of six, plus an apex.
pub const CRYSTAL_VERTS: usize = 13;
pub const CRYSTAL_RING: usize = 6;

/// Seconds a prism takes to retreat back into the drift once its life is up.
const SUBLIMATE: f32 = 6.0;

/// Plants one prism as its own entity.
#[allow(clippy::too_many_arguments)]
pub fn plant(
    world: &mut World,
    position: [f32; 3],
    axis: [f32; 3],
    height: f32,
    radius: f32,
    grow_seconds: f32,
    life: f32,
) {
    let entity = world.spawn_with((Crystal {
        position,
        axis,
        height,
        radius,
        seed: (position[0] * 0.137 + position[2] * 0.311).rem_euclid(1.0),
        age: 0.0,
        life,
        grow: grow_seconds.max(0.05),
    },));
    let _ = entity;
}

/// Ages every prism and retires the ones that have finished sublimating.
///
/// The despawn is gathered first and applied second: removing an entity while a
/// query is walking it is the one thing the two-pass pattern exists to prevent.
pub fn age(world: &mut World, delta_time: f32) {
    if delta_time <= 0.0 {
        return;
    }
    let mut finished: Vec<Entity> = Vec::new();

    world
        .query::<(&mut Crystal,)>()
        .for_each(|entity, (crystal,)| {
            crystal.age += delta_time;
            if crystal.age >= crystal.life + SUBLIMATE {
                finished.push(entity);
            }
        });

    for entity in finished {
        world.despawn_recursive(entity);
    }
}

/// How far grown a prism is, from nothing, through its standing life, back to
/// nothing.
///
/// The retreat is a shrink rather than a fade, so a formation goes back into the
/// drift it came out of and nothing pops.
fn growth(crystal: &Crystal) -> f32 {
    if crystal.age < crystal.grow {
        crystal.age / crystal.grow
    } else if crystal.age < crystal.life {
        1.0
    } else {
        (1.0 - (crystal.age - crystal.life) / SUBLIMATE).max(0.0)
    }
}

/// Gathers the live prisms into the data texture the shape library reads, and
/// reports how many made it.
///
/// Three rows per crystal, one column each: position and height, axis and
/// radius, then growth and seed.
pub fn gather(world: &World, texels: &mut [f32]) -> usize {
    texels.fill(0.0);
    let width = CRYSTAL_MAX * 4;
    let mut count = 0;

    for (_, (crystal,)) in world.query_ref::<(&Crystal,)>().iter() {
        if count >= CRYSTAL_MAX {
            break;
        }
        let mut offset = count * 4;
        texels[offset..offset + 4].copy_from_slice(&[
            crystal.position[0],
            crystal.position[1],
            crystal.position[2],
            crystal.height,
        ]);
        offset += width;
        texels[offset..offset + 4].copy_from_slice(&[
            crystal.axis[0],
            crystal.axis[1],
            crystal.axis[2],
            crystal.radius,
        ]);
        offset += width;
        texels[offset..offset + 4].copy_from_slice(&[growth(crystal), crystal.seed, 0.0, 0.0]);
        count += 1;
    }
    count
}

/// Retires every prism at once, for the settings toggle.
pub fn clear(world: &mut World) {
    let standing: Vec<Entity> = world.ecs.worlds[GAME].query_entities(CRYSTAL).collect();
    for entity in standing {
        world.despawn_recursive(entity);
    }
}
