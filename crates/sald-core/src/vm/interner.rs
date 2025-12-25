use parking_lot::Mutex;
use rustc_hash::FxHashSet;
use std::sync::{Arc, OnceLock};

pub struct Interner {
    pool: Mutex<FxHashSet<Arc<str>>>,
}

impl Interner {
    pub fn new() -> Self {
        Self {
            pool: Mutex::new(FxHashSet::default()),
        }
    }

    pub fn global() -> &'static Self {
        static INTERNER: OnceLock<Interner> = OnceLock::new();
        INTERNER.get_or_init(Self::new)
    }

    pub fn intern(&self, s: &str) -> Arc<str> {
        let mut pool = self.pool.lock();
        if let Some(interned) = pool.get(s) {
            return interned.clone();
        }

        let interned: Arc<str> = Arc::from(s);
        pool.insert(interned.clone());
        interned
    }
}

pub fn intern(s: &str) -> Arc<str> {
    Interner::global().intern(s)
}
