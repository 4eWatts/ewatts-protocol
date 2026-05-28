#!/usr/bin/env python3
"""
Gera CSV com simulações de emissão para Excel.
3 cenários de adoção × 3 bootstrap caps = 9 simulações.
Dados ano a ano por 10 anos.
"""

import csv
import math

# ─── Constants ───────────────────────────────────────────────────────
BLOCKS_PER_YEAR = 52596
RATE_PRECISION  = 1_000_000
CAP_PRECISION   = 1_000_000
EMISSION_PREC   = 1_000_000_000
UNITS_PER_EWATT = 1_000_000

ANNUAL_GROWTH_RATE = 25_000   # 2.5% in RATE_PRECISION
EFF_REF_INT         = 1_000_000

GENESIS_SUPPLY_EWATT = 100.0
GENESIS_SUPPLY_UNITS = int(GENESIS_SUPPLY_EWATT * UNITS_PER_EWATT)

YEARS = 30  # simulate 30 years to see long-term behavior

# ─── Emission Formula (same as reward.rs) ────────────────────────────
def emission_rate_per_block(total_eff: int, total_supply: int, bootstrap_cap: int) -> float:
    if total_eff == 0 or total_supply == 0:
        return 0.0
    if total_eff < EFF_REF_INT:
        inv = EFF_REF_INT * CAP_PRECISION // total_eff
        mult = min(bootstrap_cap * CAP_PRECISION // 1, inv)
    else:
        mult = EFF_REF_INT * CAP_PRECISION // total_eff
    em_prec = (total_supply * ANNUAL_GROWTH_RATE * mult * EMISSION_PREC
               // BLOCKS_PER_YEAR // RATE_PRECISION // CAP_PRECISION // UNITS_PER_EWATT)
    return em_prec / EMISSION_PREC


# ─── Te Functions ────────────────────────────────────────────────────
def te_pessimista(year: int) -> int:
    """Rede nunca cresce: te = 1 o tempo todo."""
    return 1

def te_base(year: int) -> int:
    """Crescimento gradual: te vai de 1 até 10× EFF_REF em 5 anos, depois estabiliza."""
    ramp_years = 5
    if year < ramp_years:
        progress = year / ramp_years
    else:
        progress = 1.0
    return int(1 + (EFF_REF_INT * 10 - 1) * progress)

def te_otimista(year: int) -> int:
    """Bootstrap rápido: te atinge EFF_REF em 6 meses, 100× em 10 anos, 1000× em 20 anos."""
    if year < 0.5:
        progress = year / 0.5
        return int(1 + (EFF_REF_INT - 1) * progress)
    elif year < 10:
        progress = (year - 0.5) / 9.5
        return int(EFF_REF_INT + (EFF_REF_INT * 100 - EFF_REF_INT) * progress)
    elif year < 20:
        progress = (year - 10) / 10
        return int(EFF_REF_INT * 100 + (EFF_REF_INT * 1000 - EFF_REF_INT * 100) * progress)
    else:
        return EFF_REF_INT * 1000


def miners_from_te(te: int) -> int:
    """Estimate miner count from total effective commitment."""
    # Each miner at 1 GB/s × commit_window contributes ~EFF_REF_INT/1000 to te
    if te == 1:
        return 1
    return max(1, te * 1000 // EFF_REF_INT)


# ─── Simulation ──────────────────────────────────────────────────────
def simulate(scenario_name: str, te_fn, caps: list) -> list:
    """
    Returns list of dicts, one per year per cap.
    Optimized: computes yearly emission directly instead of block-by-block.
    The emission rate changes slowly (as supply grows), so using mid-year
    rate × BLOCKS_PER_YEAR is accurate to <0.1%.
    """
    rows = []

    for cap in caps:
        supply_ewatt = GENESIS_SUPPLY_EWATT
        supply_units = GENESIS_SUPPLY_UNITS
        total_mined = 0.0

        for year in range(YEARS + 1):
            if year == 0:
                rows.append({
                    "Cenario": scenario_name,
                    "Cap": f"{cap}×",
                    "Ano": 0,
                    "Supply_Ewatt": round(supply_ewatt, 6),
                    "Emission_Anual_Ewatt": 0,
                    "Crescimento_Anual_Pct": 0,
                    "Founder_Accum_Pct": 0,
                    "Miners_Estimados": 1,
                    "Emission_Bloco_Ewatt": 0,
                    "Multiplicador_xEquilibrium": 0,
                    "Te": 0,
                })
                continue

            # Mid-year te for representative rate
            te = te_fn(year - 0.5)

            # Per-block emission at start-of-year supply (slightly conservative)
            em_per_block = emission_rate_per_block(te, supply_units, cap)
            yearly_emission = em_per_block * BLOCKS_PER_YEAR

            # Update supply (using average of start and end for growth calc)
            supply_ewatt += yearly_emission
            supply_units = int(supply_ewatt * UNITS_PER_EWATT)
            total_mined += yearly_emission

            growth_pct = yearly_emission / (supply_ewatt - yearly_emission) * 100

            eq_per_block = supply_ewatt * 0.025 / BLOCKS_PER_YEAR
            mult = em_per_block / eq_per_block if eq_per_block > 0 else 0

            miners = miners_from_te(te)
            founder_pct = total_mined / supply_ewatt * 100

            rows.append({
                "Cenario": scenario_name,
                "Cap": f"{cap}×",
                "Ano": year,
                "Supply_Ewatt": round(supply_ewatt, 6),
                "Emission_Anual_Ewatt": round(yearly_emission, 6),
                "Crescimento_Anual_Pct": round(growth_pct, 4),
                "Founder_Accum_Pct": round(founder_pct, 4),
                "Miners_Estimados": miners,
                "Emission_Bloco_Ewatt": round(em_per_block, 8),
                "Multiplicador_xEquilibrium": round(mult, 4),
                "Te": te,
            })

    return rows


# ─── Main ────────────────────────────────────────────────────────────
CAPS = [3, 5, 10]
SCENARIOS = [
    ("Pessimista - Solo Miner", te_pessimista),
    ("Base - Crescimento Gradual", te_base),
    ("Otimista - Bootstrap Rapido", te_otimista),
]

all_rows = []
for sc_name, te_fn in SCENARIOS:
    all_rows.extend(simulate(sc_name, te_fn, CAPS))

# Write CSV
fieldnames = [
    "Cenario", "Cap", "Ano",
    "Supply_Ewatt", "Emission_Anual_Ewatt",
    "Crescimento_Anual_Pct", "Founder_Accum_Pct",
    "Miners_Estimados", "Emission_Bloco_Ewatt",
    "Multiplicador_xEquilibrium", "Te"
]

with open("emission_simulacao_30anos.csv", "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=fieldnames, delimiter=";")
    writer.writeheader()
    writer.writerows(all_rows)

print(f"CSV gerado: emission_simulacao_30anos.csv")
print(f"Total de linhas: {len(all_rows)}")
print(f"{len(SCENARIOS)} cenarios × {len(CAPS)} caps × {YEARS + 1} anos = {len(all_rows)}")

# Print summary table
# Summary table (yearly steps, not block-by-block — fast)
print("\n" + "=" * 140)
header = f"{'Cenario':>30} {'Cap':>5}"
for y in [0, 1, 3, 5, 10, 20, 30]:
    header += f" {'Ano '+str(y):>10}"
print(header)
print("=" * 140)
for sc_name, te_fn in SCENARIOS:
    for cap in CAPS:
        sim_rows = simulate(sc_name, te_fn, [cap])
        line = f"{sc_name:>30} {cap:>4}×"
        for r in sim_rows:
            if r["Ano"] in [0, 1, 3, 5, 10, 20, 30]:
                line += f" {r['Supply_Ewatt']:>10.2f}"
        print(line)
