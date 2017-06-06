#![feature(test)]

extern crate test;
use test::Bencher;

extern crate constellation;
extern crate rayon;

use constellation::{SystemCommandBuffer, VecResource, MapResource, World, SequentialExecute};

#[macro_use]
extern crate serde_derive;
extern crate serde;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct A {
    pub a: f32
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct B {
    pub b: f32
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct C {
    pub c: f32
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct D {
    pub d: f32
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct E {
    pub e: f32
}

type As = VecResource<A>;
type Bs = VecResource<B>;
type Cs = MapResource<C>;
type Ds = MapResource<D>;
type Es = MapResource<E>;

fn setup() -> World {
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        scope.run_r0w5(|ctx, a: &mut As, b: &mut Bs, c: &mut Cs, d: &mut Ds, e: &mut Es| {
            for i in 0..1000usize {
                let entity = ctx.create();
                a.add(entity, A { a: i as f32 });
            }

            for i in 0..1000usize {
                let entity = ctx.create();
                a.add(entity, A { a: i as f32 });
                b.add(entity, B { b: i as f32 });
            }

            for i in 0..1000usize {
                let entity = ctx.create();
                a.add(entity, A { a: i as f32 });
                c.add(entity, C { c: i as f32 });
            }

            for i in 0..1000usize {
                let entity = ctx.create();
                d.add(entity, D { d: i as f32 });
                e.add(entity, E { e: i as f32 });
            }
        });
    });

    let mut world = World::new();
    world.register_resource(As::new());
    world.register_resource(Bs::new());
    world.register_resource(Cs::new());
    world.register_resource(Ds::new());
    world.register_resource(Es::new());

    world.run(&mut update);

    world
}

fn setup_processing() -> SystemCommandBuffer<()> {
    let mut update = SystemCommandBuffer::default();

    update.queue_systems(|scope| {
        // read a, write b
        scope.run_r1w1(|ctx, a: &As, b: &mut Bs| {
            ctx.iter_r1w1(a, b).components(|_, a_component, b_component| {
                let mut total = a_component.a;
                for (_, other) in a.iter() {
                    total += other.a;
                }

                b_component.b = total;
            });
        });

        // read a, write c
        scope.run_r1w1(|ctx, a: &As, c: &mut Cs| {
            ctx.iter_r1w1(a, c).components(|_, a_component, c_component| {
                let mut total = a_component.a;
                for (_, other) in a.iter() {
                    total += other.a;
                }

                c_component.c = total;
            });
        });

        // read d, write e
        scope.run_r1w1(|ctx, d: &Ds, e: &mut Es| {
            ctx.iter_r1w1(d, e).components(|_, d_component, e_component| {
                let mut total = d_component.d;
                for (_, other) in d.iter() {
                    total += other.d;
                }

                e_component.e = total;
            });
        });

        // write a
        scope.run_r0w1(|ctx, a: &mut As| {
            ctx.iter_r0w1(a).components(|_, a_component| {
                a_component.a = 0.0;
            });
        });

        // read d, write e
        scope.run_r1w1(|ctx, d: &Ds, e: &mut Es| {
            ctx.iter_r1w1(d, e).components(|_, d_component, e_component| {
                let mut total = d_component.d;
                for (_, other) in d.iter() {
                    total += other.d;
                }

                e_component.e = total;
            });
        });
    });

    update
}

#[bench]
fn scheduling_sequential(b: &mut Bencher) {
    let mut world = setup();
    let mut update = setup_processing();

    b.iter(|| world.run_sequential(&mut update, SequentialExecute::ParallelBatchedCommit));
}

#[bench]
fn scheduling(b: &mut Bencher) {
    let mut world = setup();
    let mut update = setup_processing();

    b.iter(|| world.run(&mut update));
}