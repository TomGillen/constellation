# Constellation ECS

A data-oriented entity component system optimized for cache coherent resource access
and parallel system execution.

Constellation takes a "resource-first" approach to an ECS, where rather than focusing on 
components, the library is concerned about ensuring safe concurrent access to shared resources,
while providing direct access to each resource's own APIs. These resources may store per-entity
data such as positions, or may represent application wide services such as asset loaders
or input devices.

Methods for iterating through entity component data are built on top of the direct resource
access APIs.

Systems request read or write access to a set of resources, which can then be scheduled to
be potentially executed in parallel by recording them into a `SystemCommandBuffer` and
executing the command buffer within a `World`.

Constellation is heavily influenced by the [Bitsquid engine](http://bitsquid.blogspot.com) (now Autodesk Stingray), the [Molecule Engine](https://blog.molecular-matters.com) and [Specs](https://github.com/slide-rs/specs).

[Crates.io](https://crates.io/crates/constellation)  
[Documentation](https://docs.rs/constellation)

### Similar Projects
* [Specs](https://github.com/slide-rs/specs)
* [ecs-rs](https://github.com/HeroesGrave/ecs-rs)

# Examples

Defining Resources:

```rust
// Per-entity position data.
struct Position {
    x: f32,
    y: f32,
    z: f32
}

// Store position data into a vector resource
type Positions = VecResource<Position>;

// Per-entity debug names.
struct DebugName {
    name: String
}

// Store debug names in a map resource
type DebugNames = MapResource<DebugName>;

let mut world = World::new();
world.register_resource(Positions::new());
world.register_resource(DebugNames::new());
```

Update the world with Systems:

```rust
let mut update = SystemCommandBuffer::default();
update.queue_systems(|scope| {
    scope.run_r1w1(|ctx, velocities: &Velocities, positions: &mut Positions| {
        println!("Updating positions");
        // iterate through all components for entities with data in both
        // position and velocity resources
        for (_, p, v) in (velocities, positions).iter().components() {
            p.x += v.x;
            p.y += v.y;
            p.z += v.z;
        }
    });

    scope.run_r2w0(|ctx, names: &DebugNames, positions: &Positions| {
        println!("Printing positions");
        // iterate through all entity IDs for entities with data in both
        // `names` and `positions`
        let (entity_iter, n, p) = (names, positions).iter().entities(ctx);

        // `n` and `p` allow (potentially mutable) access to entity data inside
        // the resource without the ability to add or remove entities from the resource
        // - which would otherwise invalidate the iterator
        for e in entity_iter {
            println!("Entity {} is at {:?}",
                     n.get(e).unwrap().name,
                     p.get(e).unwrap());
        }
        });
    });
});

world.run(&mut update);
```

# Parallel System Execution

Systems queued into a command buffer within a single call to `queue_systems` may be executed
in parallel by the world.

The order in which systems are queued is significant in one way: the scheduler guarantees that
any changes to resources will always be observed in the same order in which systems were
queued.

For example, given two systems - `ReadPositions` and `WritePositions` - if *WritePositions* was
queued before *ReadPositions*, then it is guarenteed that *ReadPositions* will see any changes
made by *WritePositions*. Conversely, if the order were to be swapped, then *ReadPositions*
is guaranteed to *not* observe the changes made by *WritePositions*.

There is one exception to this. Entity deletions are committed when all concurrently executing
systems have completed. This behavior is deterministic, but not always obvious. If you wish to
ensure that entity deletions from one system are always seen by a later system, then queue
the two systems in separate `queue_systems` calls.
