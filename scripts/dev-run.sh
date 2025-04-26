#!/bin/bash


ETCD_ENDPOINTS=http://localhost:2379,http://localhost:2381,http://localhost:2383 \
RUST_LOG=debug \
NODE_ID=dev_run_id \
    cargo run