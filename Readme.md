Practical embodiment of maroon/durable execution ideas into a pratical task
- https://dimakorolev.substack.com/p/distributed-stateful-workflows
- https://dimakorolev.substack.com/p/stateful-orchestration-engines

It's mostly a research project but aims to solve a particular problem of 


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
- [ ] MN. Request outdated transactions(p2p) (in prog...)
- [ ] MN. Fix "epoch" (local)
- [ ] MN. integration - "puf-puf-magic"






- [ ] MN. leader regularly publishes commited "epoch" updates to etcd
- [ ] G/MN. Add API to request key ranges for G
    - [ ] MN. store used ranges on etcd
- [ ] dump data to s3?? (??: what exactly we need to persist? Format? Easy to bootstrap later??)
- [ ] write script that finds leader, pauses the container and then restores it after a new leader elected
- [ ] G. make it working as a server/sidecar/library
- [ ] MN. Bootstratp node from s3