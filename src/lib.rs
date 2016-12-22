#[macro_use]
extern crate mopa;
extern crate fnv;
extern crate crossbeam;
extern crate rayon;
extern crate arrayvec;
extern crate atom;
extern crate tuple_utils;

mod entities;
mod world;
mod resource;

pub mod bitset;
pub mod join;

pub use entities::*;
pub use world::*;
pub use resource::*;

#[cfg(test)]
mod tests {
    use super::*;

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

        let mut setup = SystemCommandBuffer::new();
        setup.queue_systems(|scope| {
            scope.run_r0w2(|tx, velocities: &mut Velocities, positions: &mut Positions| {
                for i in 0..1000 {
                    let e = tx.create();
                    let i = (i as f32) * 10f32;
                    velocities.add(e.index(), Velocity { x: i, y: i, z: i });
                    positions.add(e.index(), Position { x: i, y: i, z: i });
                }
            });
        });

        world.run(&mut setup);

        let mut update = SystemCommandBuffer::new();
        update.queue_systems(|scope| {
            scope.run_r1w1(|_, velocities: &Velocities, positions: &mut Positions| {
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
        });

        world.run(&mut update);
    }
}
