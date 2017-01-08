#![deny(missing_docs)]

//! # Constellation ECS
//!
//! A data-oriented entity component system optimized for cache coherent resource access
//! and parallel system execution.
//!
//! Constellation does not have any native understanding of a "component". Instead, the library
//! is concerned about ensuring safe concurrent access to shared resources, while providing direct
//! access to each resource's own APIs. These resources may store per-entity data such as
//! positions, or may represent application wide services such as asset loaders or input devices.
//!
//! Systems request read or write access to a set of resources, which can then be scheduled to
//! be potentially executed in parallel by recording them into a `SystemCommandBuffer` and
//! executing the command buffer within a `World`.
//!
//! # Examples
//!
//! Defining Resources:
//!
//! ```
//! # use self::constellation::*;
//! // Per-entity position data.
//! struct Position {
//!     x: f32,
//!     y: f32,
//!     z: f32
//! }
//!
//! // Store position data into a vector resource
//! type Positions = VecResource<Position>;
//!
//! // Per-entity debug names.
//! struct DebugName {
//!     name: String
//! }
//!
//! // Store debug names in a map resource
//! type DebugNames = MapResource<DebugName>;
//!
//! let mut world = World::new();
//! world.register_resource(Positions::new());
//! world.register_resource(DebugNames::new());
//! ```
//!
//! Update the world with Systems:
//!
//! ```
//! # use self::constellation::*;
//! # #[derive(Debug)]
//! # struct Position {
//! #     x: f32,
//! #     y: f32,
//! #     z: f32
//! # }
//! # type Positions = VecResource<Position>;
//! #
//! # struct Velocity {
//! #     x: f32,
//! #     y: f32,
//! #     z: f32
//! # }
//! # type Velocities = VecResource<Velocity>;
//! #
//! # struct DebugName {
//! #     name: String
//! # }
//! # type DebugNames = MapResource<DebugName>;
//! #
//! # let mut world = World::new();
//! # world.register_resource(Positions::new());
//! # world.register_resource(Velocities::new());
//! # world.register_resource(DebugNames::new());
//! let mut update = SystemCommandBuffer::default();
//! update.queue_systems(|scope| {
//!     scope.run_r1w1(|ctx, velocities: &Velocities, positions: &mut Positions| {
//!         println!("Updating positions");
//!
//!         // iterate through all entities with data in both position and velocity resources
//!         ctx.iter().r1w1(velocities, positions, |entity_iter, v, p| {
//!             // `v` and `p` allow (mutable in the case of `p`) access to entity data inside
//!             // the resource without the ability to add or remove entities from the resource
//!             // - which would otherwise invalidate the iterator
//!             for e in entity_iter {
//!                 // unchecked getters offer direct indexing
//!                 let position = unsafe { p.get_unchecked_mut(e) };
//!                 let velocity = unsafe { v.get_unchecked(e) };
//!                 position.x += velocity.x;
//!                 position.y += velocity.y;
//!                 position.z += velocity.z;
//!             }
//!         });
//!     });
//!
//!     scope.run_r2w0(|ctx, names: &DebugNames, positions: &Positions| {
//!         println!("Printing positions");
//!
//!         ctx.iter().r2w0(names, positions, |entity_iter, n, p| {
//!             for e in entity_iter {
//!                 println!("Entity {} is at {:?}",
//!                          n.get(e).unwrap().name,
//!                          p.get(e).unwrap());
//!             }
//!         });
//!     });
//! });
//!
//! world.run(&mut update);
//! ```
//!
//! # Parallel System Execution
//!
//! Systems queued into a command buffer within a single call to `queue_systems` may be executed
//! in parallel by the world.
//!
//! The order in which systems are queued is significant in one way: the scheduler guarantees that
//! any changes to resources will always be observed in the same order in which systems were
//! queued.
//!
//! For example, given two systems - `ReadPositions` and `WritePositions` - if *WritePositions* was
//! queued before *ReadPositions*, then it is guarenteed that *ReadPositions* will see any changes
//! made by *WritePositions*. Conversely, if the order were to be swapped, then *ReadPositions*
//! is guaranteed to *not* observe the changes made by *WritePositions*.
//!
//! There is one exception to this. Entity deletions are committed when all concurrently executing
//! systems have completed. This behavior is deterministic, but not always obvious. If you wish to
//! ensure that entity deletions from one system are always seen by a later system, then queue
//! the two systems in separate `queue_systems` calls.

extern crate fnv;
extern crate crossbeam;
extern crate rayon;
extern crate arrayvec;
extern crate atom;
extern crate tuple_utils;
#[macro_use]
extern crate bitflags;

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
        z: f32,
    }

    type Positions = VecResource<Position>;

    struct Velocity {
        x: f32,
        y: f32,
        z: f32,
    }

    type Velocities = VecResource<Velocity>;

    #[test]
    fn example() {
        let mut world = World::new();
        world.register_resource(Positions::new());
        world.register_resource(Velocities::new());

        let mut setup = SystemCommandBuffer::default();
        setup.queue_systems(|scope| {
            scope.run_r0w2(|tx, velocities: &mut Velocities, positions: &mut Positions| {
                for i in 0..1000 {
                    let e = tx.create();
                    let i = (i as f32) * 10f32;
                    velocities.add(e, Velocity { x: i, y: i, z: i });
                    positions.add(e, Position { x: i, y: i, z: i });
                }
            });
        });

        world.run(&mut setup);

        let mut update = SystemCommandBuffer::default();
        update.queue_systems(|scope| {
            scope.run_r1w1(|ctx, velocities: &Velocities, positions: &mut Positions| {
                println!("Updating positions");

                ctx.iter().components_r1w1(velocities, positions, |e, v, p| {
                    p.x += v.x;
                    p.y += v.y;
                    p.z += v.z;

                    ctx.destroy(e);
                });
            });
        });

        world.run(&mut update);
    }
}
