use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::any::TypeId;
use std::cell::UnsafeCell;
use std::mem;
use std::marker::PhantomData;

use rayon::prelude::*;
use fnv::FnvHashMap;
use arrayvec::ArrayVec;

use entities::*;
use resource::*;

bitflags! {
    flags ResourceDescription: u32 {
        const ENTITY_DATA = 0b00000001
    }
}

struct ResourceCell {
    cell: UnsafeCell<Box<Resource>>,
    type_id: TypeId,
    desc: ResourceDescription
}

impl ResourceCell {
    // pub unsafe fn get(&self) -> &Resource {
    //     (&*self.cell.get()).deref()
    // }

    pub unsafe fn get_mut(&self) -> &mut Resource {
        (&mut *self.cell.get()).deref_mut()
    }

    pub unsafe fn get_as<T: Resource>(&self) -> &T {
        assert!(self.type_id == TypeId::of::<T>());
        let borrow = &*self.cell.get();
        borrow.downcast_ref_unsafe::<T>()
    }

    pub unsafe fn get_as_mut<T: Resource>(&self) -> &mut T {
        assert!(self.type_id == TypeId::of::<T>());
        let borrow = &mut *self.cell.get();
        borrow.downcast_mut_unsafe::<T>()
    }
}

/// Constructs and configures a world resource.
pub struct ResourceBuilder<T: Resource> {
    resource: T,
    desc: ResourceDescription
}

impl<T: Resource> ResourceBuilder<T> {
    /// Constructs a new `ResourceBuilder`.
    pub fn new(resource: T) -> ResourceBuilder<T> {
        ResourceBuilder {
            resource: resource,
            desc: ResourceDescription::empty()
        }
    }

    fn to_resource_cell(self) -> ResourceCell {
        ResourceCell {
            cell: UnsafeCell::new(Box::new(self.resource)),
            type_id: TypeId::of::<T>(),
            desc: self.desc
        }
    }
}

impl<T: Resource + EntityResource> ResourceBuilder<T> {
    /// Marks this resource as containing entity data which should be cleared when entities
    /// are destroyed.
    pub fn activate_entity_disposal(mut self) -> ResourceBuilder<T> {
        self.desc.insert(ENTITY_DATA);
        self
    }
}

/// Stores entities are resources, and provides mechanisms to update the world state via systems.
pub struct World {
    entities: Entities,
    resources: FnvHashMap<TypeId, (u8, ResourceCell)>,
    entity_resources: Vec<TypeId>,
    changes_buffer: Option<Vec<EntityChangeSet>>
}

// resource access is ensured safe by the system scheduler
unsafe impl Sync for World {}

impl World {
    /// Constructs a new `World`.
    pub fn new() -> World {
        World {
            entities: Entities::new(),
            resources: FnvHashMap::default(),
            entity_resources: Vec::new(),
            changes_buffer: Some(Vec::new())
        }
    }

    /// Registers a new resource with the `World`, allowing systems to access the resource.
    pub fn register<T: Resource>(&mut self, builder: ResourceBuilder<T>) -> u8 {
        let cell = builder.to_resource_cell();
        let type_id = cell.type_id;

        if cell.desc.contains(ENTITY_DATA) {
            self.entity_resources.push(type_id);
        }

        let id = self.resources.len() as u8;
        let value = (id, cell);
        self.resources.insert(type_id, value);

        id
    }

    /// Registers a new resource with the `World`, allowing systems to access the resource.
    pub fn register_resource<T: Resource>(&mut self, resource: T) -> u8 {
        self.register(resource.to_builder())
    }

    // Can only be safely called from within a system, and only for the resources the system declares
    unsafe fn get_resource<T: Resource>(&self) -> Option<(u8, &T)> {
        let type_id = TypeId::of::<T>();
        if let Some(&(id, ref resource)) = self.resources.get(&type_id) {
            let resource = resource.get_as();
            return Some((id, resource));
        }
        return None;
    }

    // Can only be safely called from within a system, and only for the resources the system declares
    unsafe fn get_resource_mut<T: Resource>(&self) -> Option<(u8, &mut T)> {
        let type_id = TypeId::of::<T>();
        if let Some(&(id, ref resource)) = self.resources.get(&type_id) {
            let resource = resource.get_as_mut();
            return Some((id, resource));
        }
        return None;
    }

    // Can only be safely called in between systems, on the main thread
    unsafe fn clean_deleted_entities(&mut self, changes: &[EntityChangeSet]) {
        for id in self.entity_resources.iter() {
            let &mut (_, ref mut resource) = self.resources.get_mut(&id).unwrap();
            let resource = resource.get_mut();
            for cs in changes.iter() {
                resource.clear_entity_data(&cs.deleted);
            }
        }
    }
}

trait System<S: Send + Sync + 'static> : Send {
    fn id(&self) -> u32;
    fn resource_access(&self) -> (&HashSet<TypeId>, &HashSet<TypeId>);
    fn execute(&mut self, &World, &S) -> EntityChangeSet;
}

/// Contains system execution state information.
pub struct SystemContext<'a, S: Send + Sync + 'static> {
    entities: EntitiesTransaction<'a>,
    state: &'a S
}

impl<'a, S: Send + Sync + 'static> Deref for SystemContext<'a, S> {
    type Target = EntitiesTransaction<'a>;

    fn deref(&self) -> &EntitiesTransaction<'a> {
        &self.entities
    }
}

impl<'a, S: Send + Sync + 'static> DerefMut for SystemContext<'a, S> {
    fn deref_mut(&mut self) -> &mut EntitiesTransaction<'a> {
        &mut self.entities
    }
}

impl<'a, S: Send + Sync + 'static> SystemContext<'a, S> {
    /// Gets the current system execution state.
    pub fn state(&self) -> &S {
        self.state
    }

    fn to_change_set(self) -> EntityChangeSet {
        self.entities.to_change_set()
    }
}

struct FnSystem<S, F> where F: FnMut(&World, &S) -> EntityChangeSet + Send, S: Send + Sync + 'static {
    id: u32,
    read: HashSet<TypeId>,
    write: HashSet<TypeId>,
    f: F,
    phantom: PhantomData<&'static S>
}

impl<S: Send + Sync + 'static, T: FnMut(&World, &S) -> EntityChangeSet + Send> System<S> for FnSystem<S, T> {
    #[inline]
    fn id(&self) -> u32 {
        self.id
    }

    #[inline]
    fn resource_access(&self) -> (&HashSet<TypeId>, &HashSet<TypeId>) {
        (&self.read, &self.write)
    }

    #[inline]
    fn execute(&mut self, world: &World, state: &S) -> EntityChangeSet {
        (self.f)(world, state)
    }
}

/// Records system executions, to be run later within a `World`.
pub struct SystemCommandBuffer<S: Send + Sync + 'static = ()> {
    batches: Vec<Vec<Box<System<S>>>>
}

/// Systems queued within a `SystemScope` may be scheduled to run in parallel.
pub struct SystemScope<S: Send + Sync + 'static> {
    systems: Vec<Box<System<S>>>
}

impl SystemCommandBuffer<()> {
    /// Constructs a new 'SystemCommandBuffer' with an empty state.
    pub fn default() -> SystemCommandBuffer<()> {
        SystemCommandBuffer::new()
    }
}

impl<S: Send + Sync + 'static> SystemCommandBuffer<S> {
    /// Constructs a new `SystemCommandBuffer`.
    pub fn new() -> SystemCommandBuffer<S> {
        SystemCommandBuffer {
            batches: Vec::new()
        }
    }

    /// Queues a sequence of systems into the command buffer. Each system may be run concurrently.
    pub fn queue_systems<'a, F, R>(&mut self, f: F) -> R
        where F: FnOnce(&mut SystemScope<S>) -> R + 'a
    {
        let mut scope = SystemScope::new();
        let result = f(&mut scope);

        let mut reading = HashSet::<TypeId>::new();
        let mut writing = HashSet::<TypeId>::new();
        let mut batch = Vec::<Box<System<S>>>::new();
        for system in scope.systems.into_iter() {
            {
                let (system_reading, system_writing) = system.resource_access();
                if !can_batch_system(&reading, &writing, (system_reading, system_writing)) {
                    self.batches.push(batch);
                    batch = Vec::<Box<System<S>>>::new();
                    reading.clear();
                    writing.clear();
                }

                reading = &reading | system_reading;
                writing = &writing | system_writing;
            }
            batch.push(system);
        }

        if batch.len() > 0 {
            self.batches.push(batch);
        }

        result
    }
}

fn can_batch_system(reading: &HashSet<TypeId>, writing: &HashSet<TypeId>, (system_reads, system_writes): (&HashSet<TypeId>, &HashSet<TypeId>)) -> bool {
    // the system does not write to any resources being read (which includes writers)
    // the system does not read any resources that are being written
    reading.is_disjoint(system_writes) && writing.is_disjoint(system_reads)
}

impl<S: Send + Sync + 'static> SystemScope<S> {
    fn new() -> SystemScope<S> {
        SystemScope {
            systems: Vec::new()
        }
    }
}

macro_rules! impl_run_system {
    ($name:ident [$($read:ident),*] [$($write:ident),*]) => (
        impl<S: Send + Sync + 'static> SystemScope<S> {
            /// Queues a new system into the command buffer.
            ///
            /// Each system queued within a single `SystemScope` may be executed in parallel
            /// with each other. See crate documentation for more information.
            #[allow(non_snake_case, unused_variables, unused_mut)]
            pub fn $name<$($read,)* $($write,)* F>(&mut self, mut f: F) -> u32
                where $($read:Resource,)*
                      $($write:Resource,)*
                      F: for<'a, 'b> FnMut(&'a mut SystemContext<'b, S>, $(&'b $read,)* $(&'b mut $write,)*) + Send + 'static
            {
                let system = move |world: &World, state: &S| {
                    // safety of these gets is ensured by the system scheduler
                    $(let (_, $read) = unsafe { world.get_resource::<$read>().expect("World does not contain required resource") };)*
                    $(let (_, mut $write) = unsafe { world.get_resource_mut::<$write>().expect("World does not contain required resource") };)*

                    let mut context = SystemContext {
                        entities: world.entities.transaction(),
                        state: state
                    };

                    f(&mut context, $($read.deref(),)* $($write.deref_mut(),)*);
                    context.to_change_set()
                };

                let mut read = HashSet::<TypeId>::new();
                $(read.insert(TypeId::of::<$read>());)*
                $(read.insert(TypeId::of::<$write>());)*

                let mut write = HashSet::<TypeId>::new();
                $(write.insert(TypeId::of::<$write>());)*

                let id = self.systems.len() as u32;
                let boxed = Box::new(FnSystem {
                    id: id,
                    read: read,
                    write: write,
                    f: system,
                    phantom: PhantomData
                });

                self.systems.push(boxed);
                id
            }
        }
    )
}

impl_run_system!(run_r0w0 [] []);
impl_run_system!(run_r1w0 [R0] []);
impl_run_system!(run_r2w0 [R0, R1] []);
impl_run_system!(run_r3w0 [R0, R1, R2] []);
impl_run_system!(run_r4w0 [R0, R1, R2, R3] []);
impl_run_system!(run_r5w0 [R0, R1, R2, R3, R4] []);
impl_run_system!(run_r6w0 [R0, R1, R2, R3, R4, R5] []);
impl_run_system!(run_r0w1 [] [W0]);
impl_run_system!(run_r1w1 [R0] [W0]);
impl_run_system!(run_r2w1 [R0, R1] [W0]);
impl_run_system!(run_r3w1 [R0, R1, R2] [W0]);
impl_run_system!(run_r4w1 [R0, R1, R2, R3] [W0]);
impl_run_system!(run_r5w1 [R0, R1, R2, R3, R4] [W0]);
impl_run_system!(run_r6w1 [R0, R1, R2, R3, R4, R5] [W0]);
impl_run_system!(run_r0w2 [] [W0, W1]);
impl_run_system!(run_r1w2 [R0] [W0, W1]);
impl_run_system!(run_r2w2 [R0, R1] [W0, W1]);
impl_run_system!(run_r3w2 [R0, R1, R2] [W0, W1]);
impl_run_system!(run_r4w2 [R0, R1, R2, R3] [W0, W1]);
impl_run_system!(run_r5w2 [R0, R1, R2, R3, R4] [W0, W1]);
impl_run_system!(run_r6w2 [R0, R1, R2, R3, R4, R5] [W0, W1]);
impl_run_system!(run_r0w3 [] [W0, W1, W3]);
impl_run_system!(run_r1w3 [R0] [W0, W1, W2]);
impl_run_system!(run_r2w3 [R0, R1] [W0, W1, W2]);
impl_run_system!(run_r3w3 [R0, R1, R2] [W0, W1, W2]);
impl_run_system!(run_r4w3 [R0, R1, R2, R3] [W0, W1, W2]);
impl_run_system!(run_r5w3 [R0, R1, R2, R3, R4] [W0, W1, W2]);
impl_run_system!(run_r6w3 [R0, R1, R2, R3, R4, R5] [W0, W1, W2]);

/// Determines the behavior entity transaction commits when running a
/// system command buffer sequentially.
pub enum SequentialExecute {
    /// Entity creations and delections are always comitted after each system,
    /// guarenteeing that delections will always be observed by later queued systems.
    /// This does not always result in the same behavior as parallel command buffer execution.
    SequentialCommit,
    /// Entity creations and delections are always comitted in the same batches that would
    /// otherwise had been scheduled for parallel execution had the command buffer been executed
    /// in parallel. This emulates the same behavior as parallel command buffer execution.
    ParallelBatchedCommit
}

impl World {
    /// Executes a `SystemCommandBuffer`, potentially scheduling systems for parallel execution, with the given state.
    pub fn run_with_state<S: Send + Sync + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, state: &S) {
        self.execute_batched(systems, |world, batch, changes| {
            batch.par_iter_mut()
                .weight_max()
                .map(|system| system.execute(world, state))
                .collect_into(changes);
        });
    }

    /// Executes a `SystemCommandBuffer`, potentially scheduling systems for parallel execution, with a default state value.
    pub fn run<S: Send + Sync + Default + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>) {
        let state: S = Default::default();
        self.run_with_state(systems, &state);
    }

    /// Executes a `SystemCommandBuffer` sequentially, with the given state.
    pub fn run_with_state_sequential<S: Send + Sync + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, mode: SequentialExecute, state: &S) {
        match mode {
            SequentialExecute::SequentialCommit => self.run_sequential_sc(systems, state),
            SequentialExecute::ParallelBatchedCommit => self.run_sequential_pc(systems, state)
        };
    }

    /// Executes a `SystemCommandBuffer` sequentially, with a default state value.
    pub fn run_sequential<S: Send + Sync + Default + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, mode: SequentialExecute) {
        let state: S = Default::default();
        self.run_with_state_sequential(systems, mode, &state);
    }

    fn run_sequential_pc<S: Send + Sync + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, state: &S) {
        self.execute_batched(systems, |world, batch, changes| {
            for system in batch.iter_mut() {
                changes.push(system.execute(world, state));
            }
        });
    }

    fn run_sequential_sc<S: Send + Sync + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, state: &S) {
        for batch in systems.batches.iter_mut() {
            for system in batch.iter_mut() {
                let changes = [system.execute(self, state)];
                unsafe { self.clean_deleted_entities(&changes) };
                self.entities.merge(ArrayVec::from(changes).into_iter());
            }
        }
    }

    fn execute_batched<F, S: Send + Sync + 'static>(&mut self, systems: &mut SystemCommandBuffer<S>, mut f: F)
        where F: FnMut(&mut World, &mut Vec<Box<System<S>>>, &mut Vec<EntityChangeSet>)
    {
        let mut changes = mem::replace(&mut self.changes_buffer, None).unwrap();
        for batch in systems.batches.iter_mut() {
            let additional = batch.len() - changes.len();
            if additional > 0 {
                changes.reserve(additional);
            }

            f(self, batch, &mut changes);

            unsafe { self.clean_deleted_entities(&changes) };
            self.entities.merge(changes.drain(..));
        }

        self.changes_buffer = Some(changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::*;
    use resource::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestResource {
        pub x: u8
    }

    struct TestResource2 { }
    struct TestResource3 { }

    impl Resource for TestResource { }
    impl Resource for TestResource2 { }
    impl Resource for TestResource3 { }

    #[test]
    fn create_world() {
        World::new();
    }

    #[test]
    fn register_resource() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });
    }

    #[test]
    fn register_resources_unique_id() {
        let mut world = World::new();
        let mut ids: HashSet<u8> = HashSet::new();

        let id = world.register_resource(TestResource { x: 1 });
        assert!(!ids.contains(&id));
        ids.insert(id);

        let id = world.register_resource(TestResource2 {});
        assert!(!ids.contains(&id));
        ids.insert(id);

        let id = world.register_resource(TestResource3 {});
        assert!(!ids.contains(&id));
        ids.insert(id);
    }

    #[test]
    fn get_resource() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });

        let (_, test) = unsafe { world.get_resource::<TestResource>().unwrap() };
        assert_eq!(test.x, 1);
    }

    #[test]
    fn get_resource_mut() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });

        let (_, mut test) = unsafe { world.get_resource_mut::<TestResource>().unwrap() };
        assert_eq!(test.x, 1);
        test.x = 2;
    }

    #[test]
    fn run_system() {
        let mut world = World::new();

        let run_count = Arc::new(AtomicUsize::new(0));

        let run_count_clone = run_count.clone();

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r0w0(move |_| { run_count_clone.fetch_add(1, Ordering::Relaxed); });
        });

        world.run(&mut buffer);

        assert_eq!(run_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn run_multiple_system() {
        let mut world = World::new();

        let run_count = Arc::new(AtomicUsize::new(0));

        let run_count_clone1 = run_count.clone();
        let run_count_clone2 = run_count.clone();
        let run_count_clone3 = run_count.clone();

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r0w0(move |_| { run_count_clone1.fetch_add(1, Ordering::Relaxed); });
            scope.run_r0w0(move |_| { run_count_clone2.fetch_add(1, Ordering::Relaxed); });
            scope.run_r0w0(move |_| { run_count_clone3.fetch_add(1, Ordering::Relaxed); });
        });

        world.run(&mut buffer);

        assert_eq!(run_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn run_system_exclusive_write_sequential() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });
        world.register_resource(TestResource2 {});

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r1w0(move |_, r: &TestResource| {
                assert_eq!(r.x, 1);
            });
            scope.run_r1w0(move |_, r: &TestResource| {
                assert_eq!(r.x, 1);
            });
            scope.run_r1w1(|_, _: &TestResource2, w: &mut TestResource| {
                assert_eq!(w.x, 1);
                w.x = 2;
            });
            scope.run_r1w0(move |_, r: &TestResource| {
                assert_eq!(r.x, 2);
            });
        });

        world.run_sequential(&mut buffer, SequentialExecute::SequentialCommit);
    }

    #[test]
    fn run_system_exclusive_write_parallel() {
        for _ in 0..1000 {
            let mut world = World::new();
            world.register_resource(TestResource { x: 1 });
            world.register_resource(TestResource2 {});

            let mut buffer = SystemCommandBuffer::default();
            let (a, b, c, d) = buffer.queue_systems(|scope| {
                let a = scope.run_r1w0(move |_, r: &TestResource| {
                    assert_eq!(r.x, 1);
                });
                let b = scope.run_r1w0(move |_, r: &TestResource| {
                    assert_eq!(r.x, 1);
                });
                let c = scope.run_r1w1(|_, _: &TestResource2, w: &mut TestResource| {
                    assert_eq!(w.x, 1);
                    w.x = 2;
                });
                let d = scope.run_r1w0(move |_, r: &TestResource| {
                    assert_eq!(r.x, 2);
                });

                (a, b, c, d)
            });

            assert_eq!(buffer.batches.len(), 3);
            assert_eq!(buffer.batches[0].len(), 2);
            assert_eq!(buffer.batches[0][0].id(), a);
            assert_eq!(buffer.batches[0][1].id(), b);
            assert_eq!(buffer.batches[1].len(), 1);
            assert_eq!(buffer.batches[1][0].id(), c);
            assert_eq!(buffer.batches[2].len(), 1);
            assert_eq!(buffer.batches[2][0].id(), d);

            world.run(&mut buffer);
        }
    }

    #[test]
    fn system_batch_all_read_distinct() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r1w0(|_, _: &TestResource2| {});
            scope.run_r1w0(|_, _: &TestResource3| {});
        });

        assert_eq!(buffer.batches.len(), 1);
        assert_eq!(buffer.batches[0].len(), 3);
    }

    #[test]
    fn system_batch_all_read() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r1w0(|_, _: &TestResource2| {});
        });

        assert_eq!(buffer.batches.len(), 1);
        assert_eq!(buffer.batches[0].len(), 3);
    }

    #[test]
    fn system_batch_all_write_distinct() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r0w1(|_, _: &mut TestResource| {});
            scope.run_r0w1(|_, _: &mut TestResource2| {});
            scope.run_r0w1(|_, _: &mut TestResource3| {});
        });

        assert_eq!(buffer.batches.len(), 1);
        assert_eq!(buffer.batches[0].len(), 3);
        assert_eq!(buffer.batches[0][0].id(), 0);
        assert_eq!(buffer.batches[0][1].id(), 1);
        assert_eq!(buffer.batches[0][2].id(), 2);
    }

    #[test]
    fn system_batch_all_write() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r0w1(|_, _: &mut TestResource| {});
            scope.run_r0w1(|_, _: &mut TestResource2| {});
            scope.run_r0w2(|_, _: &mut TestResource, _: &mut TestResource2| {});
        });

        assert_eq!(buffer.batches.len(), 2);
        assert_eq!(buffer.batches[0].len(), 2);
        assert_eq!(buffer.batches[0][0].id(), 0);
        assert_eq!(buffer.batches[0][1].id(), 1);
        assert_eq!(buffer.batches[1].len(), 1);
        assert_eq!(buffer.batches[1][0].id(), 2);
    }

    #[test]
    fn system_batch_all_write_interleaved() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r0w1(|_, _: &mut TestResource| {});
            scope.run_r0w2(|_, _: &mut TestResource, _: &mut TestResource2| {});
            scope.run_r0w1(|_, _: &mut TestResource2| {});
        });

        assert_eq!(buffer.batches.len(), 3);
        assert_eq!(buffer.batches[0].len(), 1);
        assert_eq!(buffer.batches[0][0].id(), 0);
        assert_eq!(buffer.batches[1].len(), 1);
        assert_eq!(buffer.batches[1][0].id(), 1);
        assert_eq!(buffer.batches[2].len(), 1);
        assert_eq!(buffer.batches[2][0].id(), 2);
    }

    #[test]
    fn system_batch_mixed() {
        let mut buffer = SystemCommandBuffer::default();

        buffer.queue_systems(|scope| {
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r1w1(|_, _: &TestResource, _: &mut TestResource2| {});
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r2w0(|_, _: &TestResource, _: &TestResource2| {});
            scope.run_r1w0(|_, _: &TestResource| {});
            scope.run_r0w1(|_, _: &mut TestResource| {});
        });

        assert_eq!(buffer.batches.len(), 3);
        assert_eq!(buffer.batches[0].len(), 3);
        assert_eq!(buffer.batches[0][0].id(), 0);
        assert_eq!(buffer.batches[0][1].id(), 1);
        assert_eq!(buffer.batches[0][2].id(), 2);
        assert_eq!(buffer.batches[1].len(), 2);
        assert_eq!(buffer.batches[1][0].id(), 3);
        assert_eq!(buffer.batches[1][1].id(), 4);
        assert_eq!(buffer.batches[2].len(), 1);
        assert_eq!(buffer.batches[2][0].id(), 5);
    }

    #[test]
    fn system_command_buffer() {
        let mut systems = SystemCommandBuffer::default();
        systems.queue_systems(|scope| {
            scope.run_r1w1(|_, _: &TestResource, _: &mut TestResource2| {

            });

            scope.run_r1w1(|_, _: &TestResource, _: &mut TestResource2| {

            });
        });

        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });
        world.register_resource(TestResource2 {});
        world.run(&mut systems);
    }

    #[test]
    fn pass_state() {
        let mut systems = SystemCommandBuffer::<u32>::new();
        systems.queue_systems(|scope| {
            scope.run_r0w0(|ctx| {
                assert_eq!(ctx.state(), &5u32);
            });
        });

        let mut world = World::new();
        world.run_with_state(&mut systems, &5u32);
    }

    #[test]
    fn clean_deleted_entities() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r0w1(|tx, resource: &mut VecResource<u32>| {
                println!("Creating entities");
                for i in 0..1000 {
                    let e = tx.create();
                    resource.add(e, i);
                }
                println!("Entities created");
            });

            scope.run_r1w0(|entities, resource: &VecResource<u32>| {
                println!("Verifying entity creation");
                iter_entities_r1w0(resource, |iter, r| {
                    for i in iter {
                        assert_eq!(i, *r.get(entities.by_index(i)).unwrap());
                    }
                });
                println!("Verified entity creation");
            });

            scope.run_r0w1(|tx, resource: &mut VecResource<u32>| {
                println!("Deleting entities");
                let mut entities = Vec::<Index>::new();
                iter_entities_r1w0(resource, |iter, _| {
                    for i in iter {
                        entities.push(i);
                    }
                });

                for e in entities {
                    let entity = tx.by_index(e);
                    tx.destroy(entity);
                }
                println!("Deleted entities");
            });

            scope.run_r1w0(|_, resource: &VecResource<u32>| {
                println!("Verifying entity deletion");
                let mut entities = Vec::<Index>::new();
                iter_entities_r1w0(resource, |iter, _| {
                    for e in iter {
                        entities.push(e);
                    }
                });

                assert_eq!(entities.len(), 0);
                println!("Verified entity delection");
            });
        });

        world.run(&mut buffer);
    }
}
