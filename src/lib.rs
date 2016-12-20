#[macro_use]
extern crate mopa;
extern crate fnv;
extern crate crossbeam;
extern crate rayon;
extern crate arrayvec;
extern crate atom;
extern crate tuple_utils;

pub mod entities;
pub mod world;
pub mod bitset;
pub mod join;
pub mod resource;

#[cfg(test)]
mod tests {
    use entities::*;
    use world::*;
    use resource::*;

    struct Position {
        x: f32,
        y: f32,
        z: f32
    }

    type Positions = VecResource<Position>;

    struct Velocity {
        x: f32,
        y: f32,
        z: f32
    }

    type Velocities = VecResource<Velocity>;

    #[test]
    fn example() {
        let mut world = World::new();
        world.register_resource(Positions::new());
        world.register_resource(Velocities::new());

        let setup = world.system_r0w2(|tx, velocities: &mut Velocities, positions: &mut Positions| {
            for i in 0..1000 {
                let e = tx.create();
                let i = (i as f32) * 10f32;
                velocities.add(e.index(), Velocity { x: i, y: i, z: i });
                positions.add(e.index(), Position { x: i, y: i, z: i });
            }
        });

        world.update(&mut vec![setup]);

        let update_positions = world.system_r1w1(|_, velocities: &Velocities, positions: &mut Positions| {
            println!("Updating positions");

            iter_entities_r1w1(velocities, positions, |iter, v, p| {
                for e in iter {
                    let position = unsafe { p.get_unchecked_mut(e) };
                    let velocity = unsafe { v.get_unchecked(e) };
                    position.x += velocity.x;
                    position.y += velocity.y;
                    position.z += velocity.z;
                }
            });
        });

        let mut systems = vec![update_positions];

        world.update(&mut systems);
    }
}
