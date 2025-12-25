
















use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};


const INITIAL_THRESHOLD: usize = 10000; 
const HEAP_GROW_FACTOR: f64 = 1.5;

const STEP_BUDGET_US: u64 = 500; 

const OBJECTS_PER_STEP: usize = 100;


#[derive(Debug, Clone, Default)]
pub struct GcStats {
    
    pub total_tracked: usize,
    
    pub cycles_broken: usize,
    
    pub tracked_count: usize,
    
    pub collections: usize,
    
    pub incremental_steps: usize,
}


pub type ObjectId = u64;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    Array,
    Dictionary,
    Instance,
}


#[derive(Debug, Clone)]
#[allow(unused)]
enum GcPhase {
    
    Idle,
    
    Marking {
        
        reachable: FxHashSet<ObjectId>,
        
        iter_keys: Vec<ObjectId>,
        iter_pos: usize,
    },
    
    Sweeping {
        
        to_clear: Vec<ObjectId>,
        iter_pos: usize,
    },
}


#[derive(Clone)]
pub enum TrackedObject {
    Array(Weak<RefCell<Vec<super::Value>>>),
    Dictionary(Weak<RefCell<FxHashMap<String, super::Value>>>),
    Instance(Weak<RefCell<super::Instance>>),
}

impl TrackedObject {
    
    pub fn is_alive(&self) -> bool {
        match self {
            TrackedObject::Array(w) => w.strong_count() > 0,
            TrackedObject::Dictionary(w) => w.strong_count() > 0,
            TrackedObject::Instance(w) => w.strong_count() > 0,
        }
    }

    
    pub fn strong_count(&self) -> usize {
        match self {
            TrackedObject::Array(w) => w.strong_count(),
            TrackedObject::Dictionary(w) => w.strong_count(),
            TrackedObject::Instance(w) => w.strong_count(),
        }
    }

    
    pub fn upgrade_array(&self) -> Option<Rc<RefCell<Vec<super::Value>>>> {
        match self {
            TrackedObject::Array(w) => w.upgrade(),
            _ => None,
        }
    }

    pub fn upgrade_dict(&self) -> Option<Rc<RefCell<FxHashMap<String, super::Value>>>> {
        match self {
            TrackedObject::Dictionary(w) => w.upgrade(),
            _ => None,
        }
    }

    pub fn upgrade_instance(&self) -> Option<Rc<RefCell<super::Instance>>> {
        match self {
            TrackedObject::Instance(w) => w.upgrade(),
            _ => None,
        }
    }

    
    pub fn clear_contents(&self) {
        match self {
            TrackedObject::Array(w) => {
                if let Some(rc) = w.upgrade() {
                    rc.borrow_mut().clear();
                }
            }
            TrackedObject::Dictionary(w) => {
                if let Some(rc) = w.upgrade() {
                    rc.borrow_mut().clear();
                }
            }
            TrackedObject::Instance(w) => {
                if let Some(rc) = w.upgrade() {
                    rc.borrow_mut().fields.clear();
                }
            }
        }
    }
}


pub struct GcHeap {
    
    next_id: u64,
    
    tracked: FxHashMap<ObjectId, TrackedObject>,
    
    threshold: usize,
    
    pub stats: GcStats,
    
    phase: GcPhase,
}

impl GcHeap {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            tracked: FxHashMap::default(),
            threshold: INITIAL_THRESHOLD,
            stats: GcStats::default(),
            phase: GcPhase::Idle,
        }
    }

    
    pub fn should_collect(&self) -> bool {
        match &self.phase {
            GcPhase::Idle => self.tracked.len() > self.threshold,
            _ => true, 
        }
    }

    
    pub fn track_array(&mut self, arr: &Rc<RefCell<Vec<super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Array(Rc::downgrade(arr)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    
    pub fn track_dict(&mut self, dict: &Rc<RefCell<FxHashMap<String, super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Dictionary(Rc::downgrade(dict)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    
    pub fn track_instance(&mut self, inst: &Rc<RefCell<super::Instance>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Instance(Rc::downgrade(inst)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    
    
    
    pub fn collect(&mut self, roots: Vec<&super::Value>) {
        let start = Instant::now();
        let budget = Duration::from_micros(STEP_BUDGET_US);

        
        self.cleanup_dead();

        loop {
            match std::mem::replace(&mut self.phase, GcPhase::Idle) {
                GcPhase::Idle => {
                    
                    let reachable = self.mark_from_roots(&roots);
                    let iter_keys: Vec<ObjectId> = self.tracked.keys().copied().collect();

                    
                    
                    let mut to_clear = Vec::new();
                    for id in &iter_keys {
                        if !reachable.contains(id) {
                            if let Some(obj) = self.tracked.get(id) {
                                if obj.is_alive() {
                                    to_clear.push(*id);
                                }
                            }
                        }
                    }

                    if to_clear.is_empty() {
                        
                        self.finish_collection();
                        return;
                    }

                    self.phase = GcPhase::Sweeping {
                        to_clear,
                        iter_pos: 0,
                    };
                    self.stats.incremental_steps += 1;
                }

                GcPhase::Marking {
                    reachable,
                    iter_keys,
                    iter_pos,
                } => {
                    
                    
                    self.phase = GcPhase::Marking {
                        reachable,
                        iter_keys,
                        iter_pos,
                    };
                    let to_clear = Vec::new();
                    
                    self.phase = GcPhase::Sweeping {
                        to_clear,
                        iter_pos: 0,
                    };
                }

                GcPhase::Sweeping {
                    to_clear,
                    mut iter_pos,
                } => {
                    let mut processed = 0;

                    while iter_pos < to_clear.len() {
                        
                        if processed >= OBJECTS_PER_STEP && start.elapsed() >= budget {
                            
                            self.phase = GcPhase::Sweeping { to_clear, iter_pos };
                            self.stats.incremental_steps += 1;
                            return;
                        }

                        let id = to_clear[iter_pos];
                        if let Some(obj) = self.tracked.get(&id) {
                            obj.clear_contents();
                            self.stats.cycles_broken += 1;
                        }

                        iter_pos += 1;
                        processed += 1;
                    }

                    
                    self.finish_collection();
                    return;
                }
            }

            
            if start.elapsed() >= budget {
                self.stats.incremental_steps += 1;
                return;
            }
        }
    }

    
    fn finish_collection(&mut self) {
        self.stats.collections += 1;
        self.stats.tracked_count = self.tracked.len();
        self.stats.incremental_steps += 1;

        
        let new_threshold = (self.tracked.len() as f64 * HEAP_GROW_FACTOR) as usize;
        self.threshold = new_threshold.max(INITIAL_THRESHOLD);

        self.phase = GcPhase::Idle;
    }

    
    fn cleanup_dead(&mut self) {
        self.tracked.retain(|_, obj| obj.is_alive());
    }

    
    fn mark_from_roots(&self, roots: &[&super::Value]) -> FxHashSet<ObjectId> {
        let mut reachable = FxHashSet::default();
        let mut worklist: Vec<&super::Value> = roots.iter().copied().collect();

        while let Some(value) = worklist.pop() {
            self.mark_value(value, &mut reachable, &mut worklist);
        }

        reachable
    }

    
    fn mark_value<'a>(
        &self,
        value: &'a super::Value,
        reachable: &mut FxHashSet<ObjectId>,
        _worklist: &mut Vec<&'a super::Value>,
    ) {
        use super::Value;

        match value {
            Value::Array(arr) => {
                
                for (id, tracked) in &self.tracked {
                    if let TrackedObject::Array(w) = tracked {
                        if let Some(rc) = w.upgrade() {
                            if Rc::ptr_eq(&rc, arr) {
                                if reachable.insert(*id) {
                                    
                                    
                                }
                                break;
                            }
                        }
                    }
                }
            }
            Value::Dictionary(dict) => {
                for (id, tracked) in &self.tracked {
                    if let TrackedObject::Dictionary(w) = tracked {
                        if let Some(rc) = w.upgrade() {
                            if Rc::ptr_eq(&rc, dict) {
                                reachable.insert(*id);
                                break;
                            }
                        }
                    }
                }
            }
            Value::Instance(inst) => {
                for (id, tracked) in &self.tracked {
                    if let TrackedObject::Instance(w) = tracked {
                        if let Some(rc) = w.upgrade() {
                            if Rc::ptr_eq(&rc, inst) {
                                reachable.insert(*id);
                                break;
                            }
                        }
                    }
                }
            }
            Value::Function(func) => {
                
                for upvalue in &func.upvalues {
                    let uv = upvalue.borrow();
                    if uv.closed.is_some() {
                        
                    }
                }
            }
            Value::BoundMethod { receiver: _, .. } => {
                
            }
            Value::InstanceMethod { receiver: _, .. } => {
                
            }
            Value::Namespace { members: _, .. } => {
                
            }
            _ => {}
        }
    }

    
    pub fn get_stats(&self) -> GcStats {
        self.stats.clone()
    }
}

impl Default for GcHeap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gc_new() {
        let gc = GcHeap::new();
        assert_eq!(gc.stats.total_tracked, 0);
        assert_eq!(gc.stats.collections, 0);
    }

    #[test]
    fn test_gc_track_array() {
        let mut gc = GcHeap::new();
        let arr = Rc::new(RefCell::new(Vec::new()));
        let id = gc.track_array(&arr);
        assert_eq!(id, 0);
        assert_eq!(gc.stats.total_tracked, 1);
    }

    #[test]
    fn test_gc_cleanup_dead() {
        let mut gc = GcHeap::new();

        
        {
            let arr = Rc::new(RefCell::new(Vec::new()));
            gc.track_array(&arr);
        } 

        
        assert_eq!(gc.tracked.len(), 1);

        
        gc.cleanup_dead();
        assert_eq!(gc.tracked.len(), 0);
    }
}
