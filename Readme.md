Practical embodiment of maroon/durable execution ideas into a pratical task
- https://dimakorolev.substack.com/p/distributed-stateful-workflows
- https://dimakorolev.substack.com/p/stateful-orchestration-engines

It's mostly a research project but aims to solve a particular problem of 


# Terminology
MN - maroon node
G - gateway

# Plan

First part of the plan is to get the reliable engine that will get requests from gateways, and distribute them across MNs. The exact execution of requests I'd leave for the next stages.

## Steps
- [ ] local run of etcd in docker compose
    - [ ] add possibility to introduce delays between etcd nodes
- [ ] MN. deploy empty application in N exemplars in docker-compose that just write to the log
- [ ] MN. leader election through etcd
- [ ] write script that finds leader, pauses the container and then restores it after a new leader elected
- [ ] G. create a library/main script that can send requests to MN.
    - [ ] make it in a form of library but for now it can be called via script
- [ ] G. find all the MNs(get from ETCD)
- [ ] G. request range from leader(get leaderID from ETCD)
    - [ ] MN. store used ranges on etcd
- [ ] G. Emit request (can be just print current time) on timer ticker. Should be sent to all MNs
- [ ] MN. Establish transport between MNs (for gossip and state updates)
- [ ] MN. regularly send current vector state to all MNs
- [ ] MN. leader regularly publishes vector commited updates