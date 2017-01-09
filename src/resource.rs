use std::ops::{Deref, DerefMut};
use std::collections::hash_map;
use std::collections::hash_map::{Values, ValuesMut};
use std::slice::{Iter, IterMut};
use std::any::Any;

use fnv::FnvHashMap;

use entities::*;
use bitset::*;
use world::*;

/// A resource whos system access is controlled by the `World`.
pub trait Resource: Any + Send + Sync {
    /// Clears all data related to the given entities from the resource.
    fn clear_entity_data(&mut self, &[Entity]) {}

    /// Converts this resource into a `ResourceBuilder` for constructing itself.
    fn to_builder(self) -> ResourceBuilder<Self>
        where Self: Sized
    {
        ResourceBuilder::new(self)
    }
}

impl Resource {
    /// Returns a reference to the boxed value, blindly assuming it to be of type `T`.
    /// If you are not *absolutely certain* of `T`, you *must not* call this.
    #[inline]
    pub unsafe fn downcast_ref_unsafe<T>(&self) -> &T {
        &*(self as *const Self as *const T)
    }

    /// Returns a reference to the boxed value, blindly assuming it to be of type `T`.
    /// If you are not *absolutely certain* of `T`, you *must not* call this.
    #[inline]
    pub unsafe fn downcast_mut_unsafe<T>(&mut self) -> &mut T {
        &mut *(self as *mut Self as *mut T)
    }
}

/// An entity resource is a resource which stores data about entities.
pub trait EntityResource: Resource {
    // type Filter: BitSetLike;

    /// The type of API used to access the resource while its filter
    /// is write-locked behind a borrow.
    type Api;

    // fn deconstruct(&self) -> (&Self::Filter, &Self::Api);
    // fn deconstruct_mut(&mut self) -> (&Self::Filter, &mut Self::Api);

    /// Splits the entity resource into a bitset used for entity iteration,
    /// and its restricted API.
    fn deconstruct(&self) -> (&BitSet, &Self::Api);

    /// Splits the entity resource into a bitset used for entity iteration,
    /// and its restricted API.
    fn deconstruct_mut(&mut self) -> (&BitSet, &mut Self::Api);
}

/// A component resource is a resource which stores data related to entities
/// in the form of a single `Self::Component` per entity.
pub trait ComponentResourceApi {
    /// The type of data stored for each entity.
    type Component;

    /// Gets a shared reference to the component associated with the given
    /// entity, if present.
    fn get(&self, Entity) -> Option<&Self::Component>;

    /// Gets a mutable reference to the component associated with the given
    /// entity, if present.
    fn get_mut(&mut self, Entity) -> Option<&mut Self::Component>;

    /// Gets a shared reference to the component associated with the given
    /// entity without performing any bounds or liveness checking.
    ///
    /// # Safety
    ///
    /// This function performs no bounds checking. Requesting data for an entity
    /// that does not represent a living entity with data in this resource
    /// will return an undefined result.
    unsafe fn get_unchecked(&self, entity: Entity) -> &Self::Component;

    /// Gets a mutable reference to the component associated with the given
    /// entity without performing any bounds or liveness checking.
    ///
    /// # Safety
    ///
    /// This function performs no bounds checking. Requesting data for an entity
    /// that does not represent a living entity with data in this resource
    /// will return an undefined result.
    unsafe fn get_unchecked_mut(&mut self, entity: Entity) -> &mut Self::Component;
}

/// A `MapResource` stores per-entity data in a `HashMap`.
///
/// This entity resource is suitable for data which is only present for a small
/// portion of the total entities in the `World`.
pub struct MapResource<T: Any + Send + Sync> {
    filter: BitSet,
    storage: MapStorage<T>,
}

impl<T: Any + Send + Sync> MapResource<T> {
    /// Constructs a new `MapResource`.
    pub fn new() -> MapResource<T> {
        MapResource {
            filter: BitSet::new(),
            storage: MapStorage::new(),
        }
    }

    /// Adds entity data to the resource.
    pub fn add(&mut self, entity: Entity, component: T) {
        self.storage.m.insert(entity, component);
        self.filter.add(entity.index());
    }

    /// Removes entity data from the resource.
    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let component = self.storage.m.remove(&entity);
        if component.is_some() {
            self.filter.remove(entity.index());
        }
        return component;
    }
}

impl<T: Any + Send + Sync> Resource for MapResource<T> {
    fn clear_entity_data(&mut self, entities: &[Entity]) {
        for entity in entities {
            self.remove(*entity);
        }
    }

    fn to_builder(self) -> ResourceBuilder<MapResource<T>> {
        ResourceBuilder::new(self).activate_entity_disposal()
    }
}

impl<T: Any + Send + Sync> EntityResource for MapResource<T> {
    // type Filter = BitSet;
    type Api = MapStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &MapStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut MapStorage<T>) {
        (&self.filter, &mut self.storage)
    }
}

impl<T: Any + Send + Sync> Deref for MapResource<T> {
    type Target = MapStorage<T>;

    fn deref(&self) -> &MapStorage<T> {
        &self.storage
    }
}

impl<T: Any + Send + Sync> DerefMut for MapResource<T> {
    fn deref_mut(&mut self) -> &mut MapStorage<T> {
        &mut self.storage
    }
}

/// Provides methods to retrieve and mutate entity data
/// stored inside a `MapResource`.
pub struct MapStorage<T> {
    m: FnvHashMap<Entity, T>,
}

impl<T> MapStorage<T> {
    fn new() -> MapStorage<T> {
        MapStorage { m: FnvHashMap::default() }
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn iter_components(&self) -> Values<Entity, T> {
        self.m.values()
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn iter_components_mut(&mut self) -> ValuesMut<Entity, T> {
        self.m.values_mut()
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn iter(&self) -> hash_map::Iter<Entity, T> {
        self.m.iter()
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn iter_mut(&mut self) -> hash_map::IterMut<Entity, T> {
        self.m.iter_mut()
    }
}

impl<T> ComponentResourceApi for MapStorage<T> {
    type Component = T;

    fn get(&self, entity: Entity) -> Option<&T> {
        self.m.get(&entity)
    }

    #[inline]
    unsafe fn get_unchecked(&self, entity: Entity) -> &T {
        self.m.get(&entity).unwrap()
    }

    fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.m.get_mut(&entity)
    }

    #[inline]
    unsafe fn get_unchecked_mut(&mut self, entity: Entity) -> &mut T {
        self.m.get_mut(&entity).unwrap()
    }
}

/// A `VecResource` stores per-entity data in a `Vec`.
///
/// This entity resource is suitable for data which is present for almost all of
/// the total entities in the `World`. The `VecResource` provides fast
/// sequential access as long as it maintains high occupancy.
pub struct VecResource<T: Any + Send + Sync> {
    filter: BitSet,
    storage: VecStorage<T>,
}

impl<T: Any + Send + Sync> Resource for VecResource<T> {
    fn clear_entity_data(&mut self, entities: &[Entity]) {
        for entity in entities {
            self.remove(*entity);
        }
    }

    fn to_builder(self) -> ResourceBuilder<VecResource<T>> {
        ResourceBuilder::new(self).activate_entity_disposal()
    }
}

impl<T: Any + Send + Sync> VecResource<T> {
    /// Constructs a new `VecResource`.
    pub fn new() -> VecResource<T> {
        VecResource {
            filter: BitSet::new(),
            storage: VecStorage::new(),
        }
    }

    /// Adds entity data to the resource.
    pub fn add(&mut self, entity: Entity, component: T) {
        use std::ptr;

        let index = entity.index() as usize;

        // expand storage if needed
        if self.storage.v.len() <= index {
            let additional = index - self.storage.v.len() + 1;

            // we leave the extra memory uninitialized
            self.storage.v.reserve(additional);
            unsafe {
                self.storage.v.set_len(index + 1);
            }

            self.storage.g.reserve(additional);
            for _ in 0..additional {
                self.storage.g.push(None);
            }
        }

        if self.storage.g[index] != None {
            panic!("VecResource already contains entity data for index {}",
                   index);
        }

        // copy the component into the array without reading/dropping
        // the existing (uninitialized) value
        unsafe {
            ptr::write(self.storage.v.get_unchecked_mut(index), component);
        }
        self.storage.g[index] = Some(entity.generation());
        self.filter.add(entity.index());
    }

    /// Removes entity data from the resource.
    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        use std::ptr;

        let index = entity.index() as usize;
        if self.storage.v.len() <= index || self.storage.g[index] != Some(entity.generation()) {
            return None;
        }

        self.filter.remove(entity.index());
        self.storage.g[index] = None;

        // copy the component out of thr array
        // - we will now treat this slot as unitialized
        Some(unsafe { ptr::read(&self.storage.v[index]) })
    }
}

impl<T: Any + Send + Sync> EntityResource for VecResource<T> {
    // type Filter = BitSet;
    type Api = VecStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &VecStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut VecStorage<T>) {
        (&self.filter, &mut self.storage)
    }
}

impl<T: Any + Send + Sync> Deref for VecResource<T> {
    type Target = VecStorage<T>;

    fn deref(&self) -> &VecStorage<T> {
        &self.storage
    }
}

impl<T: Any + Send + Sync> DerefMut for VecResource<T> {
    fn deref_mut(&mut self) -> &mut VecStorage<T> {
        &mut self.storage
    }
}

/// Provides methods to retrieve and mutate entity data stored inside a `VecResource`.
pub struct VecStorage<T> {
    v: Vec<T>,
    g: Vec<Option<Generation>>,
}

impl<T> VecStorage<T> {
    fn new() -> VecStorage<T> {
        VecStorage {
            v: Vec::new(),
            g: Vec::new(),
        }
    }

    /// Gets an iterator over immutable references to all entity data stored in the resource.
    pub fn iter(&self) -> VecStorageIter<T> {
        VecStorageIter {
            i: 0,
            iter: self.v.iter(),
            g: &self.g,
        }
    }

    /// Gets an iterator over mutable references to all entity data stored in the resource.
    pub fn iter_mut(&mut self) -> VecStorageIterMut<T> {
        VecStorageIterMut {
            i: 0,
            iter: self.v.iter_mut(),
            g: &self.g,
        }
    }
}

impl<T> ComponentResourceApi for VecStorage<T> {
    type Component = T;

    fn get(&self, entity: Entity) -> Option<&T> {
        let index = entity.index() as usize;
        if self.g.len() > index && self.g[index] == Some(entity.generation()) {
            return Some(&self.v[index]);
        }
        None
    }

    #[inline]
    unsafe fn get_unchecked(&self, entity: Entity) -> &T {
        self.v.get_unchecked(entity.index() as usize)
    }

    fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let index = entity.index() as usize;
        if self.g.len() > index && self.g[index] == Some(entity.generation()) {
            return Some(&mut self.v[index]);
        }
        None
    }

    #[inline]
    unsafe fn get_unchecked_mut(&mut self, entity: Entity) -> &mut T {
        self.v.get_unchecked_mut(entity.index() as usize)
    }
}

/// An iterator over entity data stored in a `VecResource`.
pub struct VecStorageIter<'a, T: 'a> {
    i: usize,
    iter: Iter<'a, T>,
    g: &'a [Option<Generation>],
}

impl<'a, T> Iterator for VecStorageIter<'a, T> {
    type Item = (Entity, &'a T);

    #[inline]
    fn next(&mut self) -> Option<(Entity, &'a T)> {
        for x in self.iter.by_ref() {
            let index = self.i;
            self.i = self.i + 1;

            if let Some(gen) = self.g[index] {
                let entity = Entity::new(index as Index, gen);
                return Some((entity, x));
            }
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.g.len()))
    }
}

/// An iterator over entity data stored in a `VecResource`.
pub struct VecStorageIterMut<'a, T: 'a> {
    i: usize,
    iter: IterMut<'a, T>,
    g: &'a [Option<Generation>],
}

impl<'a, T> Iterator for VecStorageIterMut<'a, T> {
    type Item = (Entity, &'a mut T);

    #[inline]
    fn next(&mut self) -> Option<(Entity, &'a mut T)> {
        for x in self.iter.by_ref() {
            let index = self.i;
            self.i = self.i + 1;

            if let Some(gen) = self.g[index] {
                let entity = Entity::new(index as Index, gen);
                return Some((entity, x));
            }
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.g.len()))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use entities::*;
    use world::*;

    #[test]
    fn map_deconstruct() {
        let map = &mut MapResource::<u32>::new();
        let entity = Entity::new(1, 0);

        map.add(entity, 5u32);
        assert_eq!(*map.get(entity).unwrap(), 5u32);

        let (bitset, api) = <MapResource<u32> as EntityResource>::deconstruct(&map);
        assert!(bitset.contains(entity.index()));
        assert_eq!(*api.get(entity).unwrap(), 5u32);
    }

    #[test]
    fn map_deconstruct_mut() {
        let map = &mut MapResource::<u32>::new();
        let entity = Entity::new(1, 0);

        map.add(entity, 5u32);
        assert_eq!(*map.get(entity).unwrap(), 5u32);

        let (bitset, api) = <MapResource<u32> as EntityResource>::deconstruct_mut(map);
        assert!(bitset.contains(entity.index()));
        assert_eq!(*api.get(entity).unwrap(), 5u32);
        *api.get_mut(entity).unwrap() = 6u32;
        assert_eq!(*api.get(entity).unwrap(), 6u32);
    }

    #[test]
    fn vec_iter() {
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a, 1);
        vec.add(b, 2);
        vec.add(c, 3);

        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some((a, &1u32)));
        assert_eq!(iter.next(), Some((b, &2u32)));
        assert_eq!(iter.next(), Some((c, &3u32)));
    }

    #[test]
    fn vec_iter_mut() {
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a, 1);
        vec.add(b, 2);
        vec.add(c, 3);

        let mut iter = vec.iter_mut();
        assert_eq!(iter.next(), Some((a, &mut 1u32)));
        assert_eq!(iter.next(), Some((b, &mut 2u32)));
        assert_eq!(iter.next(), Some((c, &mut 3u32)));
    }

    #[test]
    fn run_iter_entities_r1w1() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());
        world.register_resource(MapResource::<u32>::new());
        world.register_resource(VecResource::<u64>::new());

        let mut test = SystemCommandBuffer::default();
        test.queue_systems(|scope| {
            scope.run_r0w3(|ctx, v: &mut VecResource<u32>, m: &mut MapResource<u32>, r: &mut VecResource<u64>| {
                let a = ctx.create();
                let b = ctx.create();
                let c = ctx.create();

                v.add(a, 1);
                v.add(b, 2);
                v.add(c, 3);

                m.add(a, 1);
                m.add(c, 4);

                r.add(a, 0);
                r.add(b, 0);
                r.add(c, 0);
            });

            scope.run_r2w1(|ctx, map: &MapResource<u32>, vec: &VecResource<u32>, out: &mut VecResource<u64>| {
                ctx.iter_r2w1(map, vec, out).entities(|iter, m, v, o| {
                    for e in iter {
                        let x = unsafe { o.get_unchecked_mut(e) };
                        *x = (*v.get(e).unwrap() + *m.get(e).unwrap()) as u64;
                    }
                });
            });

            scope.run_r3w0(|ctx, map: &MapResource<u32>, vec: &VecResource<u32>, out: &VecResource<u64>| {
                let mut checked = 0;

                ctx.iter_r3w0(map, vec, out).entities(|iter, m, v, o| {
                    for e in iter {
                        assert_eq!(*o.get(e).unwrap(),
                                   (m.get(e).unwrap() + v.get(e).unwrap()) as u64);
                        checked = checked + 1;
                    }
                });

                assert_eq!(checked, 2);
            });
        });

        world.run_sequential(&mut test, SequentialExecute::SequentialCommit);
    }
}
