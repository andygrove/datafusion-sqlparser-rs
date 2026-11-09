// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Heap allocations per parse compared with the original crate.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use common::{parse_v1, parse_v2, queries};

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new_size as u64, Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Returns the number of allocations and total bytes requested by `parse`.
fn measure<T>(parse: impl FnOnce() -> T) -> (u64, u64) {
    let (allocations, bytes) = (ALLOCATIONS.load(Relaxed), BYTES.load(Relaxed));
    black_box(parse());
    (
        ALLOCATIONS.load(Relaxed) - allocations,
        BYTES.load(Relaxed) - bytes,
    )
}

fn main() {
    println!("| query | v1 allocations | v2 allocations | v1 bytes | v2 bytes |");
    println!("|---|---:|---:|---:|---:|");
    for query in queries() {
        let (v1_count, v1_bytes) = measure(|| parse_v1(&query.sql));
        let (v2_count, v2_bytes) = measure(|| parse_v2(&query.sql));
        println!(
            "| {} | {v1_count} | {v2_count} | {v1_bytes} | {v2_bytes} |",
            query.name
        );
    }
}
