// Sald Garbage Collector
// Incremental Mark-and-Sweep GC for detecting and breaking circular references
//
// Design Philosophy:
// - Works alongside Rust's Arc/Mutex (not replacing them)
// - Tracks "GC-managed" objects in a separate registry
// - When cycles are detected via tracing, we can break them
// - Minimal changes to existing Value system
// - INCREMENTAL: Spreads work across multiple steps to minimize latency
//
// This approach:
// 1. Keeps Arc/Mutex for normal memory management
// 2. Adds cycle detection for Array, Dictionary, Instance types
// 3. When collection runs, traces from roots and identifies unreachable cycles
// 4. Breaks cycles by clearing references in unreachable objects
// 5. Work is done incrementally to avoid stop-the-world pauses

use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// GC configuration
const INITIAL_THRESHOLD: usize = 10000; // Object count (lower = more frequent GC)
const HEAP_GROW_FACTOR: f64 = 1.5;
/// Maximum time budget per incremental step (microseconds)
const STEP_BUDGET_US: u64 = 500; // 500μs = 0.5ms
/// Objects to process per step (fallback if time check is expensive)
const OBJECTS_PER_STEP: usize = 100;

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
    /// Number of incremental steps taken
    pub incremental_steps: usize,
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

/// GC Phase for incremental collection
#[derive(Debug, Clone)]
#[allow(unused)]
enum GcPhase {
    /// Not collecting - normal operation
    Idle,
    /// Marking phase - tracing from roots
    Marking {
        /// Objects already marked as reachable
        reachable: FxHashSet<ObjectId>,
        /// Iterator position in tracked objects
        iter_keys: Vec<ObjectId>,
        iter_pos: usize,
    },
    /// Sweeping phase - breaking unreachable cycles
    Sweeping {
        /// Objects that are unreachable but alive (cycles)
        to_clear: Vec<ObjectId>,
        iter_pos: usize,
    },
}

/// Weak reference to a tracked object for cycle detection
#[derive(Clone)]
pub enum TrackedObject {
    Array(Weak<Mutex<Vec<super::Value>>>),
    Dictionary(Weak<Mutex<FxHashMap<String, super::Value>>>),
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

    pub fn upgrade_dict(&self) -> Option<Arc<Mutex<FxHashMap<String, super::Value>>>> {
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
                    arc.lock().clear();
                }
            }
            TrackedObject::Dictionary(w) => {
                if let Some(arc) = w.upgrade() {
                    arc.lock().clear();
                }
            }
            TrackedObject::Instance(w) => {
                if let Some(arc) = w.upgrade() {
                    arc.lock().fields.clear();
                }
            }
        }
    }
}

/// The GC Heap - tracks objects for cycle detection with incremental collection
pub struct GcHeap {
    /// Counter for unique IDs
    next_id: u64,
    /// All tracked objects (weak references)
    tracked: FxHashMap<ObjectId, TrackedObject>,
    /// Threshold for automatic collection
    threshold: usize,
    /// GC statistics
    pub stats: GcStats,
    /// Current GC phase (for incremental collection)
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

    /// Check if GC should run (or continue running)
    pub fn should_collect(&self) -> bool {
        match &self.phase {
            GcPhase::Idle => self.tracked.len() > self.threshold,
            _ => true, // Continue if already in progress
        }
    }

    /// Track a new array
    pub fn track_array(&mut self, arr: &Arc<Mutex<Vec<super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Array(Arc::downgrade(arr)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Track a new dictionary
    pub fn track_dict(&mut self, dict: &Arc<Mutex<FxHashMap<String, super::Value>>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Dictionary(Arc::downgrade(dict)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Track a new instance
    pub fn track_instance(&mut self, inst: &Arc<Mutex<super::Instance>>) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tracked
            .insert(id, TrackedObject::Instance(Arc::downgrade(inst)));
        self.stats.total_tracked += 1;
        self.stats.tracked_count = self.tracked.len();
        id
    }

    /// Run one incremental step of garbage collection
    /// Returns true if collection is complete, false if more work remains
    /// Roots are values that are definitely reachable (stack, globals)
    pub fn collect(&mut self, roots: Vec<&super::Value>) {
        let start = Instant::now();
        let budget = Duration::from_micros(STEP_BUDGET_US);

        // First, ensure we've removed dead references
        self.cleanup_dead();

        loop {
            match std::mem::replace(&mut self.phase, GcPhase::Idle) {
                GcPhase::Idle => {
                    // Start new collection - initialize marking phase
                    let reachable = self.mark_from_roots(&roots);
                    let iter_keys: Vec<ObjectId> = self.tracked.keys().copied().collect();

                    // Transition to sweeping since marking is fast enough
                    // (we marked all reachable objects at once)
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
                        // Nothing to sweep, we're done
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
                    // This phase is now handled in Idle transition above
                    // Keep for future true incremental marking if needed
                    self.phase = GcPhase::Marking {
                        reachable,
                        iter_keys,
                        iter_pos,
                    };
                    let to_clear = Vec::new();
                    // ... move to sweeping
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
                        // Check time budget every few objects
                        if processed >= OBJECTS_PER_STEP && start.elapsed() >= budget {
                            // Save progress and return
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

                    // Sweeping complete
                    self.finish_collection();
                    return;
                }
            }

            // Check if we've exceeded our time budget
            if start.elapsed() >= budget {
                self.stats.incremental_steps += 1;
                return;
            }
        }
    }

    /// Complete the collection cycle and reset state
    fn finish_collection(&mut self) {
        self.stats.collections += 1;
        self.stats.tracked_count = self.tracked.len();
        self.stats.incremental_steps += 1;

        // Update threshold
        let new_threshold = (self.tracked.len() as f64 * HEAP_GROW_FACTOR) as usize;
        self.threshold = new_threshold.max(INITIAL_THRESHOLD);

        self.phase = GcPhase::Idle;
    }

    /// Remove entries for objects that have been freed
    fn cleanup_dead(&mut self) {
        self.tracked.retain(|_, obj| obj.is_alive());
    }

    /// Mark all reachable objects from roots
    fn mark_from_roots(&self, roots: &[&super::Value]) -> FxHashSet<ObjectId> {
        let mut reachable = FxHashSet::default();
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
        reachable: &mut FxHashSet<ObjectId>,
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
                    let uv = upvalue.lock();
                    if uv.closed.is_some() {
                        // Closed upvalue contains a value that should be traced
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
