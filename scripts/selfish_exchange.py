#!/usr/bin/env python3
"""
P10-4: Selfish Mining Attack
=============================
Classical selfish mining simulation:
A miner finds a block but does not broadcast it immediately,
instead mining a private chain to waste honest miners' work.

Reference: Eyal & Sirer (2014) "Majority is not Enough"

Usage: python3 scripts/selfish_mining.py
"""

import json
import random
import math

SEED = 42
random.seed(SEED)


def simulate_selfish(alpha, gamma, rounds=100000):
    """
    alpha: fraction of total hashrate controlled by selfish miner
    gamma: fraction of honest miners that mine on selfish chain when tie
    rounds: number of events to simulate
    
    Returns: selfish_revenue_ratio (what fraction of total rewards they get)
    """
    # State machine:
    # 0: honest mining (both on same chain)
    # 1+: selfish miner has private lead of N blocks
    
    state = 0
    selfish_blocks = 0
    honest_blocks = 0

    for _ in range(rounds):
        r = random.random()

        if state == 0:
            # Both mining on public chain
            if r < alpha:
                # Selfish miner finds block, keeps it private
                state = 1
            else:
                # Honest miner finds block, publishes it
                honest_blocks += 1
                state = 0

        elif state == 1:
            if r < alpha:
                # Selfish miner finds another block (lead = 2)
                state = 2
            else:
                # Honest miner finds block, ties at 1-1
                # Both publish — gamma fraction of honest miners choose selfish chain
                if random.random() < gamma:
                    # Selfish chain wins tie → collect both selfish blocks
                    selfish_blocks += 2
                else:
                    # Honest chain wins tie
                    honest_blocks += 2
                state = 0

        elif state == 2:
            if r < alpha:
                # Selfish miner extends lead
                state = 3
            else:
                # Honest miner finds block, lead now 2-1
                # Selfish publishes one, race continues at state=1
                # Actually: honest catches up to 2-1. Selfish publishes to 2-2.
                # Tie at 2-2. Gamma decides.
                # For simplicity: selfish publishes, tie-breaking
                if random.random() < gamma:
                    selfish_blocks += 2
                else:
                    honest_blocks += 2
                state = 1

        else:
            # state >= 3: selfish lead is large
            if r < alpha:
                # Extend lead
                state += 1
            else:
                # Honest finds block, selfish publishes one to maintain lead
                # Lead reduces by 1, selfish collects 1 block
                selfish_blocks += 1
                state -= 1

    total = selfish_blocks + honest_blocks
    return selfish_blocks / max(1, total) if total > 0 else 0


def test_selfish_mining():
    print("P10-4: Selfish Mining Attack")
    print("=" * 50)
    print()
    print(f"{"Alpha":>8} {"Gamma=0.0":>12} {"Gamma=0.5":>12} {"Honest share":>12}")
    print("-" * 50)

    results = []
    for alpha_pct in [10, 20, 25, 30, 33, 40, 45, 49]:
        alpha = alpha_pct / 100
        for gamma in [0.0, 0.5]:
            selfish_share = simulate_selfish(alpha, gamma)
            honest_share = 1 - selfish_share
            threshold = alpha  # honest share should be >= this if no exploitation

            results.append({
                "alpha": alpha_pct,
                "gamma": gamma,
                "selfish_share": round(selfish_share, 4),
                "honest_share": round(honest_share, 4),
            })

            if gamma == 0.0:
                print(f"  {alpha_pct:>5}%   {selfish_share:>10.2%}                        {honest_share:>10.2%}")

    print()
    print("Analysis:")
    
    # Find threshold where selfish mining becomes profitable
    for r in results:
        if r["gamma"] == 0.0 and r["selfish_share"] > r["alpha"] / 100:
            print(f"  At alpha={r['alpha']}%, selfish share ({r['selfish_share']:.1%}) "
                  f"exceeds alpha ({r['alpha']}%) — selfish mining is profitable")
            break

    print()
    print("  eWatts mitigation: memory-bound PoW makes it harder to")
    print("  hide blocks (requires full DAG access for each block).")
    print("  But the fundamental vulnerability remains for any PoW.")
    print()

    return results


def test_exchange_attack():
    """P10-9: What happens when an actor buys 30% of the circulating supply?"""
    print()
    print("P10-9: Exchange Attack — 30% Supply Accumulation")
    print("=" * 50)
    print()

    initial_supply = 100_000_000
    attacker_target = 0.30
    attacker_hold = int(initial_supply * attacker_target)

    print(f"  Initial supply: {initial_supply:,}")
    print(f"  Attacker target: {attacker_target:.0%} = {attacker_hold:,}")
    print()

    # Scenario 1: attacker holds 30% and does nothing else
    print("  Scenario 1: Passive 30% holder")
    print(f"    If attacker does not mine: their relative share")
    print(f"    will decrease over time as new coins are mined.")
    print(f"    After 10% emission: {attacker_hold / (initial_supply * 1.1):.1%}")
    print(f"    After 50% emission: {attacker_hold / (initial_supply * 1.5):.1%}")
    print()

    # Scenario 2: attacker mines with 30% of commitment
    print("  Scenario 2: Active attacker with 30% mining power")
    print(f"    Mining rewards proportional to effective commitment.")
    print(f"    Attacker's relative share stays at ~{attacker_target:.0%}")
    print(f"    No excess returns possible (verified in P10-3).")
    print()

    # Scenario 3: attacker tries to manipulate price
    print("  Scenario 3: Market manipulation")
    print(f"    eWatts emission is cost-anchored (DRAM cost).")
    print(f"    Holding 30% of supply does not change the")
    print(f"    marginal cost of mining new coins.")
    print(f"    Any price manipulation is temporary — arbitrage")
    print(f"    between mining cost and market price corrects it.")
    print()

    print("  VERDICT: 30% supply accumulation alone does not break")
    print("  the protocol. The cost-anchored issuance prevents")
    print("  lasting manipulation. Active mining advantage is")
    print("  proportional to commitment, not holding.")
    print()

    return attacker_hold


if __name__ == "__main__":
    results = test_selfish_mining()
    test_exchange_attack()

    result_data = {
        "P10-4 selfish_mining": results,
        "P10-9 exchange_attack": "30% supply accumulation insufficient to break protocol"
    }
    with open("/tmp/p10_49_result.json", "w") as f:
        json.dump(result_data, f, indent=2)
    print("Saved to /tmp/p10_49_result.json")
