# `DATO`
*Decentralized Approximate Timekeeping Oracle*

## Introduction
Many problems in blockchains right now can be helped by having a lightweight protocol that can *agree on which messages were seen, and when they were seen*. Note that this does not require *total ordering*, meaning there is no need for full BFT SMR protocols like Tendermint or Hotstuff, which often implies having a single leader, limited throughput, and added latency.

Instead, reaching rough consensus on time can be achieved by sending a message $M$ to some validators / replicas $R$, getting each of their individual timestamps until a quorum of 2/3 is reached, and then calculating the *median* of all the timestamps to achieve the *certified timestamp*.
Each validator will sign over the message and the timestamp with their BLS private key, and clients can then aggregate these signatures into 1 quorum certificate. This certificate can then be verified by anyone / any chain (assuming PKI).

DATO exhibits the following properties:
- Linear communication complexity: $n \times (m + r)$ ($m$ = message size, $r$ = response size)
- Minimal, constant latency
- Maximal throughput

## Use Cases

We'll look at 3 use cases:
- Fair exchange accountability
- Censorship-resistant message feeds
- Simultaneous release in BRAID

### Fair Exchange accountability
Suppose Alice wants to trade an item with Bob over the internet in an atomic manner, meaning that both parties receive the item, or none do.
This is known as the fair exchange problem, and it is generally unsolvable without a trusted third party (TTP).

PBS is an example of this, where the relay (TTP) mediates the fair exchange by only delivering the payload to the proposer if the proposer has committed to the associated header. Decentralizing PBS could involve replacing the relay with a committee that, among other things, votes on the timeliness of the payload reveal (the [PTC design](https://ethresear.ch/t/payload-timeliness-committee-ptc-an-epbs-design/16054)). 

Other duties like checking block validity and the block value could be done inside of a TEE like in [TEE-Boost](https://collective.flashbots.net/t/tee-boost/3741), but making sure the builder reveals the payload when a proposer has committed to it needs to be solved in another way (host machine can always censor incoming and outgoing TEE traffic).

With DATO, we can optimistically replace the relay by acting as a "record of communication" between the builder and the proposer. In case of a fault, anyone can replay the certified log of records to use as evidence.

**Flow**
- Happy case
1. (Staked) builders make bids to a proposer (direct communication)
2. Proposer selects a winning bid and broadcasts the commitment on DATO and directly to the builder
3. DATO timestamps the commitment ($T_{commit}$)
4. Builder reveals the payload by broadcasting it on DATO (and on the Ethereum P2P network)
5. Signed block is proposed

- Faulty case
1. (Staked) builders make bids to a proposer (direct communication)
2. Proposer selects a winning bid and broadcasts the commitment on DATO and directly to the builder
3. DATO timestamps the commitment ($T_{commit}$)
4. Builder does NOT reveal the payload
5. Proposer misses the block, but can request a certificate of UNAVAILABILITY for the timestamp at $T_{commit} + \Delta$
6. Proposer uses the certficiate of UNAVAILABILITY to slash the builder

## References & Prior Work
- https://collective.flashbots.net/t/current-thinking-for-consensus/2695
- https://www.commonprefix.com/static/clients/flashbots/flashbots_report.pdf
- https://ethresear.ch/t/payload-timeliness-committee-ptc-an-epbs-design/16054