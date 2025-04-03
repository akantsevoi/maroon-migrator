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
