//! Verrous qui survivent à un panic : un thread qui plante en tenant un `Mutex` ne doit pas faire planter tous les suivants.

use std::sync::{Mutex, MutexGuard};

pub trait MutexExt<T> {
    /// Comme `lock().unwrap()`, mais récupère le contenu d'un verrou « empoisonné » au lieu de paniquer.
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_safe_survives_a_poisoned_mutex() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("boom");
        })
        .join();
        assert!(m.lock().is_err());
        assert_eq!(*m.lock_safe(), 1);
    }
}
