use std::ops::{Deref, DerefMut};
use std::any::TypeId;
use std::sync::RwLock;
use std::mem;

use rayon::prelude::*;
use fnv::FnvHashMap;
use arrayvec::ArrayVec;

use entities::*;
use resource::*;

pub struct World {
    entities: Entities,
    resources: FnvHashMap<TypeId, (u8, RwLock<BoxedResource>)>,
    entity_resources: Vec<TypeId>,
    changes_buffer: Option<Vec<EntityChangeSet>>
}

impl World {
    pub fn new() -> World {
        World {
            entities: Entities::new(),
            resources: FnvHashMap::default(),
            entity_resources: Vec::new(),
            changes_buffer: Some(Vec::new())
        }
    }

    pub fn register_resource<T: Resource>(&mut self, resource: T) -> u8 {
        let type_id = TypeId::of::<T>();
        let boxed = BoxedResource::Resource(Box::new(resource) as Box<Resource>);
        let id = self.resources.len() as u8;
        let value = (id, RwLock::new(boxed));
        self.resources.insert(type_id, value);
        id
    }

    pub fn register_entity_resource<T: EntityResource>(&mut self, resource: T) -> u8 {
        let type_id = TypeId::of::<T>();
        let boxed = BoxedResource::EntityResource(Box::new(resource) as Box<StoresEntityData>);
        let id = self.resources.len() as u8;
        let value = (id, RwLock::new(boxed));
        self.resources.insert(type_id, value);
        self.entity_resources.push(type_id);
        id
    }

    fn get_resource<T: Resource>(&self) -> Option<(u8, ResourceReadGuard<T>)> {
        let type_id = TypeId::of::<T>();
        if let Some(&(id, ref resource)) = self.resources.get(&type_id) {
            let guard = resource.read().unwrap();
            let resource = ResourceReadGuard::<T>::new(guard);
            return Some((id, resource));
        }
        return None;
    }

    fn get_resource_mut<T: Resource>(&self) -> Option<(u8, ResourceWriteGuard<T>)> {
        let type_id = TypeId::of::<T>();
        if let Some(&(id, ref resource)) = self.resources.get(&type_id) {
            let guard = resource.write().unwrap();
            let resource = ResourceWriteGuard::<T>::new(guard);
            return Some((id, resource));
        }
        return None;
    }

    fn get_resource_id<T: Resource>(&self) -> Option<u8> {
        let type_id = TypeId::of::<T>();
        if let Some(&(id, _)) = self.resources.get(&type_id) {
            return Some(id);
        }
        return None;
    }

    pub fn update(&mut self, systems: &mut [Box<System>]) {
        let mut changes = mem::replace(&mut self.changes_buffer, None).unwrap();
        for batch in SystemBatchIter::new(systems) {
            let additional = batch.len() - changes.len();
            if additional > 0 {
                changes.reserve(additional);
            }

            batch.par_iter_mut()
                .weight_max()
                .map(|system| system.execute(self))
                .collect_into(&mut changes);

            self.clean_deleted_entities(&changes);
            self.entities.merge(changes.drain(..));
        }

        self.changes_buffer = Some(changes);
    }

    pub fn update_sequential(&mut self, systems: &mut [Box<System>]) {
        for mut system in systems.iter_mut() {
            let changes = [system.execute(self)];
            self.clean_deleted_entities(&changes);
            self.entities.merge(ArrayVec::from(changes).into_iter());
        }
    }

    fn clean_deleted_entities(&mut self, changes: &[EntityChangeSet]) {
        for id in self.entity_resources.iter() {
            let &mut (_, ref mut lock) = self.resources.get_mut(&id).unwrap();
            let mut guard = lock.write().unwrap();
            let resource = guard.deref_mut();
            if let &mut BoxedResource::EntityResource(ref mut boxed) = resource {
                for cs in changes.iter() {
                    boxed.clear(&cs.deleted);
                }
            }
        }
    }
}

pub trait System : Send {
    fn resource_key(&self) -> (u64, u64);
    fn execute(&mut self, &World) -> EntityChangeSet;
}

pub struct FnSystem<T: FnMut(&World) -> EntityChangeSet + Send> {
    read: u64,
    write: u64,
    f: T
}

impl<T: FnMut(&World) -> EntityChangeSet + Send> System for FnSystem<T> {
    #[inline]
    fn resource_key(&self) -> (u64, u64) {
        (self.read, self.write)
    }

    #[inline]
    fn execute(&mut self, world: &World) -> EntityChangeSet {
        (self.f)(&world)
    }
}

impl World {
    pub fn system<F>(&self, mut f: F) -> Box<System>
        where F: FnMut(&mut EntitiesTransaction) + Send + 'static
    {
        let system = move |world: &World| {
            let mut tx = world.entities.transaction();
            f(&mut tx);
            tx.to_change_set()
        };

        Box::new(FnSystem {
            read: 0,
            write: 0,
            f: system
        })
    }
}

macro_rules! impl_system {
    ($name:ident [$($read:ident),*] [$($write:ident),*]) => (
        impl World {
            #[allow(non_snake_case, unused_variables)]
            pub fn $name<$($read,)* $($write,)* F>(&self, mut f: F) -> Box<System>
                where $($read:Resource,)*
                      $($write:Resource,)*
                      F: for<'a, 'b> FnMut(&'a mut EntitiesTransaction<'b>, $(&'b $read,)* $(&'b mut $write,)*) + Send + 'static
            {
                let system = move |world: &World| {
                    $(let (_, $read) = world.get_resource::<$read>().unwrap();)*
                    $(let (_, mut $write) = world.get_resource_mut::<$write>().unwrap();)*

                    let mut tx = world.entities.transaction();
                    f(&mut tx, $($read.deref(),)* $($write.deref_mut(),)*);
                    tx.to_change_set()
                };

                $(let $read = 1u64 << self.get_resource_id::<$read>().unwrap();)*
                $(let $write = 1u64 << self.get_resource_id::<$write>().unwrap();)*

                Box::new(FnSystem {
                    read: 0 $(| $write)* $(| $read)*,
                    write: 0 $(| $write)*,
                    f: system
                })
            }
        }
    )
}

impl_system!(system_r0w0 [] []);
impl_system!(system_r1w0 [R0] []);
impl_system!(system_r2w0 [R0, R1] []);
impl_system!(system_r3w0 [R0, R1, R2] []);
impl_system!(system_r4w0 [R0, R1, R2, R3] []);
impl_system!(system_r5w0 [R0, R1, R2, R3, R4] []);
impl_system!(system_r6w0 [R0, R1, R2, R3, R4, R5] []);
impl_system!(system_r0w1 [] [W0]);
impl_system!(system_r1w1 [R0] [W0]);
impl_system!(system_r2w1 [R0, R1] [W0]);
impl_system!(system_r3w1 [R0, R1, R2] [W0]);
impl_system!(system_r4w1 [R0, R1, R2, R3] [W0]);
impl_system!(system_r5w1 [R0, R1, R2, R3, R4] [W0]);
impl_system!(system_r6w1 [R0, R1, R2, R3, R4, R5] [W0]);
impl_system!(system_r0w2 [] [W0, W1]);
impl_system!(system_r1w2 [R0] [W0, W1]);
impl_system!(system_r2w2 [R0, R1] [W0, W1]);
impl_system!(system_r3w2 [R0, R1, R2] [W0, W1]);
impl_system!(system_r4w2 [R0, R1, R2, R3] [W0, W1]);
impl_system!(system_r5w2 [R0, R1, R2, R3, R4] [W0, W1]);
impl_system!(system_r6w2 [R0, R1, R2, R3, R4, R5] [W0, W1]);
impl_system!(system_r0w3 [] [W0, W1, W3]);
impl_system!(system_r1w3 [R0] [W0, W1, W2]);
impl_system!(system_r2w3 [R0, R1] [W0, W1, W2]);
impl_system!(system_r3w3 [R0, R1, R2] [W0, W1, W2]);
impl_system!(system_r4w3 [R0, R1, R2, R3] [W0, W1, W2]);
impl_system!(system_r5w3 [R0, R1, R2, R3, R4] [W0, W1, W2]);
impl_system!(system_r6w3 [R0, R1, R2, R3, R4, R5] [W0, W1, W2]);

struct SystemBatchIter<'a> {
    v: &'a mut [Box<System>]
}

impl<'a> SystemBatchIter<'a> {
    fn new(systems: &'a mut [Box<System>]) -> SystemBatchIter<'a> {
        SystemBatchIter {
            v: systems
        }
    }
}

fn can_batch_system(reading: u64, writing: u64, (system_reads, system_writes): (u64, u64)) -> bool {
    // the system does not write to any resources being read (which includes writers)
    // the system does not read any resources that are being written
    (reading & system_writes == 0) && (writing & system_reads == 0)
}

impl<'a> Iterator for SystemBatchIter<'a> {
    type Item = &'a mut [Box<System>];

    #[inline]
    fn next(&mut self) -> Option<&'a mut [Box<System>]> {
        if self.v.is_empty() {
            None
        } else {
            let mut shared = 0;
            let mut exclusive = 0;

            for i in 0..self.v.len() {
                let (read, write) = self.v[i].deref().resource_key();
                if !can_batch_system(shared, exclusive, (read, write)) {
                    let tmp = mem::replace(&mut self.v, &mut []);
                    let (batch, remainder) = tmp.split_at_mut(i);
                    self.v = remainder;
                    return Some(batch);
                }

                shared = shared | read;
                exclusive = exclusive | write;
            }

            let tmp = mem::replace(&mut self.v, &mut []);
            Some(tmp)
        }
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
    fn get_resouce_id() {
        let mut world = World::new();

        let asigned = world.register_resource(TestResource { x: 1 });
        let retrieved = world.get_resource_id::<TestResource>().unwrap();

        assert_eq!(asigned, retrieved);
    }

    #[test]
    fn get_resource() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });

        let (_, test) = world.get_resource::<TestResource>().unwrap();
        assert_eq!(test.x, 1);
    }

    #[test]
    fn get_resource_mut() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });

        let (_, mut test) = world.get_resource_mut::<TestResource>().unwrap();
        assert_eq!(test.x, 1);
        test.x = 2;
    }

    #[test]
    fn create_fn_system() {
        let world = World::new();
        world.system(|_| {});
    }

    #[test]
    fn run_system() {
        let mut world = World::new();

        let run_count = Arc::new(AtomicUsize::new(0));

        let run_count_clone = run_count.clone();
        let system = world.system(move |_| { run_count_clone.fetch_add(1, Ordering::Relaxed); });

        world.update(&mut vec![system]);

        assert_eq!(run_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn run_multiple_system() {
        let mut world = World::new();

        let run_count = Arc::new(AtomicUsize::new(0));

        let run_count_clone = run_count.clone();
        let system1 = world.system(move |_| { run_count_clone.fetch_add(1, Ordering::Relaxed); });

        let run_count_clone = run_count.clone();
        let system2 = world.system(move |_| { run_count_clone.fetch_add(1, Ordering::Relaxed); });

        let run_count_clone = run_count.clone();
        let system3 = world.system(move |_| { run_count_clone.fetch_add(1, Ordering::Relaxed); });

        world.update(&mut vec![system1, system2, system3]);

        assert_eq!(run_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn run_system_exclusive_write_sequential() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });
        world.register_resource(TestResource2 {});

        let reader = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 1);
        });

        let reader2 = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 1);
        });

        let writer = world.system_r1w1(|_, _: &TestResource2, w: &mut TestResource| {
            assert_eq!(w.x, 1);
            w.x = 2;
        });

        let reader3 = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 2);
        });

        world.update_sequential(&mut vec![reader, reader2, writer, reader3]);
    }

    #[test]
    fn run_system_exclusive_write_parallel() {
        let mut world = World::new();
        world.register_resource(TestResource { x: 1 });
        world.register_resource(TestResource2 {});

        let reader = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 1);
        });

        let reader2 = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 1);
        });

        let writer = world.system_r1w1(|_, _: &TestResource2, w: &mut TestResource| {
            assert_eq!(w.x, 1);
            w.x = 2;
        });

        let reader3 = world.system_r1w0(move |_, r: &TestResource| {
            assert_eq!(r.x, 2);
        });

        world.update(&mut vec![reader, reader2, writer, reader3]);
    }

    #[derive(PartialEq, Eq, Debug)]
    struct TestSystemIter {
        pub read: u64,
        pub write: u64
    }

    impl System for TestSystemIter {
        fn resource_key(&self) -> (u64, u64) {
            (self.read, self.write)
        }

        fn execute(&mut self, _: &World) -> EntityChangeSet {
            EntityChangeSet {
                deleted: Vec::new()
            }
        }
    }

    #[test]
    fn system_batch_all_read_distinct() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 0
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 2u64,
            write: 0
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 4u64,
            write: 0
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
    }

    #[test]
    fn system_batch_all_read() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 0
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 1u64,
            write: 0
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 3u64,
            write: 0
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
    }

    #[test]
    fn system_batch_all_write_distinct() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 1u64
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 2u64,
            write: 2u64
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 4u64,
            write: 4u64
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[0][0].resource_key(), (1u64, 1u64));
        assert_eq!(batches[0][1].resource_key(), (2u64, 2u64));
        assert_eq!(batches[0][2].resource_key(), (4u64, 4u64));
    }

    #[test]
    fn system_batch_all_write() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 1u64
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 2u64,
            write: 2u64
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 3u64,
            write: 3u64
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[0][0].resource_key(), (1u64, 1u64));
        assert_eq!(batches[0][1].resource_key(), (2u64, 2u64));
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[1][0].resource_key(), (3u64, 3u64));
    }

    #[test]
    fn system_batch_all_write_interleaved() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 1u64
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 3u64,
            write: 3u64
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 2u64,
            write: 2u64
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].resource_key(), (1u64, 1u64));
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[1][0].resource_key(), (3u64, 3u64));
        assert_eq!(batches[2].len(), 1);
        assert_eq!(batches[2][0].resource_key(), (2u64, 2u64));
    }

    #[test]
    fn system_batch_mixed() {
        let a = Box::new(TestSystemIter {
            read: 1u64,
            write: 0u64
        }) as Box<System + 'static>;

        let b = Box::new(TestSystemIter {
            read: 3u64,
            write: 2u64
        }) as Box<System + 'static>;

        let c = Box::new(TestSystemIter {
            read: 1u64,
            write: 0u64
        }) as Box<System + 'static>;

        let d = Box::new(TestSystemIter {
            read: 3u64,
            write: 0u64
        }) as Box<System + 'static>;

        let e = Box::new(TestSystemIter {
            read: 1u64,
            write: 0u64
        }) as Box<System + 'static>;

        let f = Box::new(TestSystemIter {
            read: 1u64,
            write: 1u64
        }) as Box<System + 'static>;

        let mut systems = vec![a, b, c, d, e, f];
        let batches = super::SystemBatchIter {
            v: systems.as_mut_slice()
        };

        let batches = batches.collect::<Vec<&mut [Box<System>]>>();

        for batch in batches.iter() {
            let keys = batch.iter().map(|s| s.resource_key()).collect::<Vec<(u64, u64)>>();
            println!("{:?}", keys);
        }

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[0][0].resource_key(), (1u64, 0u64));
        assert_eq!(batches[0][1].resource_key(), (3u64, 2u64));
        assert_eq!(batches[0][2].resource_key(), (1u64, 0u64));
        assert_eq!(batches[1].len(), 2);
        assert_eq!(batches[1][0].resource_key(), (3u64, 0u64));
        assert_eq!(batches[1][1].resource_key(), (1u64, 0u64));
        assert_eq!(batches[2].len(), 1);
        assert_eq!(batches[2][0].resource_key(), (1u64, 1u64));
    }

    #[test]
    fn create_fnsystem_r1w1() {
        let mut world = World::new();
        let r1_key = world.register_resource(TestResource { x: 1 });
        let r2_key = world.register_resource(TestResource2 {});

        let s = world.system_r1w1(|_, _: &TestResource, _: &mut TestResource2| {});

        let expected = ((1 << r1_key) | (1 << r2_key), 1 << r2_key);
        assert_eq!(s.resource_key(), expected);
    }

    #[test]
    fn clean_deleted_entities() {
        let mut world = World::new();
        world.register_entity_resource(VecResource::<u32>::new());

        let init = world.system_r0w1(|tx, resource: &mut VecResource<u32>| {
            println!("Creating entities");
            for i in 0..1000 {
                let e = tx.create();
                resource.add(e.index(), i);
            }
            println!("Entities created");
        });

        let verify_init = world.system_r1w0(|_, resource: &VecResource<u32>| {
            println!("Verifying entity creation");
            iter_entities_r1w0(resource, |iter, r| {
                for e in iter {
                    assert_eq!(e, *r.get(e).unwrap());
                }
            });
            println!("Verified entity creation");
        });

        let delete = world.system_r0w1(|tx, resource: &mut VecResource<u32>| {
            println!("Deleting entities");
            let mut entities = Vec::<Index>::new();
            iter_entities_r1w0(resource, |iter, _| {
                for e in iter {
                    entities.push(e);
                }
            });

            for e in entities {
                let entity = tx.by_index(e);
                tx.destroy(entity);
            }
            println!("Deleted entities");
        });

        let verify_delete = world.system_r1w0(|_, resource: &VecResource<u32>| {
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

        world.update(&mut vec![init, verify_init, delete, verify_delete]);
    }
}
