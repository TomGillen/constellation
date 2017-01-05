use std::sync::atomic::{AtomicUsize, Ordering};
use std::fmt;
use crossbeam::sync::SegQueue;

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

    /// Creates a new entity transaction. Transactions can be used to allocate or delete
    /// entities in multiple threads concurrently. Entity deletions are comitted with the
    /// transaction is merged via `merge`.
    pub fn transaction(&self) -> EntitiesTransaction {
        EntitiesTransaction {
            entities: self,
            deleted: match self.deleted_pool.try_pop() {
                Some(vec) => vec,
                None => Vec::new(),
            },
        }
    }

    /// Merges a set of entity transactions, comitting their allocations and delections.
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

/// An entity transaction allows concurrent creations and deletions of entities from an `Entities`.
pub struct EntitiesTransaction<'a> {
    entities: &'a Entities,
    deleted: Vec<Entity>,
}

/// Summarises the final changes made during the lifetime of an entity transaction.
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
    /// Entity destructions are deferred until after the system has completed execution.
    /// All related data stored in entity resources will also be removed at this time.
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

    /// Converts this transaction into a change set, consuming the transaction in the process.
    pub fn to_change_set(self) -> EntityChangeSet {
        EntityChangeSet { deleted: self.deleted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn entity_deconstruct() {
        let entity = Entity::new(5, 10);
        assert!(entity.index() == 5);
        assert!(entity.generation() == 10);
    }

    #[test]
    fn entities_new() {
        Entities::new();
    }

    #[test]
    fn entities_allocate() {
        let em = Entities::new();
        em.allocate();
    }

    #[test]
    fn entities_allocated_entity_is_alive() {
        let em = Entities::new();
        let entity = em.allocate();
        assert!(em.is_alive(&entity));
    }

    #[test]
    fn entities_allocate_many_no_duplicates() {
        let em = Entities::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }
    }

    #[test]
    fn entities_allocate_many_no_duplicates_comitted() {
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

    #[test]
    fn entitiestx_new() {
        let em = Entities::new();
        em.transaction();
    }

    #[test]
    fn entitiestx_create() {
        let em = Entities::new();
        let mut tx = em.transaction();

        tx.create();
    }

    #[test]
    fn entitiestx_created_is_alive() {
        let em = Entities::new();
        let mut tx = em.transaction();

        let entity = tx.create();
        assert!(tx.is_alive(&entity));
    }

    #[test]
    fn entitiestx_merge_allocates() {
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
    fn entitiestx_merge_deletes() {
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
