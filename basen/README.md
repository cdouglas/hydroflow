# V0 (chain replication)

## namespace

(no outgoing calls)

## datanodes

REGISTER(id) -> NN:
- join the cluster: recv uniq clusterID for this session
- report blocks and state

HEARTBEAT(id, list<blockid>) -> NN:
- report new blocks and state
- refresh nonce for leases

## client

CREATE(key) -> NN: send a key + overwrite flag
- fail if overwrite=false and key exists
- fail if there is alrady an active lease for the key and clientid doesn't match
returns a lease

ADDBLOCK(lease) -> NN
- matches to a key w/ outstanding lease
returns token w/ ordered set of nodes (assume chain) + logical ID

CLOSE(lease, list<blockid>) -> NN: + durability flag
- relinquish the lease, optionally wait until data are durable on all nodes

OPEN(key, location): /AZ/pod/rack/node/device
- recv list of sealed blocks w/ locations and token

INFO(key): -> NN
- current size + active len? Whatever

READ(block, token) -> DN:
- open each block, in order and stream data back. Assume namespace orders replicas by client distance

WRITE(token, data, offset) -> DN: 
- send data to the first node in the list, replicates down the chain
returns acknowledgements for each (token, offset) pair


# lease
- UUID
- expiration? hash of key + secret w/ datanode?


# Discussion w/ Lucky on fault tolerance
- correlation matrix (pairwise probability of correlated failure)
- fault domains as discrete groupings is inaccurate and doesn't capture heterogeneity
- spot instances could be used for storage, but replicating across spot instances in the same AZ is not a discrete risk
- replicating across devices attached to the same node increases durability, particularly if if one device is a dying SSD
- fault domains and replication should not be discrete choices, but exist in a continuous space
- Copysets: subsets of cluster nodes to reduce the probability of data loss


## V0 example

### namespace
- initialize the namespace with an explicit operation, creates an empty namespace w/ unique ID
  - can be in-memory at first
- create nonce, but until there's namespace to recover, nothing else to do
- on receipt of client traffic, reject if there are fewer than K nodes
- [opt] function maps DN node information to a location; given two locations, can compute distance
- record dn id -> timestamp for liveness tracking

### datanodes
- create a unique ID for the DN if it doesn't exist
- initialize data directories (i.e., read in existing blocks)
- REGISTER with configured NN address
- periodically send HEARTBEAT to NN
- only checks whether the nonce on the lease is still valid, doesn't know stream/inode/whatever
on WRITE(token, pkt, offset)
- lookup existing token, create a new entry if it doesn't exist
- record pkt into buffer, track HWM as contiguous data into the buffer
- 
- advance and ack HWM if offset is exactly equal to existing HWM; otherwise buffer


### client
write
- CREATE(key) to NN, recv lease
- ADDBLOCK(lease) to NN, recv token w/ list of nodes
- WRITE(token, pkt_0, 0) to first node in list, manage acks
  - if acks are not received, resend after timeout
  - for now, client does its own chunking
- ADDBLOCK* for subsequent blocks
- CLOSE(lease) to NN, recv ack
read
- OPEN(key) to NN, recv list of blocks w/ locations
- READ(block, token) to first node in list, recv data OR on timeout, try next replica

# V1 Durability



# V2 Fault Tolerance


