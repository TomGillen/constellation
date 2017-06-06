#![feature(test)]

extern crate test;
use test::Bencher;

extern crate constellation;
extern crate rayon;

use constellation::{SystemCommandBuffer, VecResource, World, SequentialExecute};

#[macro_use]
extern crate serde_derive;
extern crate serde;

pub const TOTAL_ENTITIES: usize = 1000;
pub const G: f32 = 9.81;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RigidBody {
    pub mass: f32,    
    pub x: f32,
    pub y: f32    
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicBody {
    pub dx: f32,
    pub dy: f32,
    pub ddx: f32,
    pub ddy: f32,
}

type Bodies = VecResource<RigidBody>;
type Motion = VecResource<DynamicBody>;

fn setup() -> World {
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        scope.run_r0w2(|ctx, bodies: &mut Bodies, motion: &mut Motion| {
            for i in 0..TOTAL_ENTITIES {
                let e = ctx.create();
                bodies.add(e, RigidBody { mass: 10.0, x: i as f32, y: i as f32 });
                motion.add(e, DynamicBody { dx: -(i as f32), dy: -(i as f32), ddx: 0.0, ddy: 0.0 });
            }
        });
    });

    let mut world = World::new();
    world.register_resource(Bodies::new());
    world.register_resource(Motion::new());

    world.run(&mut update);

    world
}

#[bench]
fn nbody_setup(b: &mut Bencher) {
    b.iter(|| setup());
}

#[bench]
fn nbody_sequential(b: &mut Bencher) {
    let mut world = setup();
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        // calculate acceleration
        scope.run_r1w1(|ctx, bodies: &Bodies, motion: &mut Motion| {
            ctx.iter_r1w1(bodies, motion).components(|_, pos, vel| {
                for (_, b) in bodies.iter() {
                    let rx = b.x - pos.x;
                    let ry = b.y - pos.y;
                    let r = (rx * rx + ry * ry).sqrt();

                    if r != 0.0 {
                        let f = (G * b.mass) / r;
                        vel.ddx += (rx / r) * f;
                        vel.ddy += (ry / r) * f;
                    }
                }
            });
        });

        // integrate positions
        scope.run_r0w2(|ctx, bodies: &mut Bodies, motion: &mut Motion| {
            ctx.iter_r0w2(bodies, motion).components(|_, pos, vel| {
                vel.dx += vel.ddx;
                vel.dy += vel.ddy;
                pos.x += vel.dx;
                pos.y += vel.dy;
            });
        });
    });

    b.iter(|| world.run_sequential(&mut update, SequentialExecute::ParallelBatchedCommit));
}

#[bench]
fn nbody(b: &mut Bencher) {
    let mut world = setup();
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        // calculate acceleration
        scope.run_r1w1(|ctx, bodies: &Bodies, motion: &mut Motion| {
            ctx.iter_r1w1(bodies, motion).components(|_, pos, vel| {
                for (_, b) in bodies.iter() {
                    let rx = b.x - pos.x;
                    let ry = b.y - pos.y;
                    let r = (rx * rx + ry * ry).sqrt();

                    if r != 0.0 {
                        let f = (G * b.mass) / r;
                        vel.ddx += (rx / r) * f;
                        vel.ddy += (ry / r) * f;
                    }
                }
            });
        });

        // integrate positions
        scope.run_r0w2(|ctx, bodies: &mut Bodies, motion: &mut Motion| {
            ctx.iter_r0w2(bodies, motion).components(|_, pos, vel| {
                vel.dx += vel.ddx;
                vel.dy += vel.ddy;
                pos.x += vel.dx;
                pos.y += vel.dy;
            });
        });
    });

    b.iter(|| world.run(&mut update));
}