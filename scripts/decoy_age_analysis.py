#!/usr/bin/env python3
"""
Decoy Selection Bias & Output Age Heuristic Analysis
=====================================================
Tests two key privacy attacks on ring signatures:

1. Decoy selection bias: Are decoys chosen uniformly from available UTXOs,
   or is there bias toward recent/old outputs?
2. Output age heuristic: Can an observer identify the real spender by
   checking which ring member is closest in age to the spending transaction?

Usage: python3 scripts/decoy_age_analysis.py
"""

import json
import random
import math
import statistics

SEED = 42
random.seed(SEED)

NUM_UTXOS = 1000      # available UTXOs in the ring
NUM_TXS = 500         # transactions to simulate
RING_SIZE = 11        # standard ring size


def simulate_utxo_pool():
    """Generate a pool of UTXOs with varying ages."""
    utxos = []
    for i in range(NUM_UTXOS):
        # UTXO age: block height when it was created
        # Mix of recent (0-100) and old (100-1000)
        age = random.choices(
            [random.randint(0, 100), random.randint(100, 1000)],
            weights=[0.6, 0.4]  # 60% recent, 40% old
        )[0]
        utxos.append({
            "id": i,
            "block_height": age,
            "amount": random.randint(1, 10000),
        })
    return utxos


def test_decoy_selection_bias():
    """Check if decoy selection is biased toward recent or old UTXOs."""
    print("Decoy Selection Bias Analysis")
    print("-" * 50)

    utxos = simulate_utxo_pool()
    recent = [u for u in utxos if u["block_height"] <= 100]
    old = [u for u in utxos if u["block_height"] > 100]
    print(f"  UTXO pool: {len(utxos)} total ({len(recent)} recent, {len(old)} old)")
    print(f"  Recent/old ratio: {len(recent)/len(old):.2f}")

    # Simulate ring construction many times, measure decoy age distribution
    decoy_ages = []
    for _ in range(NUM_TXS):
        # Randomly pick a real UTXO to spend
        real = random.choice(utxos)
        # Select RING_SIZE-1 decoys (random from pool excluding real)
        pool = [u for u in utxos if u["id"] != real["id"]]
        decoys = random.sample(pool, min(RING_SIZE - 1, len(pool)))
        for d in decoys:
            decoy_ages.append(d["block_height"])
        # Also add the real UTXO's age
        decoy_ages.append(real["block_height"])

    # Analyze distribution
    decoy_recent = sum(1 for a in decoy_ages if a <= 100)
    decoy_old = sum(1 for a in decoy_ages if a > 100)
    total_decoys = len(decoy_ages)
    recent_pct = decoy_recent / total_decoys * 100
    old_pct = decoy_old / total_decoys * 100

    pool_recent_pct = len(recent) / len(utxos) * 100
    pool_old_pct = len(old) / len(utxos) * 100

    print()
    print(f"  Decoys selected: recent={decoy_recent} ({recent_pct:.1f}%), "
          f"old={decoy_old} ({old_pct:.1f}%)")
    print(f"  Pool distribution: recent={pool_recent_pct:.1f}%, "
          f"old={pool_old_pct:.1f}%")
    print()

    bias = abs(recent_pct - pool_recent_pct)
    if bias < 2:
        print("  VERDICT: No significant bias. Decoy selection matches pool distribution.")
    elif bias < 5:
        print("  VERDICT: Mild bias detected. Monitor selection algorithm.")
    else:
        print("  VERDICT: Significant bias! Decoy selection does not reflect pool.")
    print()
    return bias


def test_output_age_heuristic():
    """
    Classic attack: the real spender's UTXO tends to be more recent
    than decoys. An observer can guess which ring member is real by
    picking the one with the most 'typical' age.
    """
    print("Output Age Heuristic Attack")
    print("-" * 50)

    utxos = simulate_utxo_pool()

    correct_guess = 0
    total_attempts = 0

    for _ in range(NUM_TXS):
        # Pick a real spender with a recent UTXO (as real spenders do)
        real = random.choice([u for u in utxos if u["block_height"] <= 200])
        pool = [u for u in utxos if u["id"] != real["id"]]
        decoys = random.sample(pool, min(RING_SIZE - 1, len(pool)))
        ring = decoys + [real]
        random.shuffle(ring)

        total_attempts += 1

        # Heuristic 1: guess the UTXO with median age (not too old, not too new)
        ages = sorted([u["block_height"] for u in ring])
        median_age = ages[len(ages) // 2]
        guess_median = [u for u in ring if u["block_height"] == median_age]
        if guess_median and guess_median[0]["id"] == real["id"]:
            correct_guess += 1

    accuracy = correct_guess / max(1, total_attempts) * 100
    random_chance = 1 / RING_SIZE * 100

    print(f"  Transactions simulated: {total_attempts}")
    print(f"  Ring size: {RING_SIZE}")
    print(f"  Random chance: {random_chance:.1f}%")
    print(f"  Median age heuristic accuracy: {accuracy:.1f}%")
    print()

    if accuracy < random_chance * 1.5:
        print("  VERDICT: Heuristic fails. Anonymity preserved against age analysis.")
    elif accuracy < random_chance * 3:
        print("  VERDICT: Moderate vulnerability. Age heuristic provides some signal.")
    else:
        print("  VERDICT: Weak privacy. Age analysis can identify real spender.")
    print()
    return accuracy


def test_amount_tracking():
    """
    Track amounts across hops: if amount A leaves wallet X and amount
    close to A arrives at wallet Y shortly after, observer may link them.
    """
    print("Amount Tracking Across Hops")
    print("-" * 50)

    wallets = 100
    txs = 2000

    # Generate transactions with amounts
    amounts_out = []
    amounts_in = []

    for i in range(txs):
        amt = random.choices(
            [random.randint(1, 100), random.randint(100, 10000)],
            weights=[0.8, 0.2]
        )[0]

        if i % 3 == 0:
            # Send
            amounts_out.append(amt)
        else:
            # Receive
            amounts_in.append(amt)

    # Check if we can match sends to receives
    matched = 0
    for out in amounts_out:
        for i, inc in enumerate(amounts_in):
            if abs(inc - out) < out * 0.01:  # within 1%
                matched += 1
                break

    match_rate = matched / max(1, len(amounts_out)) * 100
    print(f"  Sends: {len(amounts_out)}, Receives: {len(amounts_in)}")
    print(f"  Matched by exact amount: {matched} ({match_rate:.1f}%)")
    print()

    if match_rate < 1:
        print("  VERDICT: Amount tracking ineffective. High randomness in values.")
    elif match_rate < 5:
        print("  VERDICT: Low correlation. Amount matching rarely succeeds.")
    else:
        print("  VERDICT: Amounts can be tracked. Consider output obfuscation.")
    print()
    return match_rate


if __name__ == "__main__":
    print()
    print("=" * 50)
    print("Privacy Attack Analysis (Phase 9)")
    print("=" * 50)
    print()

    bias = test_decoy_selection_bias()
    age_acc = test_output_age_heuristic()
    amt_match = test_amount_tracking()

    print("=" * 50)
    print("Summary:")
    print(f"  Decoy selection bias: {bias:.1f}% from pool distribution")
    print(f"  Age heuristic accuracy: {age_acc:.1f}% (random: {1/RING_SIZE*100:.1f}%)")
    print(f"  Amount match rate: {amt_match:.1f}%")
    print()

    result = {
        "decoy_bias_pct": round(bias, 1),
        "age_heuristic_accuracy_pct": round(age_acc, 1),
        "amount_match_rate_pct": round(amt_match, 1),
    }
    with open("/tmp/privacy_attack_results.json", "w") as f:
        json.dump(result, f, indent=2)
    print("Saved to /tmp/privacy_attack_results.json")
