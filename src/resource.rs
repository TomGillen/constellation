use std::ops::{Deref, DerefMut};
use std::collections::hash_map;
use std::collections::hash_map::{Values, ValuesMut};
use std::slice;
use std::any::Any;

use fnv::FnvHashMap;

use entities::*;
use bitset::*;
use join::*;

pub trait Resource : Any + Send + Sync { }

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

pub trait StoresEntityData : Send + Sync {
    fn clear(&mut self, &[Index]);
}

impl StoresEntityData {
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

pub trait EntityResource : Resource + StoresEntityData {
    //type Filter: BitSetLike;
    type Api;

    // fn deconstruct(&self) -> (&Self::Filter, &Self::Api);
    // fn deconstruct_mut(&mut self) -> (&Self::Filter, &mut Self::Api);

    fn deconstruct(&self) -> (&BitSet, &Self::Api);
    fn deconstruct_mut(&mut self) -> (&BitSet, &mut Self::Api);
}

macro_rules! impl_iter_entities {
    ($name:ident [$($read:ident),*] [$($write:ident),*] [$iter:ty]) => (
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

// todo: find out why the compiler gets confused when using associated types in the FnOnce with iter_entities for the iterator and BitSet
// solving this would eliminate the need to provide the iterator type in the macro invokations, and allow EntityResources to specify alternate BitSetLike filters via associated types

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

pub struct MapResource<T: Any + Send + Sync> {
    filter: BitSet,
    storage: MapStorage<T>
}

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

impl<T: Any + Send + Sync> Resource for MapResource<T> { }

impl<T: Any + Send + Sync> EntityResource for MapResource<T> {
    //type Filter = BitSet;
    type Api = MapStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &MapStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut MapStorage<T>) {
        (&self.filter, &mut self.storage)
    }
}

impl<T: Any + Send + Sync> StoresEntityData for MapResource<T> {
    fn clear(&mut self, entities: &[Index]) {
        for entity in entities {
            self.remove(*entity);
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
    m: FnvHashMap<Index, T>
}

impl<T> MapStorage<T> {
    pub fn new() -> MapStorage<T> {
        MapStorage {
            m: FnvHashMap::default()
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
        if self.storage.v.len() <= index {
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
    //type Filter = BitSet;
    type Api = VecStorage<T>;

    fn deconstruct(&self) -> (&BitSet, &VecStorage<T>) {
        (&self.filter, &self.storage)
    }

    fn deconstruct_mut(&mut self) -> (&BitSet, &mut VecStorage<T>) {
        (&self.filter, &mut self.storage)
    }
}

impl<T: Any + Send + Sync> StoresEntityData for VecResource<T> {
    fn clear(&mut self, entities: &[Index]) {
        for entity in entities {
            self.remove(*entity);
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
