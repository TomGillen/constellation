use std::ops::{Deref, DerefMut};
use std::collections::hash_map;
use std::collections::hash_map::{Values, ValuesMut};
use std::slice::{Iter, IterMut};
use std::any::Any;

use fnv::FnvHashMap;

use entities::*;
use bitset::*;
use join::*;
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

macro_rules! impl_iter_entities {
    ($name:ident [$($read:ident),*] [$($write:ident),*] [$iter:ty]) => (
        /// Constructs an iterator which yields the index of each entity with
        /// data stored in all given entity resources.
        ///
        /// This function borrows each resource, preventing mutation of the
        /// resource for the duration of its' scope. However, the user is
        /// provided with restricted APIs for each resource which may allow
        /// mutable access to entity data stored within the resource, without
        /// allowing any operations which would invalidate the entity iterator.
        #[allow(non_snake_case)]
        pub fn $name<'a, $($read,)* $($write,)* F, R>($($read: &$read,)* $($write: &mut $write,)* f: F) -> R
            where $($read:EntityResource,)*
                  $($write:EntityResource,)*
                  F: FnOnce($iter, $(&$read::Api,)* $(&mut $write::Api,)*) -> R + 'a
        {
            $(let $read = $read.deconstruct();)*
            $(let $write = $write.deconstruct_mut();)*
            let iter = ($($read.0,)* $($write.0,)*).and().iter();

            f(iter, $($read.1,)* $($write.1,)*)
        }
    )
}

impl_iter_entities!(iter_entities_r0w1 [] [W0] [BitIter<&BitSet>]);
impl_iter_entities!(iter_entities_r0w2 [] [W0, W1] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(iter_entities_r0w3 [] [W0, W1, W2] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r1w0 [R0] [] [BitIter<&BitSet>]);
impl_iter_entities!(iter_entities_r1w1 [R0] [W0] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(iter_entities_r1w2 [R0] [W0, W1] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r1w3 [R0] [W0, W1, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r2w0 [R0, R1] [] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(iter_entities_r2w1 [R0, R1] [W0] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r2w2 [R0, R1] [W0, W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r2w3 [R0, R1] [W0, W1, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(iter_entities_r3w0 [R0, R1, R2] [] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r3w1 [R0, R1, R2] [W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r3w2 [R0, R1, R2] [W1, W2] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(iter_entities_r3w3 [R0, R1, R2] [W1, W2, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(iter_entities_r4w0 [R0, R1, R2, R3] [] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(iter_entities_r4w1 [R0, R1, R2, R3] [W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(iter_entities_r4w2 [R0, R1, R2, R3] [W1, W2] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(iter_entities_r4w3 [R0, R1, R2, R3] [W1, W2, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>>]);

// todo: find out why the compiler gets confused when using associated types in
// the FnOnce with iter_entities for the iterator and BitSet.
// Solving this would eliminate the need to provide the iterator type in the
// macro invokations, and allow EntityResources to specify alternate BitSetLike
// filters via associated types.

// pub fn iter_entities_r1w1<'a, R1, W1, F>(r1: &R1, w1: &mut W1, f: F)
//     where R1: EntityResource,
//           W1: EntityResource,
//           F: FnOnce(BitIter<<(&R1::Filter, &W1::Filter) as BitAnd>::Value>, &R1::Api, &mut W1::Api) + 'a
// {
//     let (r1_filter, r1_api) = r1.deconstruct();
//     let (w1_filter, w1_api) = w1.deconstruct_mut();
//     let iter = (r1_filter, w1_filter).and().iter();
//     f(iter, r1_api, w1_api);
// }

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

    /// Gets an immutable reference to entity data.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.m.get(&entity)
    }

    /// Gets a mutable reference to entity data.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.m.get_mut(&entity)
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn components(&self) -> Values<Entity, T> {
        self.m.values()
    }

    /// Gets an iterator over all entity data stored in the resource.
    pub fn components_mut(&mut self) -> ValuesMut<Entity, T> {
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
            panic!("VecResource already contains entity data for index {}", index);
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

    /// Gets an immutable reference to entity data.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        let index = entity.index() as usize;
        if self.g.len() > index && self.g[index] == Some(entity.generation()) {
            return Some(&self.v[index]);
        }
        None
    }

    /// Gets an immutable reference to entity data without performing bounds
    /// and liveness checks.
    ///
    /// # Safety
    ///
    /// This function performs no bounds checking. Requesting data for an entity
    /// index that does not represent a living entity with data in this resource
    /// will return an undefined result.
    #[inline]
    pub unsafe fn get_unchecked(&self, index: Index) -> &T {
        self.v.get_unchecked(index as usize)
    }

    /// Gets a mutable reference to entity data.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let index = entity.index() as usize;
        if self.g.len() > index && self.g[index] == Some(entity.generation()) {
            return Some(&mut self.v[index]);
        }
        None
    }

    /// Gets a mutable reference to entity data without performing bounds and
    /// liveness checks.
    ///
    /// # Safety
    ///
    /// This function performs no bounds checking. Requesting data for an entity
    /// index that does not represent a living entity with data in this resource
    /// will return an undefined result.
    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: Index) -> &mut T {
        self.v.get_unchecked_mut(index as usize)
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

    #[test]
    fn map_deconstruct() {
        let map = &mut MapResource::<u32>::new();
        let entity = Entity::new(1, 0);

        map.add(entity, 5u32);
        assert_eq!(*map.get(entity).unwrap(), 5u32);

        let (bitset, api) = map.deconstruct();
        assert!(bitset.contains(entity.index()));
        assert_eq!(*api.get(entity).unwrap(), 5u32);
    }

    #[test]
    fn map_deconstruct_mut() {
        let map = &mut MapResource::<u32>::new();
        let entity = Entity::new(1, 0);

        map.add(entity, 5u32);
        assert_eq!(*map.get(entity).unwrap(), 5u32);

        let (bitset, api) = map.deconstruct_mut();
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
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a, 1);
        vec.add(b, 2);
        vec.add(c, 3);

        let map = &mut MapResource::<u32>::new();
        map.add(a, 1);
        map.add(c, 4);

        iter_entities_r1w1(map, vec, |iter, m, v| {
            for e in iter {
                let x = unsafe { v.get_unchecked_mut(e) };
                *x += *m.get(Entity::new(e, 0)).unwrap();
            }
        });

        assert_eq!(vec.get(a), Some(&2u32));
        assert_eq!(vec.get(b), Some(&2u32));
        assert_eq!(vec.get(c), Some(&7u32));
    }
}
