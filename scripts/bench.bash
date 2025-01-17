#!/bin/bash
#
# Usage (to test): $ SMOKE=true bash scripts/bench.bash
#
# - `SMOKE`: Run benchmarks quickly but not representative
#
set -ex

# Clean-up old results
if [ "$SMOKE" = "" ]; then
rm -rf gh-pages
fi
NEXMARK_CSV_FILE='nexmark_results.csv'
NEXMARK_DRAM_CSV_FILE='dram_nexmark_results.csv'
NEXMARK_PERSISTENCE_CSV_FILE='persistence_nexmark_results.csv'
GALEN_CSV_FILE='galen_results.csv'
LDBC_CSV_FILE='ldbc_results.csv'
rm -f crates/nexmark/${NEXMARK_CSV_FILE} crates/dbsp/${GALEN_CSV_FILE} crates/dbsp/${LDBC_CSV_FILE} crates/nexmark/${NEXMARK_DRAM_CSV_FILE} crates/nexmark/${NEXMARK_PERSISTENCE_CSV_FILE}

# Run nexmark benchmark
EVENT_RATE=10000000
MAX_EVENTS=100000000
GENERATORS=8
CORES=6
if [ "$SMOKE" != "" ]; then
  EVENT_RATE=10000000
  MAX_EVENTS=1000000
fi
cargo bench --bench nexmark -- --first-event-rate=${EVENT_RATE} --max-events=${MAX_EVENTS} --cpu-cores ${CORES}  --num-event-generators ${GENERATORS} --source-buffer-size 10000 --input-batch-size 40000 --csv ${NEXMARK_CSV_FILE}

# Run galen benchmark
cargo bench --bench galen --features="with-csv" -- --workers 10 --csv ${GALEN_CSV_FILE}

# Run ldbc benchmarks
DATASET_SMALL='graph500-22'
DATASET_MEDIUM='datagen-8_4-fb'
if [ "$SMOKE" != "" ]; then
    DATASET_SMALL='wiki-Talk'
    DATASET_MEDIUM='kgs'
fi
cargo bench --bench ldbc-graphalytics --features="with-csv" -- bfs ${DATASET_SMALL} --threads 1 --csv ${LDBC_CSV_FILE}
cargo bench --bench ldbc-graphalytics --features="with-csv" -- bfs ${DATASET_MEDIUM} --threads 6 --csv ${LDBC_CSV_FILE}
cargo bench --bench ldbc-graphalytics --features="with-csv" -- pagerank ${DATASET_SMALL} --threads 1 --csv ${LDBC_CSV_FILE}
cargo bench --bench ldbc-graphalytics --features="with-csv" -- pagerank ${DATASET_MEDIUM} --threads 6 --csv ${LDBC_CSV_FILE}

# Run nexmark benchmark with persistence
EVENT_RATE=5000000
MAX_EVENTS=3000000
CORES=1
if [ "$SMOKE" != "" ]; then
  EVENT_RATE=5000000
  MAX_EVENTS=100000
fi
cargo bench --bench nexmark -- --first-event-rate=${EVENT_RATE} --max-events=${MAX_EVENTS} --cpu-cores ${CORES} --num-event-generators 6 --source-buffer-size 10000 --input-batch-size 40000 --csv ${NEXMARK_DRAM_CSV_FILE}
cargo bench --bench nexmark --features persistence -- --first-event-rate=${EVENT_RATE} --max-events=${MAX_EVENTS} --cpu-cores ${CORES} --num-event-generators 6 --source-buffer-size 10000 --input-batch-size 40000 --csv ${NEXMARK_PERSISTENCE_CSV_FILE}
