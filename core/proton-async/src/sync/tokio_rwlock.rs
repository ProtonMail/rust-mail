#![allow(clippy::explicit_auto_deref, clippy::explicit_deref_methods)]
use std::ops::{Deref, DerefMut};
use tokio::sync;

pub struct RwLock<T: ?Sized>(sync::RwLock<T>);
impl<T> RwLock<T> {
    pub fn new(v: T) -> Self {
        Self(sync::RwLock::new(v))
    }
}

impl<T: ?Sized> RwLock<T> {
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard(self.0.write().await)
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard(self.0.read().await)
    }
}

pub struct RwLockWriteGuard<'a, T: ?Sized>(sync::RwLockWriteGuard<'a, T>);

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized>(sync::RwLockReadGuard<'a, T>);

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

pub struct Mutex<T: ?Sized>(sync::Mutex<T>);

impl<T> Mutex<T> {
    pub fn new(v: T) -> Self {
        Self(sync::Mutex::new(v))
    }
}

impl<T: ?Sized> Mutex<T> {
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        MutexGuard(self.0.lock().await)
    }
}

pub struct MutexGuard<'a, T: ?Sized>(sync::MutexGuard<'a, T>);

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}
