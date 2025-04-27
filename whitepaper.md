# Durable, distributed, universal task queue for OLTP systems

# Abstract



# Introduction

Quite often nowadays software engineers spend a lot of their time on solving "durable execution" problem. 
Meaning - they don't write code that solves business problems, they write code that solves "infrastructure problems". Like: retries, idempotencyID checks, DLQs, etc
So here we provide the architecture on how to build such a universal system. That will be durable and can be used in many different OLTP scenarious.

We can say that almost any modern distributed event-based system is a task queue. But any time it is built it is built from some building blocks from zero. Lost of human resources are spent on keeping it up and running and providing the proper abstractions, so feature teams can deliver their features quickly.

So the idea is to make a universal solution that will be durable, easy to Ops and feature teams don't need to think about what happens underneath. They need to think only about implementing features, and they can be sure their features will be executed.

# High level design

parts:
- distributed nodes
- consensus
- transactions
- sequence IDs assigner
- DSL
- gateways
- on failures





# Main objectives
- durable, distributed, universal task queue for OLTP systems


# Nodes order
let's say we want to write each 1/16 of second.
What we're going to do:
we have 5 nodes: A,B,C,D,E
Then through etcd we don't need to have a leader, we need to have an order. Ex:

B,C,A,D,E. Which means: Each 1/16 of second B tries to write it's order. If at 1/16 it won't write anything - Next node will try to write at the next tick(at 2/16). If that one will be unsuccessfull as well - the next one, until it will write and from that point it will become the "leader" - Not really a leader. Just the "first writer"

### Ex:
#### terminology
- order: B,C,A,D,E
- ticks
    - tick: 1/16
    - t3 == tick number 3 ~=> 3/16 second from 0
    - t3~ == shortly after tick number 3
#### example scenario
|time|event|
|-|-|
|t1|B fixed sequence|
|t1~| A,D,E got update, reset timers, but not C|
|t2|B fixed sequence|
|t2~| A,D,E got update, reset timers but not C|
|t3|B fixed sequence|
|t3| C thinks B crashed and tries to fix sequence(since it's C's order), failed attempt, reset counter|
|t3~| C,A,D,E got update, reset timers|
|t3~| B crashed|
|t4|nobody tries to write|
|t5| C's order - tries to fix sequence. Success.|
|t5~| D,E got update, reset timers but not A|
|t6| A thinks that C crashed, so tries to fix the sequence - unsuccessfull, reset counter|
|t6| C fixes sequence|


### How to ensure the order 
1. node gets all nodes in "/node" (the exact prefix might be different in the implementation)
    1.1. sort nodes and find itself
    1.2. if it found itself - that's the current index. Stop here
    1.3. if not found - go next
2. from /node/{number} - uses number as an index, gets the {latest}
3. tries to create a record with the lease: "/node/{latest +1}". If unsuccessfull jump to step 1
