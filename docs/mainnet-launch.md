# eWatts Mainnet Launch Plan

## Overview

This document outlines the steps required to launch the eWatts mainnet.

## Prerequisites

- [ ] Testnet stable for 7+ days without issues
- [x] All 127 tests passing (127/127, 4 ignored are DAG benchmarks + emission bounds)
- [ ] Multi-node P2P sync verified across different networks
- [ ] Security audit completed

## Launch Sequence

### Phase 1: Genesis Configuration

1. Generate mainnet genesis key (offline, air-gapped machine)
2. Define initial supply distribution
3. Set mainnet constants:
   - DAG initial size: 8 GB
   - Block time: 600 seconds (10 minutes)
   - Initial difficulty: computed from genesis hash
   - Ramp-up: 10,000 blocks (~70 days)
   - Founder lock: 50,000 blocks (~347 days)

### Phase 2: Infrastructure

1. Deploy 3+ bootstrap nodes across different regions
2. Register DNS entries for bootstrap discovery
3. Set up monitoring and alerting
4. Create block explorer
5. Deploy public dashboard

### Phase 3: Launch

1. Publish genesis block hash
2. Announce launch date (2 weeks notice)
3. Release mining software
4. Genesis ceremony (timestamped)
5. First block mined

### Phase 4: Post-Launch

1. Monitor network health
2. Fix any emergent issues
3. Community building
4. Exchange listings

## Mainnet Parameters

| Parameter | Value | Notes |
|-----------|-------|-------|
| Block time | 600s | 10 minutes |
| DAG initial | 8 GB | Grows ~512 MB/year |
| Mining | MBPoW | DRAM bandwidth |
| Supply | Initial: empty | First coinbase mints |
| Privacy | MLSAG ring 11 | Optional |
| Governance | 95% miner/node | Protocol upgrades |

## Security Considerations

- Genesis key must be generated offline
- Bootstrap nodes should be geographically distributed
- Dashboard should be behind authentication
- Rate limiting on all public endpoints
- Regular security audits

## Post-Mainnet Roadmap

1. Block explorer website
2. Exchange integration guide
3. Mobile wallet
4. Smart contract layer (future)
