//direct rip off of rustc

use dashmap::DashMap;
use bump_scope::BumpPool;
use std::cmp::Ordering;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ptr;


mod private {
    #[derive(Clone, Copy, Debug)]
    pub struct PrivateZst;
}

/// A reference to a value that is interned, and is known to be unique.
///
/// Note that it is possible to have a `T` and a `Interned<T>` that are (or
/// refer to) equal but different values. But if you have two different
/// `Interned<T>`s, they both refer to the same value, at a single location in
/// memory. This means that equality and hashing can be done on the value's
/// address rather than the value's contents, which can improve performance.
///
/// The `PrivateZst` field means you can pattern match with `Interned(v, _)`
/// but you can only construct a `Interned` with `new_unchecked`, and not
/// directly.
pub struct Interned<'a, T>(pub &'a T, pub private::PrivateZst);


impl<'a, T> Interned<'a, T> {
    /// Create a new `Interned` value. The value referred to *must* be interned
    /// and thus be unique, and it *must* remain unique in the future. This
    /// function has `_unchecked` in the name but is not `unsafe`, because if
    /// the uniqueness condition is violated condition it will cause incorrect
    /// behaviour but will not affect memory safety.
    #[inline]
    pub const fn new_unchecked(t: &'a T) -> Self {
        Interned(t, private::PrivateZst)
    }
}

impl<'a, T> Clone for Interned<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for Interned<'a, T> {}

impl<'a, T> Deref for Interned<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a, T> PartialEq for Interned<'a, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Pointer equality implies equality, due to the uniqueness constraint.
        ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for Interned<'a, T> {}

impl<'a, T: PartialOrd> PartialOrd for Interned<'a, T> {
    fn partial_cmp(&self, other: &Interned<'a, T>) -> Option<Ordering> {
        // Pointer equality implies equality, due to the uniqueness constraint,
        // but the contents must be compared otherwise.
        if ptr::eq(self.0, other.0) {
            Some(Ordering::Equal)
        } else {
            let res = self.0.partial_cmp(other.0);
            debug_assert_ne!(res, Some(Ordering::Equal));
            res
        }
    }
}

impl<'a, T: Ord> Ord for Interned<'a, T> {
    fn cmp(&self, other: &Interned<'a, T>) -> Ordering {
        // Pointer equality implies equality, due to the uniqueness constraint,
        // but the contents must be compared otherwise.
        if ptr::eq(self.0, other.0) {
            Ordering::Equal
        } else {
            let res = self.0.cmp(other.0);
            debug_assert_ne!(res, Ordering::Equal);
            res
        }
    }
}

impl<'a, T> Hash for Interned<'a, T>
where
    T: Hash,
{
    #[inline]
    fn hash<H: Hasher>(&self, s: &mut H) {
        // Pointer hashing is sufficient, due to the uniqueness constraint.
        ptr::hash(self.0, s)
    }
}

impl<T: Debug> Debug for Interned<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

//rustc way to do lists is some weird hacky VLA things which i dont wana do
//so instead we make this type 

pub struct InternedSlice<'a, T>(pub &'a [T], pub private::PrivateZst);


impl<'a, T> InternedSlice<'a, T> {
    #[inline]
    pub const fn new_unchecked(t: &'a [T]) -> Self {
        Self(t, private::PrivateZst)
    }
}

impl<'a, T> Clone for InternedSlice<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for InternedSlice<'a, T> {}

impl<'a, T> Deref for InternedSlice<'a, T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        self.0
    }
}

impl<'a, T> PartialEq for InternedSlice<'a, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Pointer equality implies equality, due to the uniqueness constraint.
        ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for InternedSlice<'a, T> {}

impl<'a, T: PartialOrd> PartialOrd for InternedSlice<'a, T> {
    fn partial_cmp(&self, other: &InternedSlice<'a, T>) -> Option<Ordering> {
        if self==other {
            Some(Ordering::Equal)
        } else {
            let res = self.0.partial_cmp(other.0);
            debug_assert_ne!(res, Some(Ordering::Equal));
            res
        }
    }
}

impl<'a, T: Ord> Ord for InternedSlice<'a, T> {
    fn cmp(&self, other: &InternedSlice<'a, T>) -> Ordering {
        if self==other {
            Ordering::Equal
        } else {
            let res = self.0.cmp(other.0);
            debug_assert_ne!(res, Ordering::Equal);
            res
        }
    }
}

impl<'a, T> Hash for InternedSlice<'a, T>
where
    T: Hash,
{
    #[inline]
    fn hash<H: Hasher>(&self, s: &mut H) {
        // Pointer hashing is sufficient, due to the uniqueness constraint.
        ptr::hash(self.0, s)
    }
}

impl<T: Debug> Debug for InternedSlice<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug,Clone,Copy,PartialEq,PartialOrd,Ord,Eq,Hash)]
pub struct ThinSlice<'a,T>(pub Interned<'a,InternedSlice<'a,T>>);

impl<'a, T> ThinSlice<'a, T> {
    #[inline]
    pub const fn new_unchecked(t: &'a InternedSlice<'a,T>) -> Self {
        Self(Interned::new_unchecked(t))
    }
}


impl<'a, T> Deref for ThinSlice<'a, T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        &self.0
    }
}

//my own stuff

pub struct TypeArena<'a> {
	pub types:DashMap<Type<'a>,Interned<'a,Type<'a>>>,
	pub thin_slices:DashMap<&'a [Type<'a>],ThinSlice<'a,Type<'a>>>,
	pub pool:&'a BumpPool
}

impl<'a> TypeArena<'a>{
	pub fn new(pool:&'a BumpPool)->Self{
		Self{
			pool,
			types:DashMap::new(),
			thin_slices:DashMap::new(),
		}
	}
	pub fn get_type(&self,t:Type<'a>)->Interned<'a,Type<'a>>{
		*self.types.entry(t).or_insert_with(||{
			let r:&'a Type<'a> = self.pool.get().alloc(t).into_ref();
			Interned::new_unchecked(r)
		})
	}

	pub fn get_slice(&self,t:&[Type<'a>])->InternedSlice<'a,Type<'a>>{
		*self.get_thin_slice(t).0
	}

	pub fn get_thin_slice(&self,t:&[Type<'a>])->ThinSlice<'a,Type<'a>>{
		//fast path first
		if let Some(ans) = self.thin_slices.get(t){
			return *ans;
		}

		//now we need to try allocating and then droping if someone else raced us
		let bump = self.pool.get();
		let checkpoint = bump.checkpoint();
		let alive = bump.alloc_slice_clone(t).into_ref();
		match self.thin_slices.entry(alive) {
		    dashmap::Entry::Occupied(o)=>{
		    	//we didnt actually need it
		    	//also allocator is owned by this thread so no other allocs
		    	unsafe{bump.reset_to(checkpoint);}
		    	*o.get()
		    }
		    dashmap::Entry::Vacant(v) => {
		    	let fat = InternedSlice::new_unchecked(alive);
		    	let thin = ThinSlice::new_unchecked(bump.alloc(fat).into_ref());
		    	*v.insert(thin)
		    },
		}
	}
}

#[test]
fn test_type_lifetimes(){
	let pool = BumpPool::new();
	let arena = TypeArena::new(&pool);

	let s1 = arena.get_slice(&[Type::Void]);
	let s2 = arena.get_slice(&[Type::Tuple(s1)]);
	let s3 = arena.get_slice(&[Type::Tuple(s1)]);
	let _ = arena.get_slice(&[Type::Tuple(s2)]);

	assert_eq!(s2,s3);
}

#[derive(Debug,Copy,Clone,PartialEq,Eq,Hash)]
pub struct VarID(pub u32);

#[derive(Debug,Copy,Clone,PartialEq,Eq,Hash)]
pub struct GVarID(pub u32);

#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
pub enum Type<'a> {
	Function{
		input:ThinSlice<'a,Type<'a>>,
		output:Interned<'a,Type<'a>>
	},

	Tuple(InternedSlice<'a,Type<'a>>),
	Pointer(Interned<'a,Type<'a>>),
	Int(IntType),
	String,
	Void,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(
        size_of::<Type>() == 24 ,
        "size should make sense"
    );
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntSign {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntSize {
    I1,I8, I16, I32, I64, I128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntType {
    pub sign: IntSign,
    pub size: IntSize,
}


const _: () = {
    assert!(
        size_of::<IntType>() == 2,
        "anoying 2 bytes"
    );
};
