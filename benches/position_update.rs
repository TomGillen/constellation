#![feature(test)]

extern crate test;
use test::Bencher;

extern crate constellation;
extern crate rayon;

use constellation::{SystemCommandBuffer, VecResource, World, SequentialExecute};

#[macro_use]
extern crate serde_derive;
extern crate serde;

pub const TOTAL_ENTITIES: usize = 200000;
pub const MOBILE_ENTITIES: usize = 100000;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionData {
    pub dx: f32,
    pub dy: f32,
    pub ddx: f32,
    pub ddy: f32
}

type Positions = VecResource<Position>;
type Mobile = VecResource<MotionData>;

fn setup() -> World {
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        scope.run_r0w2(|ctx, pos: &mut Positions, vel: &mut Mobile| {
            for i in 0..TOTAL_ENTITIES {
                let e = ctx.create();
                pos.add(e, Position { x: 0.0, y: 0.0 });

                if i < MOBILE_ENTITIES {
                    vel.add(e, MotionData { dx: 1.0, dy: 1.0, ddx: 0.5, ddy: 0.5 });
                }
            }
        });
    });

    let mut world = World::new();
    world.register_resource(Positions::new());
    world.register_resource(Mobile::new());

    world.run(&mut update);

    world
}

#[bench]
fn position_update_setup(b: &mut Bencher) {
    b.iter(|| setup());
}

#[bench]
fn position_update_sequential(b: &mut Bencher) {
    let mut world = setup();
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        scope.run_r0w2(|ctx, pos: &mut Positions, vel: &mut Mobile| {
            ctx.iter_r0w2(pos, vel).components(|_, pos, vel| {
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
fn position_update(b: &mut Bencher) {
    let mut world = setup();
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        scope.run_r0w2(|ctx, pos: &mut Positions, vel: &mut Mobile| {
            ctx.iter_r0w2(pos, vel).components(|_, pos, vel| {
                vel.dx += vel.ddx;
                vel.dy += vel.ddy;
                pos.x += vel.dx;
                pos.y += vel.dy;
            });
        });
    });

    b.iter(|| world.run(&mut update));
}