## What is key range?
We use uint64 for making unique ids for requests coming to [gateways](./gateway.md).
An alternative way would be to generate uuid, but we decided that using uint64 is more preferable since it has smaller size. Plus by using predictably growing keys inside the range we can do some optimizations on amount of sent data.
But in order to use it we need to make sure that different gateways have non-intersectioned key ranges.

We give key ranges for each gateway on demand.

Current proposal is to have very straightforward implementation. That can/should be more sophisticated in the future. As many corner cases or potential problems are not adressed for example:
- ranges in that version are too big and if we have many gateways or they will be switched off/on - we'll be loosing many keys
	- or we would need to invent a mechanism to reassign already used ranges

These block indexes are predetermined.
So if Gateway A gets into ownership rageIndex 2 - every node can understand which range it operates on and what are the limitations

```go
var singleBlobSize uint64 = uint64(1 << 30)       // 1_073_741_824
var MaxBlockIndex uint64 = uint64(1<<(64-30)) - 1 // [0:17_179_869_184)

func rangeForIndex(index uint64) (uint64, uint64) {
	if index > MaxBlockIndex {
		panic(fmt.Sprintf("index can't be more than %v", MaxBlockIndex))
	}

	return singleBlobSize * index, singleBlobSize*(index+1) - 1
}

func rangeIndexBySequenceID(sequenceID uint64) uint64 {
	if sequenceID > MaxBlockIndex * singleBlobSize {
		panic(fmt.Sprintf("out of range"))
	}

	return sequenceID / singleBlobSize
}
```

Each gateway stores a pair: <(rangeIndex, offset)>

When gateway starts it requests index. Ex:
```
G1: (1,0)
G2: (2,0)
G3: (3,0)
```

So, if gateway was told to use rangeIndex 100500, it deterministically checks which range it belongs to. Let's say: [500:1000). 
Then from now Gateway stores: (100500,0), which means next request will be published with the id 500, then 501, etc.

While gateways publish requests and nodes get them they save at which position they are.
Each node stores sucha a vector: <(blockIndex, position)>

Ex:
```
N1<(1,10), (2,1), (3,300)>
N2<(1,12), (2,1), (3,301)>
N3<(1,11), (2,1), (3,300)>
```





