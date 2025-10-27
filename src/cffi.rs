use std::hash::Hasher;
use std::hash::Hash;
use std::fmt;
use std::slice;
use std::ops::Deref;
use std::rc::Rc;

#[repr(C)]
pub struct SliceRC<T>{
	ptr:*const T,
	len:usize,
}

impl<T> SliceRC<T>{
	pub fn new(rc:Rc<[T]>)->Self{
		rc.into()
	}
}

impl<T> Deref for SliceRC<T>{

type Target = [T];
fn deref(&self) -> &[T] {unsafe{slice::from_raw_parts(self.ptr,self.len)}}
}

impl<T> Drop for SliceRC<T>{
fn drop(&mut self) {
	let slice = std::ptr::slice_from_raw_parts(self.ptr,self.len);
	let _base :Rc<[T]> = unsafe{Rc::from_raw(slice)};
}
}

impl<T> From<Rc<[T]>> for SliceRC<T>{
fn from(rc: Rc<[T]>) -> Self {
	let t = Rc::into_raw(rc);
	Self{
		ptr:t as *const T,len:t.len()
	}
}
}


impl<T> From<SliceRC<T>> for Rc<[T]>{
fn from(src: SliceRC<T>) -> Self {
	let slice = std::ptr::slice_from_raw_parts(src.ptr,src.len);
	let ans = unsafe{Rc::from_raw(slice)};
	std::mem::forget(src);
	ans
}
}

impl<T> Clone for SliceRC<T>{
fn clone(&self) -> Self {
	let slice = std::ptr::slice_from_raw_parts(self.ptr,self.len);
	let base :Rc<[T]> = unsafe{Rc::from_raw(slice)};
	let ans = Self::new(base.clone());
	std::mem::forget(base);
	ans
}
}

impl<T: fmt::Debug> fmt::Debug for SliceRC<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: PartialEq> PartialEq for SliceRC<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}
impl<T: Eq> Eq for SliceRC<T> {}

impl<T: Hash> Hash for SliceRC<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state)
    }
}

#[repr(C)]
pub struct RC<T> {
    ptr: *const T,
}

impl<T> RC<T> {
    #[inline]
    pub fn new(rc: Rc<T>) -> Self {
        rc.into()
    }
}

impl<T> Deref for RC<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T> Drop for RC<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let _ = Rc::from_raw(self.ptr);
        }
    }
}

impl<T> From<Rc<T>> for RC<T> {
    #[inline]
    fn from(rc: Rc<T>) -> Self {
        let raw = Rc::into_raw(rc);
        Self { ptr: raw }
    }
}

impl<T> From<RC<T>> for Rc<T> {
    #[inline]
    fn from(rc_: RC<T>) -> Self {
        let rc = unsafe { Rc::from_raw(rc_.ptr) };
        std::mem::forget(rc_);
        rc
    }
}

impl<T> Clone for RC<T> {
    #[inline]
    fn clone(&self) -> Self {
        let base: Rc<T> = unsafe { Rc::from_raw(self.ptr) };
        let ans = Self::new(base.clone());
        std::mem::forget(base);
        ans
    }
}

impl<T: fmt::Debug> fmt::Debug for RC<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: PartialEq> PartialEq for RC<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}
impl<T: Eq> Eq for RC<T> {}

impl<T: Hash> Hash for RC<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state)
    }
}

#[repr(C)]
pub struct StrRC {
    ptr: *const u8,
    len: usize,
}

impl StrRC {
    #[inline]
    pub fn new(rc: Rc<str>) -> Self {
        rc.into()
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: must always be constructed from valid UTF-8 Rc<str>
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.ptr, self.len)) }
    }

}

impl Deref for StrRC {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Drop for StrRC {
    #[inline]
    fn drop(&mut self) {
        let slice = std::ptr::slice_from_raw_parts(self.ptr, self.len);
        let _base: Rc<str> = unsafe { Rc::from_raw(slice as *const str) };
    }
}

impl From<Rc<str>> for StrRC {
    #[inline]
    fn from(rc: Rc<str>) -> Self {
        let len = rc.len();
        let raw = Rc::into_raw(rc);
        Self {
            ptr: raw as *const u8,
            len,
        }
    }
}

impl From<Box<str>> for StrRC {
fn from(s: Box<str>) -> Self {Self::new(Rc::from(s))}
}

impl From<String> for StrRC {
fn from(s: String) -> Self {s.into_boxed_str().into()}
}

impl From<StrRC> for Rc<str> {
    #[inline]
    fn from(src: StrRC) -> Self {
        let slice = std::ptr::slice_from_raw_parts(src.ptr, src.len);
        let rc = unsafe { Rc::from_raw(slice as *const str) };
        std::mem::forget(src);
        rc
    }
}

impl Clone for StrRC {
    #[inline]
    fn clone(&self) -> Self {
        let slice = std::ptr::slice_from_raw_parts(self.ptr, self.len);
        let base: Rc<str> = unsafe { Rc::from_raw(slice as *const str) };
        let ans = Self::from(base.clone());
        std::mem::forget(base);
        ans
    }
}

impl fmt::Debug for StrRC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl fmt::Display for StrRC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}


impl PartialEq for StrRC {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq(other.as_str())
    }
}
impl Eq for StrRC {}

impl Hash for StrRC {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}
