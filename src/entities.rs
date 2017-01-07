use std::sync::atomic::{AtomicUsize, Ordering};
use std::fmt;
use crossbeam::sync::SegQueue;

use bitset::*;
use resource::*;
use join::*;

/// Only one entity with a given index may be alive at a time.
pub type Index = u32;

/// Generation is incremented each time an index is re-used.
pub type Generation = u8;

/// A handle is formed out of an Index and a Generation
pub trait Handle<TIndex, TGeneration>: Clone + Copy + fmt::Display {
    /// Constructs a new handle.
    fn new(index: TIndex, generation: TGeneration) -> Self;

    /// Gets the index component of the handle.
    fn index(&self) -> TIndex;

    /// Gets the generation component of the handle.
    fn generation(&self) -> TGeneration;
}

/// A handle onto an entity in a scene.
#[derive(PartialEq, Eq, Debug, Copy, Clone, Hash)]
pub struct Entity(u32);

const INDEX_BITS: u8 = 24;
const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;
const GENERATION_BITS: u8 = 8;
const GENERATION_MASK: u32 = (1 << GENERATION_BITS) - 1;
const MINIMUM_FREE_INDICES: usize = 1024;

impl Handle<Index, Generation> for Entity {
    fn new(index: Index, generation: Generation) -> Entity {
        Entity((index & INDEX_MASK) | ((generation as u32 & GENERATION_MASK) << INDEX_BITS))
    }

    fn index(&self) -> Index {
        self.0 & INDEX_MASK
    }

    fn generation(&self) -> Generation {
        let gen = (self.0 >> INDEX_BITS) & GENERATION_MASK;
        gen as u8
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(index: {}, gen: {})", self.index(), self.generation())
    }
}

/// Manages the allocation and delection of `Entity` IDs.
pub struct Entities {
    free_count_approx: AtomicUsize,
    allocated: AtomicUsize,
    generations: Vec<Generation>,
    free: SegQueue<Index>,
    deleted_pool: SegQueue<Vec<Entity>>,
}

impl Entities {
    /// Constructs a new `Entities`.
    pub fn new() -> Entities {
        Entities {
            generations: Vec::new(),
            free: SegQueue::new(),
            free_count_approx: AtomicUsize::new(0),
            allocated: AtomicUsize::new(0),
            deleted_pool: SegQueue::new(),
        }
    }

    /// Creates a new `Entity`. Allocated entities cannot be deleted until after
    /// `commit_allocations` has been called.
    pub fn allocate(&self) -> Entity {
        if self.free_count_approx.load(Ordering::Relaxed) > MINIMUM_FREE_INDICES {
            if let Some(index) = self.free.try_pop() {
                self.free_count_approx.fetch_sub(1, Ordering::Relaxed);
                let generation = self.generations[index as usize];
                return Entity::new(index, generation);
            }
        }

        let index = self.allocated.fetch_add(1, Ordering::SeqCst);
        return Entity::new(index as Index, 0 as Generation);
    }

    /// Determines if the specified `Entity` is still alive.
    pub fn is_alive(&self, entity: &Entity) -> bool {
        let index = entity.index() as usize;
        match self.generations.get(index) {
            Some(&g) => g == entity.generation(),
            None => self.allocated.load(Ordering::Relaxed) > index,
        }
    }

    /// Gets the count of currently allocated entities.
    pub fn count(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Gets the currently living `Entity` with the given `Index`.
    pub fn by_index(&self, index: Index) -> Entity {
        match self.generations.get(index as usize) {
            Some(&g) => Entity::new(index, g),
            None => Entity::new(index, 0),
        }
    }

    /// Creates a new entity transaction. Transactions can be used to allocate
    /// or delete entities in multiple threads concurrently. Entity deletions
    /// are comitted with the transaction is merged via `merge`.
    pub fn transaction(&self) -> EntitiesTransaction {
        EntitiesTransaction {
            entities: self,
            deleted: self.deleted_pool.try_pop().unwrap_or(Vec::new()),
        }
    }

    /// Merges a set of entity transactions, comitting their allocations and
    /// delections.
    pub fn merge<T: Iterator<Item = EntityChangeSet>>(&mut self, changes: T) {
        self.commit_allocations();

        let mut freed = 0;
        for set in changes {
            let mut deleted = set.deleted;
            for e in deleted.drain(..) {
                let index = e.index() as usize;
                self.generations[index] = self.generations[index] + 1;
                self.free.push(e.index());
                freed = freed + 1;
            }

            self.deleted_pool.push(deleted);
        }

        self.free_count_approx.fetch_add(freed, Ordering::Relaxed);
    }

    fn commit_allocations(&mut self) {
        let allocated = self.allocated.load(Ordering::Acquire);
        let new_entities = allocated - self.generations.len();
        if new_entities > 0 {
            self.generations.resize(allocated, 0);
        }
    }
}

/// Provides methods for efficiently iterating through entities.
pub struct EntityIterBuilder<'a> {
    entities: &'a Entities,
}

impl<'a> EntityIterBuilder<'a> {
    /// Constructs a new 'EntityIterBuilder'.
    pub fn new(entities: &'a Entities) -> EntityIterBuilder<'a> {
        EntityIterBuilder { entities: entities }
    }
}

/// An iterator which converts indexes into entitiy IDs.
pub struct EntityIter<'a, T>
    where T: Iterator<Item = Index>
{
    entities: &'a Entities,
    iter: T,
}

impl<'a, T: Iterator<Item = Index>> EntityIter<'a, T> {
    /// Constructs a new 'EntityIter'.
    pub fn new(entities: &'a Entities, iter: T) -> EntityIter<'a, T> {
        EntityIter {
            entities: entities,
            iter: iter,
        }
    }
}

impl<'a, T: Iterator<Item = Index>> Iterator for EntityIter<'a, T> {
    type Item = Entity;

    #[inline]
    fn next(&mut self) -> Option<Entity> {
        self.iter.next().map(|i| self.entities.by_index(i))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

macro_rules! impl_iter_entities {
    ($name:ident [$($read:ident),*] [$($write:ident),*] [$iter:ty]) => (
        impl<'b> EntityIterBuilder<'b> {
        /// Constructs an iterator which yields each entity with
        /// data stored in all given entity resources.
        ///
        /// This function borrows each resource, preventing mutation of the
        /// resource for the duration of its' scope. However, the user is
        /// provided with restricted APIs for each resource which may allow
        /// mutable access to entity data stored within the resource, without
        /// allowing any operations which would invalidate the entity iterator.
        #[allow(non_snake_case)]
        pub fn $name<'a, $($read,)* $($write,)* F, R>(&self, $($read: &$read,)* $($write: &mut $write,)* f: F) -> R
            where $($read:EntityResource,)*
                  $($write:EntityResource,)*
                  F: FnOnce(EntityIter<'b, $iter>, $(&$read::Api,)* $(&mut $write::Api,)*) -> R + 'a
        {
            $(let $read = $read.deconstruct();)*
            $(let $write = $write.deconstruct_mut();)*
            let iter = ($($read.0,)* $($write.0,)*).and().iter();

            f(EntityIter::new(self.entities, iter), $($read.1,)* $($write.1,)*)
        }
        }
    )
}

impl_iter_entities!(r0w1 [] [W0] [BitIter<&BitSet>]);
impl_iter_entities!(r0w2 [] [W0, W1] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(r0w3 [] [W0, W1, W2] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r1w0 [R0] [] [BitIter<&BitSet>]);
impl_iter_entities!(r1w1 [R0] [W0] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(r1w2 [R0] [W0, W1] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r1w3 [R0] [W0, W1, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r2w0 [R0, R1] [] [BitIter<BitSetAnd<&BitSet, &BitSet>>]);
impl_iter_entities!(r2w1 [R0, R1] [W0] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r2w2 [R0, R1] [W0, W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r2w3 [R0, R1] [W0, W1, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(r3w0 [R0, R1, R2] [] [BitIter<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r3w1 [R0, R1, R2] [W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r3w2 [R0, R1, R2] [W1, W2] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(r3w3 [R0, R1, R2] [W1, W2, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(r4w0 [R0, R1, R2, R3] [] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>]);
impl_iter_entities!(r4w1 [R0, R1, R2, R3] [W1] [BitIter<BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(r4w2 [R0, R1, R2, R3] [W1, W2] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>>>]);
impl_iter_entities!(r4w3 [R0, R1, R2, R3] [W1, W2, W3] [BitIter<BitSetAnd<BitSetAnd<&BitSet, BitSetAnd<&BitSet, &BitSet>>, BitSetAnd<BitSetAnd<&BitSet, &BitSet>, BitSetAnd<&BitSet, &BitSet>>>>]);

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

/// An entity transaction allows concurrent creations and deletions of entities
/// from an `Entities`.
pub struct EntitiesTransaction<'a> {
    entities: &'a Entities,
    deleted: Vec<Entity>,
}

/// Summarises the final changes made during the lifetime of an
/// entity transaction.
pub struct EntityChangeSet {
    /// The entities deleted in the transaction.
    pub deleted: Vec<Entity>,
}

impl<'a> EntitiesTransaction<'a> {
    /// Creates a new `Entity`.
    ///
    /// This `Entity` can immediately be used to register data with resources,
    /// and the calling system may destroy the entity, but other systems running
    /// concurrently will not observe the entity's creation.
    pub fn create(&mut self) -> Entity {
        self.entities.allocate()
    }

    /// Destroys an `Entity`.
    ///
    /// Entity destructions are deferred until after the system has completed
    /// execution. All related data stored in entity resources will also be
    /// removed at this time.
    pub fn destroy(&mut self, entity: Entity) {
        self.deleted.push(entity);
    }

    /// Determines if the given `Entity` is still alive.
    pub fn is_alive(&self, entity: &Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Gets the currently living `Entity` with the given `Index`.
    pub fn by_index(&self, index: Index) -> Entity {
        self.entities.by_index(index)
    }

    /// Converts this transaction into a change set,
    /// consuming the transaction in the process.
    pub fn to_change_set(self) -> EntityChangeSet {
        EntityChangeSet { deleted: self.deleted }
    }
}

#[cfg(test)]
mod entities_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn deconstruct_entity() {
        let entity = Entity::new(5, 10);
        assert!(entity.index() == 5);
        assert!(entity.generation() == 10);
    }

    #[test]
    fn new() {
        Entities::new();
    }

    #[test]
    fn allocate() {
        let em = Entities::new();
        em.allocate();
    }

    #[test]
    fn allocated_entity_is_alive() {
        let em = Entities::new();
        let entity = em.allocate();
        assert!(em.is_alive(&entity));
    }

    #[test]
    fn allocate_many_no_duplicates() {
        let em = Entities::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }
    }

    #[test]
    fn allocate_many_no_duplicates_comitted() {
        let mut em = Entities::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }

        em.commit_allocations();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }
    }
}

#[cfg(test)]
mod entities_transaction_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn new() {
        let em = Entities::new();
        em.transaction();
    }

    #[test]
    fn create() {
        let em = Entities::new();
        let mut tx = em.transaction();

        tx.create();
    }

    #[test]
    fn created_is_alive() {
        let em = Entities::new();
        let mut tx = em.transaction();

        let entity = tx.create();
        assert!(tx.is_alive(&entity));
    }

    #[test]
    fn merge_allocates() {
        let mut em = Entities::new();

        let entity: Entity;
        let cs: EntityChangeSet;

        {
            let mut tx = em.transaction();
            entity = tx.create();
            cs = tx.to_change_set();
        }

        em.merge((vec![cs]).into_iter());

        assert!(em.is_alive(&entity));
    }

    #[test]
    fn merge_deletes() {
        let mut em = Entities::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }

        em.commit_allocations();

        let cs: EntityChangeSet;

        {
            let mut tx = em.transaction();

            for entity in entities.iter() {
                tx.destroy(*entity);
            }

            cs = tx.to_change_set();
        }

        for entity in entities.iter() {
            assert!(em.is_alive(&entity));
        }

        em.merge((vec![cs]).into_iter());

        for entity in entities {
            assert!(!em.is_alive(&entity));
        }
    }
}
