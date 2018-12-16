use crate::{
	storage::{
		Key,
		cell::TypedCell,
	},
};

use std::cell::{RefCell};

/// A synchronized cell.
///
/// Provides interpreted, read-optimized and inplace-mutable
/// access to the associated constract storage slot.
///
/// # Guarantees
///
/// - `Owned`, `Typed`, `Avoid Reads`, `Mutable`
///
/// Read more about kinds of guarantees and their effect [here](../index.html#guarantees).
#[derive(Debug, PartialEq, Eq)]
pub struct SyncCell<T> {
	/// The underlying typed cell.
	cell: TypedCell<T>,
	/// The cached entity.
	elem: RefCell<Cached<T>>,
}

/// A cached entity.
///
/// This is either in sync with the contract storage or out of sync.
#[derive(Debug, PartialEq, Eq)]
pub enum Cached<T> {
	/// Desync mode.
	Desync,
	/// Synced with synced contract storage slot state.
	Sync(Option<T>),
}

impl<T> Cached<T> {
	/// Returns a `Cached` of an immutable reference to the entity.
	pub fn as_ref(&self) -> Cached<&T> {
		match self {
			Cached::Desync => Cached::Desync,
			Cached::Sync(opt_elem) => Cached::Sync(opt_elem.as_ref()),
		}
	}

	/// Converts `self` to an `Option` of immutable reference.
	///
	/// Returns `None` if it is desync.
	pub fn as_opt(&self) -> Option<&T> {
		match self {
			Cached::Desync => None,
			Cached::Sync(opt_elem) => opt_elem.into(),
		}
	}

	/// Converts `self` to an `Option` of mutable reference.
	///
	/// Returns `None` if it is desync.
	pub fn as_opt_mut(&mut self) -> Option<&mut T> {
		match self {
			Cached::Desync => None,
			Cached::Sync(opt_elem) => opt_elem.into(),
		}
	}
}

impl<T> SyncCell<T> {
	/// Creates a new copy cell for the given key.
	///
	/// # Note
	///
	/// This is unsafe since it does not check if the associated
	/// contract storage does not alias with other accesses.
	pub unsafe fn new_unchecked(key: Key) -> Self {
		Self{
			cell: TypedCell::new_unchecked(key),
			elem: RefCell::new(Cached::Desync)
		}
	}
}

impl<T> SyncCell<T>
where
	T: parity_codec::Decode
{
	/// Returns an immutable reference to the cached entity if any.
	fn cached(&self) -> Cached<&T> {
		let elem_ref = unsafe { &*self.elem.as_ptr() };
		match elem_ref {
			Cached::Desync => Cached::Desync,
			cached @ Cached::Sync(_) => cached.as_ref(),
		}
	}

	/// Returns an immutable reference to the entity if any.
	///
	/// # Note
	///
	/// Avoids unnecesary accesses from the contract storage.
	pub fn get(&self) -> Option<&T> {
		if let Cached::Sync(opt_elem) = self.cached() {
			return opt_elem
		}
		self.load()
	}

	/// Returns an immutable reference to the entity if any.
	///
	/// # Note
	///
	/// Always reads from the contract storage.
	pub fn load(&self) -> Option<&T> {
		self.elem.replace(Cached::Sync(self.cell.load()));
		{
			let cached: &Cached<T> = unsafe {
				&*self.elem.as_ptr()
			};
			cached.as_opt()
		}
	}
}

impl<T> SyncCell<T>
where
	T: parity_codec::Encode
{
	/// Sets the entity to the given entity.
	///
	/// # Note
	///
	/// This always accesses the contract storage.
	pub fn set(&mut self, val: T) {
		self.cell.store(&val);
		self.elem.replace(Cached::Sync(Some(val)));
	}

	/// Mutates the entity by the given mutator.
	///
	/// # Note
	///
	/// Synchronizes contract storage after the mutation.
	pub fn set_with<F>(&mut self, f: F)
	where
		F: FnOnce(&mut T)
	{
		if let Some(elem) = self.elem.get_mut().as_opt_mut() {
			f(elem);
			self.cell.store(&elem);
		}
	}

	/// Removes the entity from the contract storage.
	///
	/// # Note
	///
	/// This also clears the cache and cause a guaranteed
	/// contract storage access upon the next read.
	pub fn clear(&mut self) {
		self.cell.clear();
		self.elem.replace(Cached::Sync(None));
	}
}

#[cfg(all(test, feature = "test-env"))]
mod tests {
	use super::*;

	use crate::env::TestEnv;

	#[test]
	fn simple() {
		let mut cell: SyncCell<i32> = unsafe {
			SyncCell::new_unchecked(Key([0x42; 32]))
		};
		assert_eq!(cell.load(), None);
		cell.set(5);
		assert_eq!(cell.load(), Some(&5));
		cell.mutate_with(|val| *val += 10);
		assert_eq!(cell.load(), Some(&15));
		cell.clear();
		assert_eq!(cell.load(), None);
	}

	#[test]
	fn count_reads() {
		let cell: SyncCell<i32> = unsafe {
			SyncCell::new_unchecked(Key([0x42; 32]))
		};
		assert_eq!(TestEnv::total_reads(), 0);
		cell.get();
		assert_eq!(TestEnv::total_reads(), 1);
		cell.get();
		cell.get();
		assert_eq!(TestEnv::total_reads(), 1);
	}

	#[test]
	fn count_writes() {
		let mut cell: SyncCell<i32> = unsafe {
			SyncCell::new_unchecked(Key([0x42; 32]))
		};
		assert_eq!(TestEnv::total_writes(), 0);
		cell.set(1);
		assert_eq!(TestEnv::total_writes(), 1);
		cell.set(2);
		cell.set(3);
		assert_eq!(TestEnv::total_writes(), 3);
	}
}
