#!/usr/bin/env python3
"""
Emission Formula Simulator — eWatts Protocol
Compares BOOTSTRAP_CAP values: 3×, 5×, 10×
"""

# ─── Constants (matching reward.rs) ──────────────────────────────────
BLOCKS_PER_YEAR = 52596
RATE_PRECISION  = 1_000_000
CAP_PRECISION   = 1_000_000
EMISSION_PREC   = 1_000_000_000
UNITS_PER_EWATT = 1_000_000

ANNUAL_GROWTH_RATE = 25_000   # 2.5% in RATE_PRECISION
EFF_REF_INT         = 1_000_000

GENESIS_SUPPLY_EWATT = 100.0
GENESIS_SUPPLY_UNITS = int(GENESIS_SUPPLY_EWATT * UNITS_PER_EWATT)

BOOTSTRAP_CAPS = [3, 5, 10]  # multipliers to test

def emission_rate_per_block(total_eff: int, total_supply: int, bootstrap_cap: int) -> float:
    """Returns emission in Ewatt per block."""
    if total_eff == 0 or total_supply == 0:
        return 0.0

    if total_eff < EFF_REF_INT:
        inv = EFF_REF_INT * CAP_PRECISION // total_eff
        mult = min(bootstrap_cap * CAP_PRECISION // 1, inv)
    else:
        mult = EFF_REF_INT * CAP_PRECISION // total_eff

    # Returns in EMISSION_PRECISION units (1e9 per Ewatt)
    em_prec = (total_supply * ANNUAL_GROWTH_RATE * mult * EMISSION_PREC
               // BLOCKS_PER_YEAR // RATE_PRECISION // CAP_PRECISION // UNITS_PER_EWATT)

    # Convert to Ewatt: EMISSION_PRECISION units / 1e9 = Ewatt
    return em_prec / EMISSION_PREC


# ─── Scenario 1: Solo Miner (static network, worst-case founder accum) ──
def scenario_solo_miner():
    print("=" * 70)
    print("CENÁRIO 1: MINERADOR SOLITÁRIO (pior caso de founder accumulation)")
    print("Te fixo em 1 (1 commitment unitiza) durante todo o período")
    print("=" * 70)

    years = 10
    blocks = years * BLOCKS_PER_YEAR

    for cap in BOOTSTRAP_CAPS:
        supply = GENESIS_SUPPLY_EWATT
        supply_units = GENESIS_SUPPLY_UNITS
        total_mined = 0.0

        # Solo miner: te = 1 (tiny, way below EFF_REF)
        te = 1

        for block in range(blocks):
            em = emission_rate_per_block(te, supply_units, cap)
            supply += em
            supply_units = int(supply * UNITS_PER_EWATT)
            total_mined += em

            # checkpoint every year
            if (block + 1) % BLOCKS_PER_YEAR == 0:
                year = (block + 1) // BLOCKS_PER_YEAR
                annual_growth = ((supply / GENESIS_SUPPLY_EWATT) ** (1 / year) - 1) * 100 if year > 0 else 0

        # Final stats
        founder_pct = (total_mined / supply) * 100
        founder_annualized = ((1 + founder_pct / 100) ** (1 / years) - 1) * 100

        print(f"\nBoot Cap {cap}×:")
        print(f"  Supply final:         {supply:.2f} Ewatt")
        print(f"  Total minerado:       {total_mined:.2f} Ewatt ({founder_pct:.1f}% do supply final)")
        print(f"  Crescimento anual:    {((supply/GENESIS_SUPPLY_EWATT)**(1/years)-1)*100:.2f}%/ano")
        print(f"  Fundador acumula:     ~{founder_pct:.1f}% em {years} anos ({founder_annualized:.2f}%/ano)")


# ─── Scenario 2: Crescimento Gradual ──────────────────────────────────
def scenario_gradual_growth():
    print("\n" + "=" * 70)
    print("CENÁRIO 2: CRESCIMENTO GRADUAL (realista)")
    print("Te cresce de 1 até EFF_REF×10 ao longo de 5 anos, depois estabiliza")
    print("=" * 70)

    years = 10
    blocks = years * BLOCKS_PER_YEAR
    rampup_blocks = 5 * BLOCKS_PER_YEAR  # 5 years to reach 10× equilibrium

    for cap in BOOTSTRAP_CAPS:
        supply = GENESIS_SUPPLY_EWATT
        supply_units = GENESIS_SUPPLY_UNITS
        total_mined = 0.0
        miner_count_history = []

        for block in range(blocks):
            # Network grows: te goes from 1 to EFF_REF × 10 over 5 years
            if block < rampup_blocks:
                progress = block / rampup_blocks
                te = int(1 + (EFF_REF_INT * 10 - 1) * progress)
            else:
                te = EFF_REF_INT * 10  # stabilize at 10× equilibrium

            # Number of miners (rough: each miner ~EFF_REF_INT/1000 commitment)
            miners = max(1, te * 1000 // EFF_REF_INT)  # approximate

            em = emission_rate_per_block(te, supply_units, cap)
            supply += em
            supply_units = int(supply * UNITS_PER_EWATT)
            total_mined += em

            # Track miner count at checkpoints
            if (block + 1) % (BLOCKS_PER_YEAR // 4) == 0:
                miner_count_history.append((block / BLOCKS_PER_YEAR, miners))

        founder_pct = (total_mined / supply) * 100
        supply_growth_annual = ((supply / GENESIS_SUPPLY_EWATT) ** (1 / years) - 1) * 100

        print(f"\nBoot Cap {cap}×:")
        print(f"  Supply final:         {supply:.2f} Ewatt")
        print(f"  Total minerado:       {total_mined:.2f} Ewatt")
        print(f"  Fundador:             {founder_pct:.1f}% do supply final")
        print(f"  Crescimento anual:    {supply_growth_annual:.2f}%/ano")
        print(f"  Miners no pico:       ~{miners}")


# ─── Scenario 3: Crescimento Rápido ──────────────────────────────────
def scenario_fast_growth():
    print("\n" + "=" * 70)
    print("CENÁRIO 3: CRESCIMENTO RÁPIDO (bootstrap bem-sucedido)")
    print("Te salta de 1 para EFF_REF (equilíbrio) em 6 meses")
    print("Depois cresce lentamente até 100× EFF_REF em 10 anos")
    print("=" * 70)

    years = 10
    blocks = years * BLOCKS_PER_YEAR
    bootstrap_blocks = BLOCKS_PER_YEAR // 2  # 6 months to equilibrium

    for cap in BOOTSTRAP_CAPS:
        supply = GENESIS_SUPPLY_EWATT
        supply_units = GENESIS_SUPPLY_UNITS
        total_mined = 0.0

        for block in range(blocks):
            # Phase 1: bootstrap (6 months)
            if block < bootstrap_blocks:
                progress = block / bootstrap_blocks
                te = int(1 + (EFF_REF_INT - 1) * progress)
            else:
                # Phase 2: slow growth to 100× equilibrium
                progress2 = (block - bootstrap_blocks) / (blocks - bootstrap_blocks)
                te = int(EFF_REF_INT + (EFF_REF_INT * 100 - EFF_REF_INT) * progress2)

            em = emission_rate_per_block(te, supply_units, cap)
            supply += em
            supply_units = int(supply * UNITS_PER_EWATT)
            total_mined += em

        founder_pct = (total_mined / supply) * 100
        supply_growth_annual = ((supply / GENESIS_SUPPLY_EWATT) ** (1 / years) - 1) * 100

        # Effective annualized growth in last year (mature phase)
        print(f"\nBoot Cap {cap}×:")
        print(f"  Supply final:         {supply:.2f} Ewatt")
        print(f"  Total minerado:       {total_mined:.2f} Ewatt")
        print(f"  Fundador:             {founder_pct:.1f}% do supply final")
        print(f"  Crescimento anual:    {supply_growth_annual:.2f}%/ano")
        print(f"  Te final:             {te:,} ({te/EFF_REF_INT:.0f}× EFF_REF)")
        print(f"  Emissão final/bloco:  {em:.6f} Ewatt")
        print(f"  Emissão anual final:  {em * BLOCKS_PER_YEAR:.4f} Ewatt/ano")
        print(f"  % supply final:       {em * BLOCKS_PER_YEAR / supply * 100:.2f}%/ano")


# ─── Summary Table ──────────────────────────────────────────────────
def summary_table():
    print("\n" + "=" * 70)
    print("TABELA RESUMO")
    print("=" * 70)

    years = 10
    blocks = years * BLOCKS_PER_YEAR
    rampup_blocks = 5 * BLOCKS_PER_YEAR

    print(f"{'Cap':>6} | {'Cenário':>20} | {'Supply Final':>12} | {'Fundador %':>10} | {'Cresc. Anual':>12}")
    print("-" * 70)

    scenarios = [
        ("Solo Miner", range(blocks), lambda b: 1),
        ("Gradual", range(blocks),
         lambda b: int(1 + (EFF_REF_INT * 10 - 1) * min(b / rampup_blocks, 1.0))),
        ("Rápido", range(blocks),
         lambda b: int(1 + (EFF_REF_INT - 1) * min(b / (BLOCKS_PER_YEAR // 2), 1.0))
         if b < BLOCKS_PER_YEAR // 2
         else int(EFF_REF_INT + (EFF_REF_INT * 100 - EFF_REF_INT) * (b - BLOCKS_PER_YEAR // 2) / (blocks - BLOCKS_PER_YEAR // 2)))
    ]

    for cap in BOOTSTRAP_CAPS:
        for sc_name, sc_blocks, te_fn in scenarios:
            supply = GENESIS_SUPPLY_EWATT
            supply_units = GENESIS_SUPPLY_UNITS
            total_mined = 0.0

            for b in sc_blocks:
                te = te_fn(b)
                em = emission_rate_per_block(te, supply_units, cap)
                supply += em
                supply_units = int(supply * UNITS_PER_EWATT)
                total_mined += em

            founder_pct = total_mined / supply * 100
            growth_ann = ((supply / GENESIS_SUPPLY_EWATT) ** (1 / years) - 1) * 100

            print(f"{cap:>5}× | {sc_name:>20} | {supply:>10.2f} Ew | {founder_pct:>9.2f}% | {growth_ann:>11.2f}%")


if __name__ == "__main__":
    scenario_solo_miner()
    scenario_gradual_growth()
    scenario_fast_growth()
    summary_table()
