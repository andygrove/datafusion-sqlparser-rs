#!/usr/bin/env bash
# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

# Regenerates tests/corpus/v1.corpus from the original crate's test suite.
#
# Usage: dev/generate_corpus.sh /path/to/datafusion-sqlparser-rs
#
# Copies the original at the version this repository depends on, adds one line
# that logs every SQL string reaching its tokenizer, runs its tests, and records
# what the original does with each string in every dialect.
set -euo pipefail

V1_CHECKOUT=$1
V1_TAG=v0.63.0

REPO=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

git -C "$V1_CHECKOUT" archive "$V1_TAG" | tar -x -C "$WORK"

perl -0pi -e 's/(pub fn new\(dialect: &\x27a dyn Dialect, query: &\x27a str\) -> Self \{\n)/$1        if let Ok(path) = std::env::var("SQLPARSER_CORPUS_LOG") {\n            use std::io::Write;\n            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {\n                let mut record = query.as_bytes().to_vec();\n                record.push(0);\n                let _ = file.write_all(\&record);\n            }\n        }\n/' "$WORK/src/tokenizer.rs"
grep -q SQLPARSER_CORPUS_LOG "$WORK/src/tokenizer.rs" || {
    echo "could not patch the original's tokenizer" >&2
    exit 1
}

LOG="$WORK/corpus.log"
(cd "$WORK" && SQLPARSER_CORPUS_LOG="$LOG" cargo test --all-features --no-fail-fast)

mkdir -p "$REPO/tests/corpus"
cd "$REPO"
cargo run --release --example generate_corpus -- "$LOG" tests/corpus/v1.corpus
