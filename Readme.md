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
- [ ] MN. leader regularly publishes vector commited updates
- [ ] write script that finds leader, pauses the container and then restores it after a new leader elected
- [ ] G. create a library/main script that can send requests to MN.
    - [ ] make it in a form of library but for now it can be called via script
- [ ] G. request range from leader(get leaderID from ETCD)
    - [ ] MN. store used ranges on etcd
- [ ] G. Emit request (can be just print current time) on timer ticker. Should be sent to all MNs