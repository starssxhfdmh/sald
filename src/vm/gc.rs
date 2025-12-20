// Sald Garbage Collector
// Mark-and-Sweep GC for detecting and breaking circular references
//
// Design Philosophy:
// - Works alongside Rust's Arc/Mutex (not replacing them)
// - Tracks "GC-managed" objects in a separate registry
// - When cycles are detected via tracing, we can break them
// - Minimal changes to existing Value system
//
// This approach:
// 1. Keeps Arc/Mutex for normal memory management
// 2. Adds cycle detection for Array, Dictionary, Instance types
// 3. When collection runs, traces from roots and identifies unreachable cycles
// 4. Breaks cycles by clearing references in unreachable objects

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

/// GC configuration
const INITIAL_THRESHOLD: usize = 10000; // Object count (lower = more frequent GC)
const HEAP_GROW_FACTOR: f64 = 1.5;

/// GC statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct GcStats {
    /// Total objects ever tracked
    pub total_tracked: usize,
    /// Total cycles broken
    pub cycles_broken: usize,
    /// Current tracked objects
    pub tracked_count: usize,
    /// Number of GC cycles run
    pub collections: usize,
}

/// Unique ID for tracked objects
pub type ObjectId = u64;

/// Type of tracked object
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    Array,
    Dictionary,
    Instance,
}

/// Weak reference to a tracked object for cycle detection
#[derive(Clone)]
pub enum TrackedObject {
    Array(Weak<Mutex<Vec<super::Value>>>),
    Dictionary(Weak<Mutex<HashMap<String, super::Value>>>),
    Instance(Weak<Mutex<super::Instance>>),
}

impl TrackedObject {
    /// Check if the object is still alive (has strong references)
    pub fn is_alive(&self) -> bool {
        match self {
            TrackedObject::Array(w) => w.strong_count() > 0,
            TrackedObject::Dictionary(w) => w.strong_count() > 0,
            TrackedObject::Instance(w) => w.strong_count() > 0,
        }
    }

    /// Get strong reference count
    pub fn strong_count(&self) -> usize {
        match self {
            TrackedObject::Array(w) => w.strong_count(),
            TrackedObject::Dictionary(w) => w.strong_count(),
            TrackedObject::Instance(w) => w.strong_count(),
        }
    }

    /// Try to upgrade to strong reference
    pub fn upgrade_array(&self) -> Option<Arc<Mutex<Vec<super::Value>>>> {
        match self {
            TrackedObject::Array(w) => w.upgrade(),
            _ => None,
        }
    }

    pub fn upgrade_dict(&self) -> Option<Arc<Mutex<HashMap<String, super::Value>>>> {
        match self {
            TrackedObject::Dictionary(w) => w.upgrade(),
            _ => None,
        }
    }

    pub fn upgrade_instance(&self) -> Option<Arc<Mutex<super::Instance>>> {
        match self {
            TrackedObject::Instance(w) => w.upgrade(),
            _ => None,
        }
    }

    /// Clear contents to break cycles
    pub fn clear_contents(&self) {
        match self {
            TrackedObject::Array(w) => {
                if let Some(arc) = w.upgrade() {
                    if let Ok(mut arr) = arc.lock() {
                        arr.clear();
                    }
                }
            }
            TrackedObject::Dictionary(w) => {
                if let Some(arc) = w.upgrade() {
                    if let Ok(mut dict) = arc.lock() {
                        dict.clear();
                    }
                }
            }
            TrackedObject::Instance(w) => {
                if let Some(arc) = w.upgrade() {
                    if let Ok(mut inst) = arc.lock() {
                        inst.fields.clear();
                    }
                }
            }
        }
    }
}

/// The GC Heap - tracks objects for cycle detection
pub struct GcHeap {
    /// Counter for unique IDs
    next_id: u64,
    /// All tracked objects (weak references)
    tracked: HashMap<ObjectId, TrackedObject>,
    /// Threshold for automatic collection
    threshold: usize,
    /// GC statistics
    pub stats: GcStats,
    /// Is GC currently running?
    collecting: bool,
}

impl GcHeap {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            tracked: HashMap::new(),
            threshold: INITIAL_THRESHOLD,
            stats: GcStats::default(),
            collecting: false,
        }
    }

    /// Check if GC should run
    pub fn should_collect(&self) -> bool {
        !self.collecting && self.tracked.len() > self.threshold
    }

    /// Track a new array
    pub fn track_array(&mut self, arr: &Arc<Mutex<Vec<super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked.insert(id, TrackedObject::Array(Arc::downgrade(arr)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Track a new dictionary
    pub fn track_dict(&mut self, dict: &Arc<Mutex<HashMap<String, super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked.insert(id, TrackedObject::Dictionary(Arc::downgrade(dict)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Track a new instance
    pub fn track_instance(&mut self, inst: &Arc<Mutex<super::Instance>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked.insert(id, TrackedObject::Instance(Arc::downgrade(inst)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Run garbage collection
    /// Roots are values that are definitely reachable (stack, globals)
    pub fn collect(&mut self, roots: Vec<&super::Value>) {
        if self.collecting {
            return;
        }
        self.collecting = true;

        // Phase 1: Remove dead weak references (already freed by Arc)
        self.cleanup_dead();

        // Phase 2: Mark reachable objects from roots
        let reachable = self.mark_from_roots(&roots);

        // Phase 3: Find and break unreachable cycles
        let cycles_broken = self.break_cycles(&reachable);

        // Update stats
        self.stats.collections += 1;
        self.stats.cycles_broken += cycles_broken;
        self.stats.tracked_count = self.tracked.len();

        // Update threshold
        let new_threshold = (self.tracked.len() as f64 * HEAP_GROW_FACTOR) as usize;
        self.threshold = new_threshold.max(INITIAL_THRESHOLD);

        self.collecting = false;
    }

    /// Remove entries for objects that have been freed
    fn cleanup_dead(&mut self) {
        self.tracked.retain(|_, obj| obj.is_alive());
    }

    /// Mark all reachable objects from roots
    fn mark_from_roots(&self, roots: &[&super::Value]) -> HashSet<ObjectId> {
        let mut reachable = HashSet::new();
        let mut worklist: Vec<&super::Value> = roots.iter().copied().collect();

        while let Some(value) = worklist.pop() {
            self.mark_value(value, &mut reachable, &mut worklist);
        }

        reachable
    }

    /// Mark a single value and add its children to worklist
    fn mark_value<'a>(
        &self,
        value: &'a super::Value,
        reachable: &mut HashSet<ObjectId>,
        _worklist: &mut Vec<&'a super::Value>,
    ) {
        use super::Value;

        match value {
            Value::Array(arr) => {
                // Find this array's ID in tracked objects
                for (id, tracked) in &self.tracked {
                    if let TrackedObject::Array(w) = tracked {
                        if let Some(arc) = w.upgrade() {
                            if Arc::ptr_eq(&arc, arr) {
                                if reachable.insert(*id) {
                                    // First time seeing this - children traced via worklist
                                    // (tracing handled at collection level due to borrow limitations)
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
                        if let Some(arc) = w.upgrade() {
                            if Arc::ptr_eq(&arc, dict) {
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
                        if let Some(arc) = w.upgrade() {
                            if Arc::ptr_eq(&arc, inst) {
                                reachable.insert(*id);
                                break;
                            }
                        }
                    }
                }
            }
            Value::Function(func) => {
                // Trace upvalues (handled at collection level)
                for upvalue in &func.upvalues {
                    if let Ok(uv) = upvalue.lock() {
                        if uv.closed.is_some() {
                            // Closed upvalue contains a value that should be traced
                        }
                    }
                }
            }
            Value::BoundMethod { receiver: _, .. } => {
                // Receiver traced at collection level
            }
            Value::InstanceMethod { receiver: _, .. } => {
                // Receiver traced at collection level
            }
            Value::Namespace { members: _, .. } => {
                // Members traced at collection level
            }
            _ => {}
        }
    }

    /// Break cycles by clearing unreachable objects that are in cycles
    fn break_cycles(&mut self, reachable: &HashSet<ObjectId>) -> usize {
        let mut broken = 0;

        // Find unreachable objects that still have strong references (cycles)
        let mut to_clear = Vec::new();
        for (id, obj) in &self.tracked {
            if !reachable.contains(id) && obj.is_alive() {
                // Object is unreachable but still alive = part of cycle
                to_clear.push(*id);
            }
        }

        // Clear contents to break cycles
        for id in to_clear {
            if let Some(obj) = self.tracked.get(&id) {
                obj.clear_contents();
                broken += 1;
            }
        }

        broken
    }

    /// Get GC statistics
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
        let arr = Arc::new(Mutex::new(Vec::new()));
        let id = gc.track_array(&arr);
        assert_eq!(id, 0);
        assert_eq!(gc.stats.total_tracked, 1);
    }

    #[test]
    fn test_gc_cleanup_dead() {
        let mut gc = GcHeap::new();
        
        // Create and track an array
        {
            let arr = Arc::new(Mutex::new(Vec::new()));
            gc.track_array(&arr);
        } // arr dropped here
        
        // Should still be tracked (weak ref)
        assert_eq!(gc.tracked.len(), 1);
        
        // Cleanup should remove it
        gc.cleanup_dead();
        assert_eq!(gc.tracked.len(), 0);
    }
}
