#!/usr/bin/env python3
"""
Simulacao: emissao puramente inversa, sem target de crescimento.
R = BASE_EWATT_PER_BLOCK × REF_COMMIT / te

Onde BASE_EWATT_PER_BLOCK e uma constante absoluta (nao percentual do supply).
Objetivo: entender quanto emite no primeiro ano com 1 miner em diferentes BASES.
"""

BLOCKS_PER_YEAR = 52596
REF_COMMIT = 1_000_000_000  # 1 GB/s miner

GENESIS_SUPPLY = 100.0  # Ewatt

BASE_VALUES = [1, 10, 100, 1000, 10000]  # Ewatt per block at te = REF_COMMIT

print("=" * 90)
print("EMISSAO PURAMENTE INVERSA: R = BASE × REF_COMMIT / te")
print("Sem target de crescimento, sem percentual do supply")
print("=" * 90)

for base in BASE_VALUES:
    print(f"\n--- BASE = {base} Ewatt/bloco (1 miner recebe {base} Ewatt/bloco) ---")
    for te_factor in [0.001, 0.01, 0.1, 1, 10, 100, 1000]:
        te = int(REF_COMMIT * te_factor)
        if te_factor < 1:
            label = f"{te_factor*100:.1f}% de 1 miner"
        else:
            label = f"{te_factor:.0f}x 1 miner"
        
        r_per_block = base * REF_COMMIT / te
        r_per_year = r_per_block * BLOCKS_PER_YEAR
        
        # Vs genesis supply
        pct_genesis = r_per_year / GENESIS_SUPPLY * 100
        
        # Supply after 1 year
        supply_yr1 = GENESIS_SUPPLY + r_per_year
        
        print(f"  te = {label:>20} | {r_per_block:>12.2f} Ewatt/bloco | {r_per_year:>14.2f} Ewatt/ano | {pct_genesis:>10.1f}× genesis | supply yr1 = {supply_yr1:.0f} Ewatt")

print("\n" + "=" * 90)
print("CENARIO: 1 miner comeca sozinho, rede cresce gradualmente")
print("Comparacao entre formula supply-based vs inversa pura")
print("=" * 90)

# Simulate 3 years with network growth
import math

def simulate_inverse_pura(base_ewatt, supply_start):
    """R = BASE × REF_COMMIT / te, sem supply-based."""
    supply = supply_start
    total_mined = 0
    for year in range(10):
        # Network growth scenario
        if year < 1:
            te = int(REF_COMMIT * 0.1)  # 0.1 miner effective
        elif year < 2:
            te = REF_COMMIT  # 1 miner
        elif year < 3:
            te = REF_COMMIT * 3  # 3 miners
        elif year < 5:
            te = REF_COMMIT * 10  # 10 miners
        else:
            te = REF_COMMIT * 100  # 100 miners
        
        r_per_block = base_ewatt * REF_COMMIT / te
        r_per_year = r_per_block * BLOCKS_PER_YEAR
        
        supply += r_per_year
        total_mined += r_per_year
        
        growth_pct = r_per_year / (supply - r_per_year) * 100 if supply > r_per_year else 0
        
        print(f"  Ano {year+1}: {te//REF_COMMIT:>4} miners equiv | {r_per_year:>14.2f} Ewatt/ano | supply {supply:>12.2f} | {growth_pct:>6.2f}%/ano | mined total {total_mined:.0f}")

print("\nComparacao para BASE = 100 Ewatt/bloco:")
simulate_inverse_pura(100, GENESIS_SUPPLY)

print("\nComparacao para BASE = 1000 Ewatt/bloco:")
simulate_inverse_pura(1000, GENESIS_SUPPLY)

print("\nComparacao para BASE = 10000 Ewatt/bloco:")
simulate_inverse_pura(10000, GENESIS_SUPPLY)
