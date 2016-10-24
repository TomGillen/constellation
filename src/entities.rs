use std::slice::Iter;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::fmt;
use crossbeam::sync::SegQueue;

pub type Index = u32;
pub type Generation = u8;

pub trait Handle<TIndex, TGeneration> : Clone + Copy + fmt::Display {
    fn new(index: TIndex, generation: TGeneration) -> Self;
    fn index(&self) -> TIndex;
    fn generation(&self) -> TGeneration;
}

/// A handle onto an entity in a scene.
///
/// An entity handle is opaque data understandable onto to the `EntityManager` and used as a key
/// when attaching components to an entity.
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

pub trait EntityKey : Eq + Clone + Sync {
    fn new() -> Self;
    fn register(&self, resource: usize);
    fn unregister(&self, resource: usize);
    fn is_registered(&self, resource: usize) -> bool;
    fn matches(&self, other: &Self) -> bool;
}

#[derive(Debug)]
pub struct Key64 {
    key: AtomicUsize
}

impl Clone for Key64 {
    fn clone(&self) -> Key64 {
        Key64 {
            key: AtomicUsize::new(self.key.load(Ordering::Acquire))
        }
    }
}

impl PartialEq for Key64 {
    fn eq(&self, other: &Key64) -> bool {
        self.key.load(Ordering::Relaxed) == other.key.load(Ordering::Relaxed)
    }
}

impl Eq for Key64 {}

impl EntityKey for Key64 {
    #[inline]
    fn new() -> Key64 {
        Key64 { key: AtomicUsize::new(0) }
    }

    #[inline]
    fn register(&self, resource: usize) {
        let mut old = self.key.load(Ordering::Relaxed);
        loop {
            let new = old | (1 << resource);
            match self.key.compare_exchange_weak(old, new, Ordering::SeqCst, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => old = x
            }
        }
    }

    #[inline]
    fn unregister(&self, resource: usize) {
        let mut old = self.key.load(Ordering::Relaxed);
        loop {
            let new = old & !(1 << resource);
            match self.key.compare_exchange_weak(old, new, Ordering::SeqCst, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => old = x
            }
        }
    }

    #[inline]
    fn is_registered(&self, resource: usize) -> bool {
        let mask = 1 << resource;
        let key = self.key.load(Ordering::Relaxed);
        (key & mask) == mask
    }

    #[inline]
    fn matches(&self, other: &Key64) -> bool {
        let this_key = self.key.load(Ordering::Relaxed);
        let other_key = other.key.load(Ordering::Relaxed);
        (this_key & other_key) == other_key
    }
}

pub struct Entities<TKey: EntityKey> {
    free_count_approx: AtomicUsize,
    allocated: AtomicUsize,
    generations: Vec<Generation>,
    free: SegQueue<Index>,
    keys: Vec<TKey>,
    created_pool: SegQueue<Vec<(Entity, TKey)>>,
    deleted_pool: SegQueue<Vec<Index>>
}

pub struct EntityIter<'a, TKey: EntityKey + 'a> {
    generations: &'a [Generation],
    keys: &'a [TKey],
    key: TKey,
    i: usize
}

impl<'a, TKey: EntityKey> Iterator for EntityIter<'a, TKey> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        while self.i < self.keys.len() {
            let i = self.i;
            self.i = self.i + 1;

            if self.keys[i].matches(&self.key) {
                return Some(Entity::new(i as Index, self.generations[i]));
            }
        }

        return None;
    }
}

impl<TKey: EntityKey> Entities<TKey> {
    pub fn new() -> Entities<TKey> {
        Entities {
            generations: Vec::new(),
            free: SegQueue::new(),
            free_count_approx: AtomicUsize::new(0),
            allocated: AtomicUsize::new(0),
            keys: Vec::new(),
            created_pool: SegQueue::new(),
            deleted_pool: SegQueue::new()
        }
    }

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

    pub fn is_alive(&self, entity: &Entity) -> bool {
        let index = entity.index() as usize;
        match self.generations.get(index) {
            Some(&g) => g == entity.generation(),
            None     => self.allocated.load(Ordering::Relaxed) > index
        }
    }

    pub fn mark_entity(&self, entity: &Entity, resource: usize) -> bool {
        let index = entity.index() as usize;
        if self.keys.len() > index {
            self.keys[index].register(resource);
            return true;
        }

        return false;
    }

    pub fn unmark_entity(&self, entity: &Entity, resource: usize) -> bool {
        let index = entity.index() as usize;
        if self.keys.len() > index {
            self.keys[index].unregister(resource);
            return true;
        }

        return false;
    }

    pub fn count(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    pub fn transaction(&self) -> EntitiesTransaction<TKey> {
        EntitiesTransaction {
            entities: self,
            created: match self.created_pool.try_pop() {
                Some(vec) => vec,
                None      => Vec::new()
            },
            deleted: match self.deleted_pool.try_pop() {
                Some(vec) => vec,
                None      => Vec::new()
            }
        }
    }

    pub fn merge(&mut self, mut changes: EntityChangeSet<TKey>) {
        let mut freed = 0;
        for i in changes.deleted.drain(..) {
            let index = i as usize;
            self.generations[index] = self.generations[index] + 1;
            self.keys[index] = TKey::new();
            self.free.push(i);
            freed = freed + 1;
        }

        self.free_count_approx.fetch_add(freed, Ordering::Relaxed);

        for (entity, key) in changes.created.drain(..) {
            self.keys[entity.index() as usize] = key
        }

        self.created_pool.push(changes.created);
        self.deleted_pool.push(changes.deleted);
    }

    pub fn commit_allocations(&mut self) {
        let allocated = self.allocated.load(Ordering::Acquire);
        let new_entities = allocated - self.generations.len();
        if new_entities > 0 {
            self.generations.resize(allocated, 0);
            self.keys.resize(allocated, TKey::new())
        }
    }

    pub fn matching(&self, key: TKey) -> EntityIter<TKey> {
        EntityIter {
            keys: &self.keys,
            generations: &self.generations,
            key: key,
            i: 0
        }
    }
}

pub struct EntitiesTransaction<'a, TKey: 'a + EntityKey> {
    entities: &'a Entities<TKey>,
    created: Vec<(Entity, TKey)>,
    deleted: Vec<Index>
}

pub struct EntityChangeSet<TKey: EntityKey> {
    pub created: Vec<(Entity, TKey)>,
    pub deleted: Vec<Index>
}

pub struct TransactionEntitiesIter<'a, TKey: 'a + EntityKey> {
    source: EntityIter<'a, TKey>,
    created: Iter<'a, (Entity, TKey)>
}

impl<'a, TKey: 'a + EntityKey> Iterator for TransactionEntitiesIter<'a, TKey> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let next = self.source.next();
        if let Some(e) = next {
            return Some(e);
        }

        loop {
            let newly_created = self.created.next();
            if newly_created == None {
                return None
            }

            if let Some(&(entity, ref key)) = newly_created {
                if key.matches(&self.source.key) {
                    return Some(entity)
                }
            }
        }
    }
}

impl<'a, TKey: EntityKey> EntitiesTransaction<'a, TKey> {
    pub fn create(&mut self) -> Entity {
        let entity = self.entities.allocate();
        self.created.push((entity, TKey::new()));
        entity
    }

    pub fn destroy(&mut self, entity: Entity) {
        self.deleted.push(entity.index());
    }

    pub fn is_alive(&self, entity: &Entity) -> bool {
        self.entities.is_alive(entity)
    }

    pub fn mark_entity(&mut self, entity: &Entity, resource: usize) {
        if !self.entities.mark_entity(entity, resource) {
            match self.find_created_entity_key(entity) {
                Some(key) => key.register(resource),
                None => panic!("Cannot find entity {}", entity)
            }
        }
    }

    pub fn unmark_entity(&mut self, entity: &Entity, resource: usize) {
        if !self.entities.unmark_entity(entity, resource) {
            match self.find_created_entity_key(entity) {
                Some(key) => key.unregister(resource),
                None => panic!("Cannot find entity {}", entity)
            }
        }
    }

    fn find_created_entity_key(&self, entity: &Entity) -> Option<&TKey> {
        for &(e, ref key) in self.created.iter() {
            if e == *entity {
                return Some(&key)
            }
        }

        return None;
    }

    pub fn matching<'b>(&'b self, key: TKey) -> TransactionEntitiesIter<TKey> {
        TransactionEntitiesIter::<'b, TKey> {
            source: self.entities.matching(key),
            created: self.created.iter()
        }
    }

    pub fn to_change_set(self) -> EntityChangeSet<TKey> {
        EntityChangeSet {
            created: self.created,
            deleted: self.deleted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use scoped_pool::Pool;

    #[test]
    fn entity_deconstruct() {
        let entity = Entity::new(5, 10);
        assert!(entity.index() == 5);
        assert!(entity.generation() == 10);
    }

    #[test]
    fn key64_registered() {
        let key = Key64::new();
        key.register(2);
        assert!(key.is_registered(2))
    }

    #[test]
    fn key64_not_registered() {
        let key = Key64::new();
        key.register(2);
        assert!(!key.is_registered(3))
    }

    #[test]
    fn key64_matches_exact() {
        let a = Key64::new();
        a.register(1);
        a.register(5);
        a.register(20);

        let b = Key64::new();
        b.register(1);
        b.register(5);
        b.register(20);

        assert!(a.matches(&b));
    }

    #[test]
    fn key64_matches_subset() {
        let a = Key64::new();
        a.register(1);
        a.register(5);
        a.register(20);
        a.register(15);

        let b = Key64::new();
        b.register(1);
        b.register(5);
        b.register(20);

        assert!(a.matches(&b));
    }

    #[test]
    fn key64_doesnt_match_superset() {
        let a = Key64::new();
        a.register(1);
        a.register(5);
        a.register(20);

        let b = Key64::new();
        b.register(1);
        b.register(5);
        b.register(20);
        b.register(15);

        assert!(!a.matches(&b));
    }

    #[test]
    fn key64_doesnt_match_disjoint() {
        let a = Key64::new();
        a.register(1);
        a.register(5);
        a.register(20);

        let b = Key64::new();
        b.register(2);
        b.register(6);
        b.register(21);
        b.register(15);

        assert!(!a.matches(&b));
    }

    #[test]
    fn key64_doesnt_match_overlap() {
        let a = Key64::new();
        a.register(1);
        a.register(5);
        a.register(20);

        let b = Key64::new();
        b.register(2);
        b.register(5);
        b.register(20);
        b.register(15);

        assert!(!a.matches(&b));
    }

    #[test]
    fn entities_new() {
        Entities::<Key64>::new();
    }

    #[test]
    fn entities_allocate() {
        let em = Entities::<Key64>::new();
        em.allocate();
    }

    #[test]
    fn entities_allocated_entity_is_alive() {
        let em = Entities::<Key64>::new();
        let entity = em.allocate();
        assert!(em.is_alive(&entity));
    }

    #[test]
    fn entities_allocate_many_no_duplicates() {
        let em = Entities::<Key64>::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }
    }

    #[test]
    fn entities_allocate_many_no_duplicates_comitted() {
        let mut em = Entities::<Key64>::new();
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
    fn entities_mark_allocated() {
        let mut em = Entities::<Key64>::new();
        let entity = em.allocate();

        em.commit_allocations();

        assert!(em.mark_entity(&entity, 5) == true);
    }

    #[test]
    fn entities_unmark_invalid() {
        let mut em = Entities::<Key64>::new();
        let entity = Entity::new(10, 2);

        em.commit_allocations();

        assert!(em.unmark_entity(&entity, 5) == false);
    }

    #[test]
    fn entities_mark_invalid() {
        let mut em = Entities::<Key64>::new();
        let entity = Entity::new(10, 2);

        em.commit_allocations();

        assert!(em.mark_entity(&entity, 5) == false);
    }

    #[test]
    fn entities_unmark_allocated() {
        let mut em = Entities::<Key64>::new();
        let entity = em.allocate();

        em.commit_allocations();

        assert!(em.unmark_entity(&entity, 5) == true);
    }

    #[test]
    fn entities_matches() {
        let mut em = Entities::<Key64>::new();
        let mut valid_entities: HashSet<Entity> = HashSet::new();
        let mut invalid_entities: HashSet<Entity> = HashSet::new();

        for _ in 0..100 {
            let e = em.allocate();
            valid_entities.insert(e);
        }

        for _ in 0..100 {
            let e = em.allocate();
            invalid_entities.insert(e);
        }

        em.commit_allocations();

        for e in valid_entities.iter() {
            em.mark_entity(e, 3);
        }

        let key = Key64::new();
        key.register(3);

        let matching: Vec<Entity> = em.matching(key).collect();

        assert!(matching.len() == valid_entities.len());
        for e in valid_entities {
            assert!(matching.contains(&e));
        }
    }

    #[test]
    fn entitiestx_new() {
        let em = Entities::<Key64>::new();
        em.transaction();
    }

    #[test]
    fn entitiestx_create() {
        let em = Entities::<Key64>::new();
        let mut tx = em.transaction();

        tx.create();
    }

    #[test]
    fn entitiestx_created_is_alive() {
        let em = Entities::<Key64>::new();
        let mut tx = em.transaction();

        let entity = tx.create();
        assert!(tx.is_alive(&entity));
    }

    #[test]
    fn entitiestx_merge_allocates() {
        let mut em = Entities::<Key64>::new();

        let entity: Entity;
        let cs: EntityChangeSet<Key64>;

        {
            let mut tx = em.transaction();
            entity = tx.create();
            cs = tx.to_change_set();
        }

        em.commit_allocations();
        em.merge(cs);

        assert!(em.is_alive(&entity));
    }

    #[test]
    fn entitiestx_merge_deletes() {
        let mut em = Entities::<Key64>::new();
        let mut entities: HashSet<Entity> = HashSet::new();

        for _ in 0..10000 {
            let e = em.allocate();
            assert!(!entities.contains(&e));

            entities.insert(e);
        }

        em.commit_allocations();

        let cs: EntityChangeSet<Key64>;

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

        em.commit_allocations();
        em.merge(cs);

        for entity in entities {
            assert!(!em.is_alive(&entity));
        }
    }

    #[test]
    fn entitiestx_mt_create() {
        let pool = Pool::new(12);
        let mut em = Entities::<Key64>::new();

        let changes: Vec<EntityChangeSet<Key64>>;

        {
            let mut transactions: Vec<EntitiesTransaction<Key64>> = (0..50).map(|_| em.transaction()).collect();

            pool.scoped(|scope| {
                for tx in transactions.iter_mut() {
                    scope.execute(move || {
                        for _ in 0..1000 {
                            tx.create();
                        }
                    });
                }
            });

            changes = transactions.into_iter().map(|tx| tx.to_change_set()).collect();
        }

        em.commit_allocations();

        for change in changes {
            em.merge(change);
        }

        assert!(em.count() == 50000);
    }

    #[test]
    fn entitiestx_created_mark() {
        let em = Entities::<Key64>::new();
        let mut tx = em.transaction();

        let entity = tx.create();
        assert!(tx.is_alive(&entity));

        tx.mark_entity(&entity, 5);
    }

    #[test]
    fn entitiestx_created_unmark() {
        let em = Entities::<Key64>::new();
        let mut tx = em.transaction();

        let entity = tx.create();
        assert!(tx.is_alive(&entity));

        tx.unmark_entity(&entity, 5);
    }

    #[test]
    fn entitiestx_matches() {
        let mut em = Entities::<Key64>::new();
        let mut valid_entities: HashSet<Entity> = HashSet::new();

        for _ in 0..100 {
            let e = em.allocate();
            valid_entities.insert(e);
        }

        for _ in 0..100 {
            em.allocate();
        }

        em.commit_allocations();

        for e in valid_entities.iter() {
            em.mark_entity(e, 3);
        }

        let changes: EntityChangeSet<Key64>;

        {
            let mut tx = em.transaction();

            for _ in 0..100 {
                let e = tx.create();
                tx.mark_entity(&e, 3);
                valid_entities.insert(e);
            }

            for _ in 0..100 {
                tx.create();
            }

            changes = tx.to_change_set();
        }

        em.commit_allocations();
        em.merge(changes);

        let key = Key64::new();
        key.register(3);

        let matching: Vec<Entity> = em.matching(key).collect();

        assert!(matching.len() == valid_entities.len());
        for e in valid_entities {
            assert!(matching.contains(&e));
        }
    }
}
