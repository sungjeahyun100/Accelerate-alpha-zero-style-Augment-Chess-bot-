//! Counts allocator requests around one host operation at a time. This is a
//! process-local diagnostic, not a production allocator or a peak-RSS meter.

use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// The only unsafe boundary is forwarding the exact layout/pointer supplied by
// Rust's allocator contract to System. Counters use relaxed atomics and never
// read or retain allocation contents.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, old, new_size) };
        if !next.is_null() {
            REALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            REALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        next
    }
}

fn snapshot() -> [u64; 4] {
    [
        ALLOCATIONS.load(Ordering::Relaxed),
        ALLOCATED_BYTES.load(Ordering::Relaxed),
        REALLOCATIONS.load(Ordering::Relaxed),
        REALLOCATED_BYTES.load(Ordering::Relaxed),
    ]
}

pub(super) fn measure<T>(
    samples: u64,
    mut operation: impl FnMut() -> Result<T, String>,
) -> Result<Value, String> {
    let mut allocation_calls = Vec::with_capacity(samples as usize);
    let mut allocated_bytes = Vec::with_capacity(samples as usize);
    let mut reallocation_calls = Vec::with_capacity(samples as usize);
    let mut reallocated_bytes = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let before = snapshot();
        let output = black_box(operation()?);
        let after = snapshot();
        allocation_calls.push(after[0].saturating_sub(before[0]));
        allocated_bytes.push(after[1].saturating_sub(before[1]));
        reallocation_calls.push(after[2].saturating_sub(before[2]));
        reallocated_bytes.push(after[3].saturating_sub(before[3]));
        drop(output);
    }
    Ok(json!({
        "samples": samples,
        "allocationCalls": allocation_calls,
        "requestedAllocationBytes": allocated_bytes,
        "reallocationCalls": reallocation_calls,
        "requestedReallocationBytes": reallocated_bytes,
        "scope": "one synchronous operation per sample; process-global allocator counters; excludes result drop",
    }))
}
