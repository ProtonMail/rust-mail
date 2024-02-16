use std::ops::{Deref, DerefMut};
use tokio::sync;

pub struct RWLock<T: ?Sized>(sync::RwLock<T>);
impl<T: ?Sized> RWLock<T> {
    pub fn new(v: T) -> Self
    where
        T: Sized,
    {
        Self(sync::RwLock::new(v))
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard(self.0.write().await)
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard(self.0.read().await)
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized>(sync::RwLockWriteGuard<'a, T>);

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized>(sync::RwLockReadGuard<'a, T>);

impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

pub struct Mutex<T: ?Sized>(sync::Mutex<T>);

impl<T: ?Sized> Mutex<T> {
    pub fn new(v: T) -> Self
    where
        T: Sized,
    {
        Self(sync::Mutex::new(v))
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        MutexGuard(self.0.lock().await)
    }
}

pub struct MutexGuard<'a, T: ?Sized>(sync::MutexGuard<'a, T>);

impl<'a, T: ?Sized> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<'a, T: ?Sized> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}
