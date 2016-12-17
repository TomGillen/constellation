use std::ops::{Deref, DerefMut};
use std::collections::HashMap;
use std::collections::hash_map;
use std::collections::hash_map::{Values, ValuesMut};
use std::slice;
use std::sync::{RwLockReadGuard, RwLockWriteGuard};
use std::marker::PhantomData;

use mopa::Any;

use entities::*;
use bitset::*;
use join::*;

pub trait Resource : Any + Send + Sync { }
mopafy!(Resource);

pub trait EntityResource : Resource {
    type Filter: BitSetLike;
    type Api;

    fn deconstruct(&self) -> (&Self::Filter, &Self::Api);
    fn deconstruct_mut(&mut self) -> (&Self::Filter, &mut Self::Api);
    fn clear(&mut self, &[Entity]);
}

pub struct ResourceReadGuard<'a, T: Resource> {
    phantom: PhantomData<T>,
    guard: RwLockReadGuard<'a, Box<Resource>>
}

impl<'a, T: Resource> ResourceReadGuard<'a, T> {
    pub fn new(guard: RwLockReadGuard<'a, Box<Resource>>) -> ResourceReadGuard<'a, T> {
        ResourceReadGuard {
            phantom: PhantomData,
            guard: guard
        }
    }
}

impl<'a, T: Resource> Deref for ResourceReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let boxed = self.guard.deref();
        unsafe { &boxed.deref().downcast_ref_unchecked::<T>() }
    }
}

pub struct ResourceWriteGuard<'a, T: Resource> {
    phantom: PhantomData<T>,
    guard: RwLockWriteGuard<'a, Box<Resource>>
}

impl<'a, T: Resource> ResourceWriteGuard<'a, T> {
    pub fn new(guard: RwLockWriteGuard<'a, Box<Resource>>) -> ResourceWriteGuard<'a, T> {
        ResourceWriteGuard {
            phantom: PhantomData,
            guard: guard
        }
    }
}

impl<'a, T: Resource> Deref for ResourceWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let boxed = self.guard.deref();
        unsafe { &boxed.deref().downcast_ref_unchecked::<T>() }
    }
}

impl<'a, T: Resource> DerefMut for ResourceWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.guard.deref_mut().downcast_mut_unchecked::<T>() }
    }
}

pub fn iter_entities_r1w1<'a, R1, W1, F>(r1: &R1, w1: &mut W1, f: F)
    where R1: EntityResource,
          W1: EntityResource,
          F: FnOnce(BitIter<BitSetAnd<&<R1 as EntityResource>::Filter, &<W1 as EntityResource>::Filter>>, &R1::Api, &mut W1::Api) + 'a
{
    let (r1_filter, r1_api) = r1.deconstruct();
    let (w1_filter, w1_api) = w1.deconstruct_mut();
    let iter = (r1_filter, w1_filter).and().iter();
    f(iter, r1_api, w1_api);
}

pub struct MapResource<T: Any + Send + Sync> {
    filter: BitSet,
    storage: MapStorage<T>
}

impl<T: Any + Send + Sync> Resource for MapResource<T> { }

impl<T: Any + Send + Sync> MapResource<T> {
    pub fn new() -> MapResource<T> {
        MapResource {
            filter: BitSet::new(),
            storage: MapStorage::new()
        }
    }

    pub fn add(&mut self, entity: Index, component: T) {
        self.storage.m.insert(entity, component);
        self.filter.add(entity);
    }

    pub fn remove(&mut self, entity: Index) -> Option<T> {
        let result = self.storage.m.remove(&entity);
        if result.is_some() {
            self.filter.remove(entity);
        }

        result
    }
}

impl<T: Any + Send + Sync> EntityResource for MapResource<T> {
    type Filter = BitSet;
    type Api = MapStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &MapStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut MapStorage<T>) {
        (&self.filter, &mut self.storage)
    }

    fn clear(&mut self, entities: &[Entity]) {
        for entity in entities {
            self.remove(entity.index());
        }
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

pub struct MapStorage<T> {
    m: HashMap<Index, T>
}

impl<T> MapStorage<T> {
    pub fn new() -> MapStorage<T> {
        MapStorage {
            m: HashMap::new()
        }
    }

    pub fn get(&self, entity: Index) -> Option<&T> {
        self.m.get(&entity)
    }

    pub fn get_mut(&mut self, entity: Index) -> Option<&mut T> {
        self.m.get_mut(&entity)
    }

    pub fn components(&self) -> Values<Index, T> {
        self.m.values()
    }

    pub fn components_mut(&mut self) -> ValuesMut<Index, T> {
        self.m.values_mut()
    }

    pub fn iter(&self) -> hash_map::Iter<Index, T> {
        self.m.iter()
    }

    pub fn iter_mut(&mut self) -> hash_map::IterMut<Index, T> {
        self.m.iter_mut()
    }
}

pub struct VecResource<T: Any + Send + Sync> {
    filter: BitSet,
    storage: VecStorage<T>
}

impl<T: Any + Send + Sync> Resource for VecResource<T> { }

impl<T: Any + Send + Sync> VecResource<T> {
    pub fn new() -> VecResource<T> {
        VecResource {
            filter: BitSet::new(),
            storage: VecStorage::new()
        }
    }

    pub fn add(&mut self, entity: Index, component: T) {
        let index = entity as usize;
        if self.storage.v.len() <= index {
            //self.storage.v.resize(index + 1, None);
            let additional = index - self.storage.v.len() + 1;
            self.storage.v.reserve(additional);
            for _ in 0..additional {
                self.storage.v.push(None);
            }
        }

        self.storage.v[index] = Some(component);
        self.filter.add(entity);
    }

    pub fn remove(&mut self, entity: Index) -> Option<T> {
        let index = entity as usize;
        if self.storage.v.len() >= index {
            return None;
        }

        self.storage.v.push(None);
        let component = self.storage.v.swap_remove(index);
        if component.is_some() {
            self.filter.remove(entity);
        }

        return component;
    }
}

impl<T: Any + Send + Sync> EntityResource for VecResource<T> {
    type Filter = BitSet;
    type Api = VecStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &VecStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut VecStorage<T>) {
        (&self.filter, &mut self.storage)
    }

    fn clear(&mut self, entities: &[Entity]) {
        for entity in entities {
            self.remove(entity.index());
        }
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

pub struct VecStorage<T> {
    v: Vec<Option<T>>
}

impl<T> VecStorage<T> {
    pub fn new() -> VecStorage<T> {
        VecStorage {
            v: Vec::new()
        }
    }

    pub fn get(&self, entity: Index) -> Option<&T> {
        if let Some(x) = self.v.get(entity as usize) {
            return x.as_ref();
        }

        None
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, entity: Index) -> &T {
        self.v.get_unchecked(entity as usize).as_ref().unwrap()
    }

    pub fn get_mut(&mut self, entity: Index) -> Option<&mut T> {
        if let Some(x) = self.v.get_mut(entity as usize) {
            return x.as_mut();
        }

        None
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, entity: Index) -> &mut T {
        self.v.get_unchecked_mut(entity as usize).as_mut().unwrap()
    }

    pub fn iter(&self) -> VecStorageIter<T> {
        VecStorageIter {
            iter: self.v.iter()
        }
    }

    pub fn iter_mut(&mut self) -> VecStorageIterMut<T> {
        VecStorageIterMut {
            iter: self.v.iter_mut()
        }
    }
}

pub struct VecStorageIter<'a, T: 'a> {
    iter: slice::Iter<'a, Option<T>>
}

impl<'a, T> Iterator for VecStorageIter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<&'a T> {
        for x in self.iter.by_ref() {
            if x.is_some() {
                return x.as_ref();
            }
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
    }
}

pub struct VecStorageIterMut<'a, T: 'a> {
    iter: slice::IterMut<'a, Option<T>>
}

impl<'a, T> Iterator for VecStorageIterMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<&'a mut T> {
        for x in self.iter.by_ref() {
            if x.is_some() {
                return x.as_mut();
            }
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
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

        map.add(entity.index(), 5u32);
        assert_eq!(*map.get(entity.index()).unwrap(), 5u32);

        let (bitset, api) = map.deconstruct();
        assert!(bitset.contains(entity.index()));
        assert_eq!(*api.get(entity.index()).unwrap(), 5u32);
    }

    #[test]
    fn map_deconstruct_mut() {
        let map = &mut MapResource::<u32>::new();
        let entity = Entity::new(1, 0);

        map.add(entity.index(), 5u32);
        assert_eq!(*map.get(entity.index()).unwrap(), 5u32);

        let (bitset, api) = map.deconstruct_mut();
        assert!(bitset.contains(entity.index()));
        assert_eq!(*api.get(entity.index()).unwrap(), 5u32);
        *api.get_mut(entity.index()).unwrap() = 6u32;
        assert_eq!(*api.get(entity.index()).unwrap(), 6u32);
    }

    #[test]
    fn vec_iter() {
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a.index(), 1);
        vec.add(b.index(), 2);
        vec.add(c.index(), 3);

        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some(&1u32));
        assert_eq!(iter.next(), Some(&2u32));
        assert_eq!(iter.next(), Some(&3u32));
    }

    #[test]
    fn vec_iter_mut() {
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a.index(), 1);
        vec.add(b.index(), 2);
        vec.add(c.index(), 3);

        let mut iter = vec.iter_mut();
        assert_eq!(iter.next(), Some(&mut 1u32));
        assert_eq!(iter.next(), Some(&mut 2u32));
        assert_eq!(iter.next(), Some(&mut 3u32));
    }

    #[test]
    fn run_iter_entities_r1w1() {
        let a = Entity::new(1, 0);
        let b = Entity::new(2, 0);
        let c = Entity::new(4, 0);

        let vec = &mut VecResource::<u32>::new();
        vec.add(a.index(), 1);
        vec.add(b.index(), 2);
        vec.add(c.index(), 3);

        let map = &mut MapResource::<u32>::new();
        map.add(a.index(), 1);
        map.add(c.index(), 4);

        iter_entities_r1w1(map, vec, |iter, m, v| {
            vec.get(a.index());
            for e in iter {
                let x = unsafe { v.get_unchecked_mut(e) };
                *x += *m.get(e).unwrap();
            }
        });

        assert_eq!(vec.get(a.index()), Some(&2u32));
        assert_eq!(vec.get(b.index()), Some(&2u32));
        assert_eq!(vec.get(c.index()), Some(&7u32));
    }
}
