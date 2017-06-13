use std::marker::PhantomData;
use std::iter::Iterator;

use hibitset::{BitSetLike, BitIter};

use entities::*;
use resource::*;
use join::*;
use world::*;

/// Stores data needed to iterate through entity data in a 
/// set of resources.
pub struct ResourceIterBuilder<I, R, W>
{
    iter: I,
    read_resources: R,
    write_resources: W
}

/// A set of entity resources that can be iterated through.  
/// This is implemented for tuples of entity resource references.
pub trait ResourceIter {
    /// The type of entity index iterator.
    type Iter;

    /// A tuple type containing immutable references to resources.
    type Read;

    /// A tuple type containing mutable references to resources.
    type Write;

    /// Begins construction of an entity resource iterator.
    fn iter(self) -> ResourceIterBuilder<Self::Iter, Self::Read, Self::Write>;
}

/// An iterator of entity IDs for entities
/// whom have data stored in a given set of resources.
pub struct EntityIter<'b, I>
    where I: 'b
{
    index_iter: I,
    entities: &'b Entities,
    phantom: PhantomData<&'b I>
}

impl<'b, I> Iterator for EntityIter<'b, I>
    where I: Iterator<Item=Index> + 'b
{
    type Item = Entity;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.index_iter.next().map(|i| self.entities.by_index(i))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.index_iter.size_hint()
    }
}

/// An iterator of entity components for each
/// entity with data stored in a given set of resources.
pub struct ComponentIter<'b, I, R, W>
    where I: 'b
{
    index_iter: I,
    read_resource_apis: R,
    write_resource_apis: W,
    phantom: PhantomData<&'b I>
}

macro_rules! impl_resource_iterators {
    ([$($read:ident),*] [$($write:ident),*]) => {
        impl <'a, $($read,)* $($write,)*> ResourceIter for ($(&'a $read,)* $(&'a mut $write,)*)
            where $($read: EntityResource + 'a,)*
                  $($write: EntityResource + 'a,)*
        {
            type Iter = BitIter<<($(&'a $read::Filter,)* $(&'a $write::Filter,)*) as BitAnd>::Value>;
            type Read = ($(&'a $read::Api,)*);
            type Write = ($(&'a mut $write::Api,)*);

            #[allow(non_snake_case)]
            fn iter(self) -> ResourceIterBuilder<Self::Iter, Self::Read, Self::Write>
            {
                let ($($read,)* $($write,)*) = self;
                $(let $read = $read.pin();)*
                $(let $write = $write.pin_mut();)*

                let iter = ($($read.0,)* $($write.0,)*).and().iter();

                return ResourceIterBuilder {
                    iter: iter,
                    read_resources: ($($read.1,)*),
                    write_resources: ($($write.1,)*)
                };
            }
        }

        impl <'a, I, $($read,)* $($write,)*> ResourceIterBuilder<I, ($(&'a $read,)*), ($(&'a mut $write,)*)>
            where I: Iterator<Item=Index> + 'a,
                  $($read: 'a,)*
                  $($write: 'a,)*
        {
            /// Produces an iterator for iterating through all entity IDs for entities with data
            /// stored in all given resources.
            #[allow(non_snake_case)]
            pub fn entities<C: Send + Sync>(self, ctx: &'a SystemContext<'a, C>) -> (EntityIter<'a, I>, $(&'a $read,)* $(&'a mut $write,)*)
            {
                let iter = self.iter;
                let ($($read,)*) = self.read_resources;
                let ($($write,)*) = self.write_resources;

                return (
                    EntityIter {
                        index_iter: iter,
                        entities: ctx.entities,
                        phantom: PhantomData
                    },
                    $($read,)*
                    $($write,)*
                );
            }
        }       

        impl <'a, I, $($read,)* $($write,)*> ResourceIterBuilder<I, ($(&'a $read,)*), ($(&'a mut $write,)*)>
            where I: Iterator<Item=Index> + 'a,
                  $($read: ComponentResourceApi + 'a,)*
                  $($write: ComponentResourceApi + 'a,)*
        {
            /// Iterate through all entity components for entities with data
            /// stored in all given resources.
            #[allow(non_snake_case)]
            pub fn components(self) -> ComponentIter<'a, I, ($(*const $read,)*), ($(*mut $write,)*)>
            {
                let ($($read,)*) = self.read_resources;
                let ($($write,)*) = self.write_resources;

                return ComponentIter {
                    index_iter: self.iter,
                    read_resource_apis: ($($read as *const $read,)*),
                    write_resource_apis: ($($write as *mut $write,)*),
                    phantom: PhantomData
                };
            }
        }

        impl<'b, I, $($read,)* $($write,)*> Iterator for ComponentIter<'b, I, ($(*const $read,)*), ($(*mut $write,)*)>
            where I: Iterator<Item=Index> + 'b,
                $($read: ComponentResourceApi + 'b,)*
                $($write: ComponentResourceApi + 'b,)*
        {
            type Item = (Index, $(&'b $read::Component,)* $(&'b mut $write::Component,)*);

            #[inline]
            #[allow(non_snake_case)]
            fn next(&mut self) -> Option<Self::Item> {                
                self.index_iter.next().map(|i| {
                    let ($($read,)*) = self.read_resource_apis;
                    let ($($write,)*) = self.write_resource_apis;
                    unsafe { (i, $((*$read).get_unchecked(i),)* $((*$write).get_unchecked_mut(i),)*) }
                })
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.index_iter.size_hint()
            }
        }
    }
}

impl_resource_iterators!([R0] []);
impl_resource_iterators!([R0, R1] []);
impl_resource_iterators!([R0, R1, R2] []);
impl_resource_iterators!([R0, R1, R2, R3] []);
impl_resource_iterators!([R0, R1, R2, R3, R4] []);
impl_resource_iterators!([] [W0]);
impl_resource_iterators!([R0] [W0]);
impl_resource_iterators!([R0, R1] [W0]);
impl_resource_iterators!([R0, R1, R2] [W0]);
impl_resource_iterators!([R0, R1, R2, R3] [W0]);
impl_resource_iterators!([R0, R1, R2, R3, R4] [W0]);
impl_resource_iterators!([] [W0, W1]);
impl_resource_iterators!([R0] [W0, W1]);
impl_resource_iterators!([R0, R1] [W0, W1]);
impl_resource_iterators!([R0, R1, R2] [W0, W1]);
impl_resource_iterators!([R0, R1, R2, R3] [W0, W1]);
impl_resource_iterators!([R0, R1, R2, R3, R4] [W0, W1]);
impl_resource_iterators!([] [W0, W1, W2]);
impl_resource_iterators!([R0] [W0, W1, W2]);
impl_resource_iterators!([R0, R1] [W0, W1, W2]);
impl_resource_iterators!([R0, R1, R2] [W0, W1, W2]);
impl_resource_iterators!([R0, R1, R2, R3] [W0, W1, W2]);
impl_resource_iterators!([R0, R1, R2, R3, R4] [W0, W1, W2]);
impl_resource_iterators!([] [W0, W1, W2, W3]);
impl_resource_iterators!([R0] [W0, W1, W2, W3]);
impl_resource_iterators!([R0, R1] [W0, W1, W2, W3]);
impl_resource_iterators!([R0, R1, R2] [W0, W1, W2, W3]);
impl_resource_iterators!([R0, R1, R2, R3] [W0, W1, W2, W3]);
impl_resource_iterators!([R0, R1, R2, R3, R4] [W0, W1, W2, W3]);
impl_resource_iterators!([] [W0, W1, W2, W3, W4]);
impl_resource_iterators!([R0] [W0, W1, W2, W3, W4]);
impl_resource_iterators!([R0, R1] [W0, W1, W2, W3, W4]);
impl_resource_iterators!([R0, R1, R2] [W0, W1, W2, W3, W4]);
impl_resource_iterators!([R0, R1, R2, R3] [W0, W1, W2, W3, W4]);
impl_resource_iterators!([R0, R1, R2, R3, R4] [W0, W1, W2, W3, W4]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_tuple_entities() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());
        world.register_resource(VecResource::<i32>::new());

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r1w1(|ctx, a: &VecResource<u32>, b: &mut VecResource<i32>| {
                {
                    let (iter, r, w) = (a, &mut *b).iter().entities(ctx);
                    for entity in iter {
                        let x = r.get(entity).unwrap();
                        let y = w.get_mut(entity).unwrap();
                        println!("x={}, y={}", x, y);
                    }
                }

                b.get(Entity::new(0, 0));
            });
        });
    }

    #[test]
    fn entities_tuple_components() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());
        world.register_resource(VecResource::<i32>::new());

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r1w1(|_, a: &VecResource<u32>, b: &mut VecResource<i32>| {
                for (i, x, y) in (a, &mut *b).iter().components() {
                    println!("{} has x={}, y={}", i, x, y);
                }

                b.get(Entity::new(0, 0));
            });
        });
    }

    #[test]
    fn entities_tuple_entities_macro() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());
        world.register_resource(VecResource::<i32>::new());
        world.register_resource(VecResource::<u64>::new());
        world.register_resource(VecResource::<i64>::new());

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r2w2(|ctx, a: &VecResource<u32>, b: &VecResource<i32>, c: &mut VecResource<u64>, d: &mut VecResource<i64>| {
                {
                    let (iter, r, _, w, _) = (a, b, &mut *c, &mut *d).iter().entities(ctx);
                    for entity in iter {
                        let x = r.get(entity).unwrap();
                        let y = w.get_mut(entity).unwrap();
                        println!("x={}, y={}", x, y);
                    }
                }

                b.get(Entity::new(0, 0));
            });
        });
    }

    #[test]
    fn entities_tuple_components_macro() {
        let mut world = World::new();
        world.register_resource(VecResource::<u32>::new());
        world.register_resource(VecResource::<i32>::new());
        world.register_resource(VecResource::<u64>::new());
        world.register_resource(VecResource::<i64>::new());

        let mut buffer = SystemCommandBuffer::default();
        buffer.queue_systems(|scope| {
            scope.run_r2w2(|_, a: &VecResource<u32>, b: &VecResource<i32>, c: &mut VecResource<u64>, d: &mut VecResource<i64>| {
                for (i, x, y, z, w) in (a, b, &mut *c, &mut *d).iter().components() {
                    println!("{} has x={}, y={} z={} w={}", i, x, y, z, w);
                }

                b.get(Entity::new(0, 0));
            });
        });
    }
}