# Gateway (GW)

## Synonyms
- sidecar
- client library

## Init

Before GW can start working it should request range for unique keys from [MN/maroon node](./maroon-node.md)

## Work
When [GW](./gateway.md) gets a new request:
- it forms a request to maroon node
- attaches sequence id from it's current [key range](./keys-range.md)
    - if it used up the whole range - it recuests the next index range
- broadcasts it to all MNs
    - maybe in the future it will broadcast only to the nearest MNs and they will be responsible for the rest of them
    - TODO: what if it fails to send to all MNs now? Should it retry infinitely?

## Rest
Knows [MN or maroon node](./maroon-node.md)s topology:
- who is the leader?


