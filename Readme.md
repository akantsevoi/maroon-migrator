Practical embodiment of maroon/durable execution ideas into a pratical task
- https://dimakorolev.substack.com/p/distributed-stateful-workflows
- https://dimakorolev.substack.com/p/stateful-orchestration-engines

It's mostly a research project but aims to solve a particular problem of executing OLTP workload durably and relatively quick. So developers will spend time on building the applications, not on making sure everything is durable/consistent/resilient.


# Terminology
MN - maroon node
G - gateway

# How it works

- [Overview](./docs/overview.md)

# Plan

First part of the plan is to get the reliable engine that will get requests from gateways, and distribute them across MNs. The exact execution of requests I'd leave for the next stages.

## Steps
- [X] local run of etcd in docker compose
    - [X] add possibility to introduce delays between etcd nodes
- [X] MN. deploy empty application in N exemplars in docker-compose that just writes to the log
- [X] MN. node order + calculating delays
    - [X] MN. establish p2p connection between all the nodes(exchange public keys) + send pings
    - [X] MN. calculate delay for each node
- [X] MN. regularly exchange current vector state to all MNs
- [X] G. Minimal gateway implementation that just publishes transactions
- [X] MN. Request outdated transactions(p2p)
- [X] MN. Fix "epoch" (local)
- [ ] MN. integration with state machine - "puf-puf-magic"

- [ ] MN. leader regularly publishes commited "epoch" updates to etcd
- [ ] G/MN. Add API to request key ranges for G
    - [ ] MN. store used ranges on etcd
- [ ] dump data to s3?? (??: what exactly we need to persist? Format? Easy to bootstrap later??)
- [ ] write script that finds leader, pauses the container and then restores it after a new leader elected
- [ ] G. make it working as a server/sidecar/library
- [ ] MN. Bootstratp node from s3


# How to run

Run in a single-node mode. When you don't need other nodes => no consensus, no durability but good for high-level testing
```bash
make run-local PORT=3000 CONSENSUS_NODES=1
```

Runs imitation of gateway with the given key-range
- If you run several gateways - each of them should have their own KEY_RANGE
- NODE_URLS specifies nodes which gateway will try to connect to
```bash
make run-gateway KEY_RANGE=1 NODE_URLS=/ip4/127.0.0.1/tcp/3000
```

```bash
# builds docker image
make build-mn

# runs 3 nodes in docker compose
# their urls are: /ip4/127.0.0.1/tcp/3000,/ip4/127.0.0.1/tcp/3001
make run-compose
```