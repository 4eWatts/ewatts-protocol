#!/usr/bin/env python3
"""
P10-5: Spam Economics
=====================
Calculates the cost of spamming the network vs. the cost imposed on the network.
Key metric: attacker cost >= network cost (healthy) or attacker cost < network cost (vulnerable).

Usage: python3 scripts/spam_economics.py
"""

import json

# eWatts protocol constants (from constants.rs)
BLOCK_TX_LIMIT = 1000          # soft: no hard limit, but practical bound
BLOCK_TIME_SECS = 600          # 10 min target
TX_FEE_DEFAULT = 0             # eWatts has no required fee (min fee = 0)
COST_PER_TX_BYTES = 250        # ~250 bytes per tx (stealth + MLSAG + range proofs)
STORAGE_COST_GB_MONTH = 5.0    # $5/GB/month (cloud object storage)
MINING_COST_PER_HASH = 0.001   # $ per DRAM access (very rough estimate)

# Attack scenarios
def scenario_basic_spam():
    """Basic spam: send 1M txs with minimum amounts."""
    print("Scenario: Basic spam — 1 million zero-fee transactions")
    print("-" * 50)

    num_txs = 1_000_000
    tx_size_mb = num_txs * COST_PER_TX_BYTES / (1024 * 1024)
    storage_gb = tx_size_mb / 1024

    # Attacker cost: just bandwidth + minimal compute
    attacker_compute_cost = num_txs * 0.00001      # $0.00001 per tx (cheap VPS)
    attacker_bandwidth_cost = tx_size_mb * 0.001   # $0.001/MB (hetzner)
    attacker_total = attacker_compute_cost + attacker_bandwidth_cost

    # Network cost: each node stores + validates
    node_storage_cost_month = storage_gb * STORAGE_COST_GB_MONTH
    # Validation: ~5ms per tx on average hardware
    node_validation_hours = (num_txs * 0.005) / 3600
    node_compute_cost = node_validation_hours * 0.10  # $0.10/hr spot

    print(f"  Transactions: {num_txs:,}")
    print(f"  Data size: {tx_size_mb:.1f} MB ({storage_gb:.2f} GB)")
    print()
    print(f"  Attacker cost: ${attacker_total:.2f}")
    print(f"  Per-node storage cost (1 month): ${node_storage_cost_month:.2f}")
    print(f"  Per-node validation cost: ${node_compute_cost:.2f}")
    print()

    ratio = attacker_total / max(0.01, node_storage_cost_month + node_compute_cost)
    print(f"  Cost ratio (attacker / per-node): {ratio:.2f}x")
    if ratio < 0.1:
        print("  VERDICT: Critical vulnerability. Attacker costs << network costs.")
    elif ratio < 0.5:
        print("  VERDICT: Moderate vulnerability. Attacker pays less than network.")
    else:
        print("  VERDICT: Healthy. Attacker cost >= network cost.")
    print()
    return attacker_total, node_storage_cost_month + node_compute_cost


def scenario_max_blocks():
    """Continuous max-size blocks for 24h."""
    print("Scenario: Max-size blocks for 24 hours")
    print("-" * 50)

    blocks_per_day = 86400 / BLOCK_TIME_SECS  # ~144 blocks
    txs_per_block = BLOCK_TX_LIMIT
    total_txs = blocks_per_day * txs_per_block
    tx_size_mb = total_txs * COST_PER_TX_BYTES / (1024 * 1024)

    # Attacker must mine ~144 blocks (need 51% hashrate for 24h)
    attacker_mining_cost = blocks_per_day * 100  # ~$100/block mining cost
    # Or cheaper: just submit txs to mempool (no mining required for spam)
    attacker_submit_cost = total_txs * 0.000005  # $0.000005 per HTTP POST

    print(f"  Max blocks/day: {blocks_per_day:.0f}")
    print(f"  Max txs/day: {total_txs:,.0f}")
    print(f"  Data size: {tx_size_mb:.1f} MB ({tx_size_mb/1024:.2f} GB)")
    print()
    print(f"  Attacker mining cost: ${attacker_mining_cost:.2f}")
    print(f"  Attacker submit-only cost: ${attacker_submit_cost:.2f}")
    print(f"  Network storage cost (all nodes, 1 month): ${tx_size_mb/1024 * STORAGE_COST_GB_MONTH * 10:.2f} (10 nodes)")
    print()

    ratio = attacker_submit_cost / max(0.01, tx_size_mb/1024 * STORAGE_COST_GB_MONTH * 10)
    print(f"  Cost ratio: {ratio:.4f}x")
    print()
    return attacker_submit_cost, tx_size_mb/1024 * STORAGE_COST_GB_MONTH * 10


def scenario_mempool_flood():
    """Flood mempool without mining: cost to keep mempool full."""
    print("Scenario: Mempool flood — keep mempool at 10k pending txs")
    print("-" * 50)

    mempool_target = 10_000
    # Each node stores mempool in RAM
    mempool_ram_mb = mempool_target * COST_PER_TX_BYTES / (1024 * 1024)
    ram_cost_month = mempool_ram_mb / 1024 * 100  # $100/GB/month RAM

    # Attacker: just send the txs
    attacker_cost = mempool_target * 0.00001  # one-time
    # To keep mempool full: re-send txs that were mined or expired
    daily_refresh = mempool_target  # re-send daily
    attacker_daily = daily_refresh * 0.00001

    print(f"  Mempool target: {mempool_target:,} txs")
    print(f"  RAM consumed: {mempool_ram_mb:.1f} MB")
    print(f"  RAM cost/node/month: ${ram_cost_month:.2f}")
    print()
    print(f"  Attacker one-time: ${attacker_cost:.2f}")
    print(f"  Attacker daily refresh: ${attacker_daily:.2f}")
    print(f"  Network RAM cost (10 nodes): ${ram_cost_month * 10:.2f}/month")
    print()

    ratio = attacker_cost / max(0.01, ram_cost_month * 10)
    print(f"  Cost ratio: {ratio:.2f}x")
    print()
    return attacker_cost, ram_cost_month * 10


if __name__ == "__main__":
    print()
    print("=" * 50)
    print("P10-5: Spam Economics")
    print("=" * 50)
    print()

    a1, n1 = scenario_basic_spam()
    a2, n2 = scenario_max_blocks()
    a3, n3 = scenario_mempool_flood()

    print("=" * 50)
    print("Summary:")
    print(f"  Basic spam:     attacker=${a1:.2f} vs network=${n1:.2f} (ratio={a1/max(0.01,n1):.2f}x)")
    print(f"  Max blocks:     attacker=${a2:.2f} vs network=${n2:.2f} (ratio={a2/max(0.01,n2):.4f}x)")
    print(f"  Mempool flood:  attacker=${a3:.2f} vs network=${n3:.2f} (ratio={a3/max(0.01,n3):.2f}x)")
    print()

    # Check if eWatts has minimum fee protection
    print("Note: eWatts has no minimum transaction fee.")
    print("Zero-fee txs make spam attacks cheaper than fee-based chains.")
    print("Protection: mempool prioritization by effective commitment,")
    print("not by fee. Spam txs with zero commitment get low priority.")
    print()

    result = {
        "P10-5 spam_economics": {
            "basic_spam": {"attacker_cost": round(a1, 2), "network_cost": round(n1, 2)},
            "max_blocks": {"attacker_cost": round(a2, 2), "network_cost": round(n2, 2)},
            "mempool_flood": {"attacker_cost": round(a3, 2), "network_cost": round(n3, 2)},
        }
    }
    with open("/tmp/p10_5_result.json", "w") as f:
        json.dump(result, f, indent=2)
    print("Saved to /tmp/p10_5_result.json")
