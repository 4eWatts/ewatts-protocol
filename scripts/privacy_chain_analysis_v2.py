#!/usr/bin/env python3
"""
P9-2/3/4: Chain Analysis — Amount, Timing, Dust
=================================================
Heuristic attacks on blockchain privacy using synthetic data.

- Amount correlation: matching outputs with identical values
- Timing correlation: linking transactions by proximity
- Dust attack: tracing tiny outputs to identify spend patterns

Usage: python3 scripts/privacy_chain_analysis_v2.py
"""

import json
import random
import hashlib
from collections import defaultdict

SEED = 42
random.seed(SEED)

NUM_WALLETS = 100
NUM_TXS = 2000

# ─── Data Generator ───────────────────────────────────────────────────
def gen_amount_correlation_txs():
    """Generate txs with repeated amounts to test amount correlation."""
    txs = []
    wallets = [f"w{i:03d}" for i in range(NUM_WALLETS)]
    balances = {w: random.randint(10000, 100000) for w in wallets}

    ground_truth = []
    for i in range(NUM_TXS):
        a = random.choice(wallets)
        b = random.choice(wallets)
        if a == b:
            continue
        # 30% chance: reuse same amount as a different tx
        if i > 0 and random.random() < 0.3:
            # Reuse amount from a recent transaction
            prev = txs[-min(10, len(txs))]
            amt = prev["amount"]
        else:
            amt = random.randint(1, 10000)
        amt = min(amt, max(1, balances[a]))
        balances[a] -= amt
        balances[b] += amt
        stealth = hashlib.sha256(f"{a}{b}{i}".encode()).hexdigest()[:64]
        txs.append({"from": stealth, "amount": amt, "id": i})
        ground_truth.append((a, b, stealth, amt))
    return txs, ground_truth, wallets


def gen_timing_correlation_txs():
    """Generate txs with clustered timestamps."""
    txs = []
    wallets = [f"w{i:03d}" for i in range(NUM_WALLETS)]
    balances = {w: random.randint(10000, 100000) for w in wallets}
    ground_truth = []
    base_ts = 1000000

    cluster_size = 3      # 3 txs per cluster
    clusters = NUM_TXS // cluster_size
    for c in range(clusters):
        cluster_ts = base_ts + c * random.randint(1, 100)
        # Create 3 txs in rapid succession
        for j in range(cluster_size):
            a = random.choice(wallets)
            b = random.choice(wallets)
            if a == b:
                continue
            amt = random.randint(1, 5000)
            amt = min(amt, max(1, balances[a]))
            balances[a] -= amt
            balances[b] += amt
            stealth = hashlib.sha256(f"tm{c}{j}".encode()).hexdigest()[:64]
            txs.append({
                "from": stealth, "amount": amt,
                "timestamp": cluster_ts + j,  # 1s apart within cluster
                "id": c * cluster_size + j
            })
            ground_truth.append((a, b, stealth, amt, cluster_ts + j))
    return txs, ground_truth


def gen_dust_attack_txs():
    """Dust attack: send tiny amounts to many wallets, trace when spent."""
    txs = []
    wallets = [f"w{i:03d}" for i in range(NUM_WALLETS)]
    balances = {w: random.randint(10000, 100000) for w in wallets}
    ground_truth = []

    # Phase 1: send dust (1 unit) from attacker to many targets
    attacker = wallets[0]
    for target in wallets[1:]:
        balances[attacker] -= 1
        balances[target] += 1
        stealth = hashlib.sha256(f"dust{target}".encode()).hexdigest()[:64]
        txs.append({
            "from": stealth, "amount": 1, "type": "dust_drop",
            "to_target": target, "id": len(txs)
        })
        ground_truth.append((attacker, target, stealth, 1))

    # Phase 2: some targets spend their dust (consolidated with other funds)
    for target in wallets[1:51]:  # 50 targets spend dust
        if balances[target] > 1:
            amt = random.randint(1, min(100, balances[target]))
            balances[target] -= amt
            dest = random.choice(wallets)
            if dest == target:
                dest = attacker
            balances[dest] += amt
            stealth = hashlib.sha256(f"spend_dust{target}".encode()).hexdigest()[:64]
            txs.append({
                "from": stealth, "amount": amt, "type": "dust_spend",
                "source_target": target, "id": len(txs)
            })

    return txs, ground_truth, wallets


# ─── Heuristics ────────────────────────────────────────────────────────
def test_amount_correlation():
    print("P9-2: Amount correlation")
    print("-" * 40)
    txs, gt, _ = gen_amount_correlation_txs()
    print(f"  Generated {len(txs)} transactions with repeated amounts")

    # Heuristic: group by amount, count linkages
    amount_groups = defaultdict(list)
    for tx in txs:
        amount_groups[tx["amount"]].append(tx)

    # Check how many unique amounts are shared by >1 tx
    shared = sum(1 for g in amount_groups.values() if len(g) > 1)
    total = len(amount_groups)
    print(f"  Unique amounts: {total}")
    print(f"  Amounts shared by 2+ txs: {shared} ({100*shared/max(1,total):.1f}%)")

    true_pos = 0
    total_pairs = 0
    for amount, group in amount_groups.items():
        if len(group) < 2:
            continue
        for i in range(len(group)):
            for j in range(i + 1, len(group)):
                total_pairs += 1
                # Check if both txs in same real relationship
                for a, b, stealth, amt in gt:
                    if group[i]["from"] == stealth and group[j]["from"] != stealth:
                        break
                    if group[i]["from"] != stealth and group[j]["from"] == stealth:
                        break
                    if group[i]["from"] == stealth and group[j]["from"] == stealth:
                        true_pos += 1
                        break

    precision = true_pos / max(1, total_pairs) * 100
    max_possible = len(gt)
    recall = true_pos / max(1, max_possible) * 100
    print(f"  Linkable pairs by amount: {total_pairs}")
    print(f"  True positives: {true_pos}")
    print(f"  Precision: {precision:.2f}%")
    print(f"  Recall: {recall:.2f}%")
    print()

    return precision, recall


def test_timing_correlation():
    print("P9-3: Timing correlation")
    print("-" * 40)
    txs, gt = gen_timing_correlation_txs()
    print(f"  Generated {len(txs)} transactions in {len(txs)//3} clusters")

    # Heuristic: link txs within 2s of each other
    sorted_txs = sorted(txs, key=lambda x: x["timestamp"])
    linked = 0
    total_windows = 0
    for i in range(len(sorted_txs) - 1):
        if sorted_txs[i + 1]["timestamp"] - sorted_txs[i]["timestamp"] <= 2:
            total_windows += 1
            # Check if they're in the same real cluster
            id_i = sorted_txs[i]["id"] // 3
            id_j = sorted_txs[i + 1]["id"] // 3
            if id_i == id_j:
                linked += 1

    precision = linked / max(1, total_windows) * 100
    recall = linked / max(1, len(gt)) * 100
    print(f"  Adjacent txs within 2s: {total_windows}")
    print(f"  Correctly linked: {linked}")
    print(f"  Precision: {precision:.2f}%")
    print(f"  Recall: {recall:.2f}%")
    print()
    return precision, recall


def test_dust_attack():
    print("P9-4: Dust attack")
    print("-" * 40)
    txs, gt, wallets = gen_dust_attack_txs()
    dust_txs = [t for t in txs if t.get("type") == "dust_drop"]
    spend_txs = [t for t in txs if t.get("type") == "dust_spend"]
    print(f"  Dust drops: {len(dust_txs)} (amount=1)")
    print(f"  Dust spends: {len(spend_txs)}")

    # Heuristic: trace outputs of amount=1 to see where they're spent
    traceable = 0
    for d in dust_txs:
        target = d.get("to_target", "")
        # Find if this target spent around the same time
        for s in spend_txs:
            if s.get("source_target") == target:
                traceable += 1
                break

    total_traceable = min(len(spend_txs), len(dust_txs))
    print(f"  Dust recipients that spent tracked dust: {traceable}/{len(dust_txs)}")
    trace_rate = traceable / max(1, len(dust_txs)) * 100
    print(f"  Dust trace rate: {trace_rate:.1f}%")
    print()
    return trace_rate


# ─── Main ─────────────────────────────────────────────────────────────
if __name__ == "__main__":
    print()
    print("=" * 50)
    print("Chain Analysis — Phase 9 Privacy Tests")
    print("=" * 50)
    print()

    r1, rec1 = test_amount_correlation()
    r2, rec2 = test_timing_correlation()
    r3 = test_dust_attack()

    print("-" * 50)
    print("Summary:")
    print(f"  Amount correlation precision:  {r1:.1f}%")
    print(f"  Timing correlation precision:  {r2:.1f}%")
    print(f"  Dust trace rate:               {r3:.1f}%")
    print()

    if max(r1, r2, r3) < 10:
        print("VERDICT: Strong privacy — heuristics fail to infer relationships.")
    elif max(r1, r2, r3) < 30:
        print("VERDICT: Moderate privacy — some patterns inferable.")
    else:
        print("VERDICT: Weak privacy — relationships are reconstructible.")
    print()

    result = {
        "P9-2 amount_correlation": {"precision": round(r1, 1), "recall": round(rec1, 1)},
        "P9-3 timing_correlation": {"precision": round(r2, 1), "recall": round(rec2, 1)},
        "P9-4 dust_trace_rate": round(r3, 1),
    }
    with open("/tmp/p9_234_result.json", "w") as f:
        json.dump(result, f, indent=2)
    print("Saved to /tmp/p9_234_result.json")
