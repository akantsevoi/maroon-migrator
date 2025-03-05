#!/bin/bash

add_delays() {
    local delay_ms=${1:-50}
    for node in $(docker ps --filter "name=etcd-0*" --format "{{.Names}}"); do
        echo "Adding ${delay_ms}ms delay to $node"
        docker exec "$node" tc qdisc add dev eth0 root netem delay "${delay_ms}ms"
    done
}

remove_delays() {
    for node in $(docker ps --filter "name=etcd-0*" --format "{{.Names}}"); do
        echo "Removing delay from $node"
        docker exec "$node" tc qdisc del dev eth0 root || true
    done
}

case "${1:-}" in
    "add")
        add_delays "${2:-}"
        ;;
    "remove")
        remove_delays
        ;;
    *)
        echo "Usage: $0 add [delay_ms] | remove"
        echo "       delay_ms: delay in milliseconds (default: 50)"
        exit 1
        ;;
esac