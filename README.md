# Constellation ECS

A data-oriented entity component system optimized for cache coherent resource access
and parallel system execution.

Constellation does not have any native understanding of a "component". Instead, the library
is concerned about ensuring safe concurrent access to shared resources, while providing direct
access to each resource's own APIs. These resources may store per-entity data such as
positions, or may represent application wide services such as asset loaders or input devices.

Systems request read or write access to a set of resources, which can then be scheduled to
be potentially executed in parallel by recording them into a `SystemCommandBuffer` and
executing the command buffer within a `World`.

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
world.register_entity_resource(Positions::new());
world.register_entity_resource(DebugNames::new());
```

Update the world with Systems:

```rust
let mut update = SystemCommandBuffer::new();
update.queue_systems(|scope| {
    scope.run_r1w1(|entities, velocities: &Velocities, positions: &mut Positions| {
        println!("Updating positions");

        // iterate through all entities with data in both position and velocity resources
        iter_entities_r1w1(velocities, positions, |entity_iter, v, p| {
            // `v` and `p` allow (mutable in the case of `p`) access to entity data inside
            // the resource without the ability to add or remove entities from the resource
            // - which would otherwise invalidate the iterator
            for i in entity_iter {
                // unchecked getters offer direct indexing
                let position = unsafe { p.get_unchecked_mut(i) };
                let velocity = unsafe { v.get_unchecked(i) };
                position.x += velocity.x;
                position.y += velocity.y;
                position.z += velocity.z;
            }
        });
    });

    scope.run_r2w0(|entities, names: &DebugNames, positions: &Positions| {
        println!("Printing positions");

        iter_entities_r2w0(names, positions, |entity_iter, n, p| {
            for i in entity_iter {
                let entity = entities.by_index(i);
                println!("Entity {} is at {:?}",
                         n.get(entity).unwrap().name,
                         p.get(entity).unwrap());
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
