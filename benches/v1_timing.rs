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

//! Parse time compared with the original crate.

mod common;

use std::hint::black_box;

use common::{parse_v1, parse_v2, queries};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

fn compare(c: &mut Criterion) {
    for query in queries() {
        let mut group = c.benchmark_group(query.name);
        group.throughput(Throughput::Bytes(query.sql.len() as u64));
        group.bench_function("v1", |b| b.iter(|| parse_v1(black_box(&query.sql))));
        group.bench_function("v2", |b| b.iter(|| parse_v2(black_box(&query.sql))));
        group.finish();
    }
}

criterion_group!(benches, compare);
criterion_main!(benches);
