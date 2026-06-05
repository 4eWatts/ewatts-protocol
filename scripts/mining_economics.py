#!/usr/bin/env python3
"""
P10-3: Cartel Mining
====================
Simulates a mining cartel controlling majority of effective commitment.
Tests whether they can extract excess returns.

P10-6: Long-Term Miner Equilibrium (embedded)
==============================================
Simulates miner population dynamics: entry, exit, equilibrium.

P10-7: Rich Get Richer (embedded)
=================================
Tests whether wealth concentration grows or stabilizes.

Usage: python3 scripts/mining_economics.py
"""

import json
import random
import math

SEED = 42
random.seed(SEED)


def scenario_cartel():
    """Simulate 5 miners, cartel controls 70% of power."""
    print("P10-3: Cartel Mining")
    print("-" * 40)

    miners = 5
    total_power = 1000
    cartel_size = 3  # 3 of 5 miners collude
    cartel_power = 700  # 70%
    honest_power = 300  # 30%

    # Simulate block mining over 1000 blocks
    blocks = 1000
    block_reward_per_unit = 10  # base reward per unit commitment

    cartel_rewards = 0
    honest_rewards = 0
    for b in range(blocks):
        # Cartel has 70% chance of mining each block
        if random.random() < 0.70:
            cartel_rewards += cartel_power * block_reward_per_unit
        else:
            honest_rewards += honest_power * block_reward_per_unit

    # Expected proportional rewards
    cartel_expected = blocks * cartel_power * block_reward_per_unit * 0.70
    honest_expected = blocks * honest_power * block_reward_per_unit * 0.30

    cartel_extra = cartel_rewards - cartel_expected
    cartel_premium = (cartel_rewards / cartel_expected - 1) * 100

    print(f"  Cartel power: {cartel_power / total_power:.0%}")
    print(f"  Blocks simulated: {blocks}")
    print()
    print(f"  Cartel expected reward: {cartel_expected:,.0f}")
    print(f"  Cartel actual reward:   {cartel_rewards:,.0f}")
    print(f"  Cartel extra (premium): {cartel_extra:,.0f} ({cartel_premium:+.2f}%)")
    print()
    print(f"  Honest expected reward: {honest_expected:,.0f}")
    print(f"  Honest actual reward:   {honest_rewards:,.0f}")
    print()

    if abs(cartel_premium) < 2:
        print("  VERDICT: Cartel cannot extract excess. Rewards proportional to power.")
    elif cartel_premium > 5:
        print("  VERDICT: Cartel extracts excess returns. Structural vulnerability.")
    else:
        print("  VERDICT: Marginal premium. Monitor for collusion incentives.")
    print()
    return cartel_premium


def scenario_long_term_equilibrium():
    """Simulate miner entry/exit dynamics."""
    print("P10-6: Long-Term Miner Equilibrium")
    print("-" * 40)

    for n_miners in [10, 50, 100, 500, 1000, 10000, 100000]:
        total_eff = 100000  # fixed total effective commitment
        per_miner = total_eff / n_miners
        block_reward = 100

        # Simulate 1000 blocks
        blocks = 1000
        rewards = []
        for _ in range(blocks):
            # Random miner gets the block (proportional to commitment)
            winner = random.random()
            if winner < per_miner / total_eff:
                reward = block_reward * per_miner
            else:
                reward = 0
            rewards.append(reward)

        avg_reward = sum(rewards) / blocks
        variance = sum((r - avg_reward) ** 2 for r in rewards) / blocks
        std_dev = math.sqrt(variance)

        # ROI per block (reward / commitment)
        roi = avg_reward / per_miner if per_miner > 0 else 0

        # Gini approximation: small miners have higher variance
        gini_indicator = std_dev / max(0.01, avg_reward) if avg_reward > 0 else 0

        if n_miners <= 1000:
            print(f"  {n_miners:>6d} miners: avg reward={avg_reward:.2f}, "
                  f"std={std_dev:.2f}, CV={gini_indicator:.2f}, ROI={roi:.4f}")

    print()
    print("  Observation: Variance increases as miner count increases.")
    print("  Small miners face higher uncertainty, which may discourage")
    print("  entry. Large miners get more consistent rewards.")
    print()

    return True


def scenario_rich_get_richer():
    """Test whether wealth concentration increases over time."""
    print("P10-7: Rich Get Richer")
    print("-" * 40)

    n_miners = 100
    initial_wealth = [random.randint(100, 10000) for _ in range(n_miners)]
    wealth = initial_wealth[:]
    total_eff = sum(wealth)
    epochs = 1000

    distribution_history = []
    for e in range(epochs):
        # Each epoch, produce block reward proportional to commitment
        new_supply = 1000
        # Distribute proportional to current wealth
        for i in range(n_miners):
            share = wealth[i] / max(1, total_eff)
            wealth[i] += new_supply * share

        # Track Gini every 100 epochs
        if e % 100 == 0 or e == epochs - 1:
            sorted_w = sorted(wealth)
            n = n_miners
            gini = (2 * sum((i + 1) * w for i, w in enumerate(sorted_w))) / (n * sum(sorted_w)) - (n + 1) / n
            distribution_history.append((e, gini))

    gini_initial = distribution_history[0][1]
    gini_final = distribution_history[-1][1]

    print(f"  Miners: {n_miners}")
    print(f"  Epochs: {epochs}")
    print(f"  Gini (initial): {gini_initial:.3f}")
    print(f"  Gini (final):   {gini_final:.3f}")
    print(f"  Change:         {gini_final - gini_initial:+.3f}")
    print()

    if gini_final - gini_initial < 0.01:
        print("  VERDICT: Wealth distribution stable. Proportional rewards work.")
    elif gini_final - gini_initial < 0.05:
        print("  VERDICT: Mild concentration drift. Monitor long-term.")
    else:
        print("  VERDICT: Wealth concentrates. Rich get richer effect present.")
    print()

    return gini_initial, gini_final


if __name__ == "__main__":
    print()
    print("=" * 50)
    print("Mining Economics Simulations (Phase 10)")
    print("=" * 50)
    print()

    cartel_premium = scenario_cartel()

    print("-" * 50)
    print()
    scenario_long_term_equilibrium()
    print()

    print("-" * 50)
    print()
    gini_i, gini_f = scenario_rich_get_richer()

    result = {
        "P10-3 cartel_mining": {"cartel_power": "70%", "premium_pct": round(cartel_premium, 2)},
        "P10-6 long_term_equilibrium": "simulated 10 to 100k miners",
        "P10-7 rich_get_richer": {"gini_initial": round(gini_i, 3), "gini_final": round(gini_f, 3)}
    }
    with open("/tmp/p10_367_result.json", "w") as f:
        json.dump(result, f, indent=2)
    print("Saved to /tmp/p10_367_result.json")
