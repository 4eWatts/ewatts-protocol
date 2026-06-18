#!/usr/bin/env python3
"""
Simulacao: R = K / te²
Bootstrap: emite trilhoes de vezes mais que o maduro.
O primeiro Ewatt custa quase nada, o ultimo custa caro.
"""

K = 1.9e13  # mesma calibracao de antes
REF_COMMIT = 1_000_000_000
BLOCKS_PER_YEAR = 52596
GENESIS = 100.0

print("=" * 90)
print("R = K / te²  —  Emissao por tamanho da rede")
print("=" * 90)

te_values = [1e7, 1e8, 1e9, 3e9, 1e10, 1e11, 1e12, 1e13, 1e14]
labels = ["0.01 miner", "0.1 miner", "1 miner", "3 miners", "10 miners", 
          "100 miners", "1K miners", "10K miners", "100K miners"]

print(f"\n{'Rede':>15} | {'Ewatt/bloco':>15} | {'Ewatt/ano':>15} | {'× primeiro':>15} | {'Vs genesis':>15}")
print("-" * 90)

first_annual = None
for te, label in zip(te_values, labels):
    r_block = K / (te * te)
    r_year = r_block * BLOCKS_PER_YEAR
    if first_annual is None and r_year > 0:
        first_annual = r_year
    ratio = first_annual / r_year if r_year > 0 else float('inf')
    pct_genesis = r_year / GENESIS * 100
    
    ratio_str = f"{ratio:,.0f}×" if ratio < 1e15 else f"{ratio:.1e}×"
    print(f"{label:>15} | {r_block:>15.6f} | {r_year:>15.2f} | {ratio_str:>15} | {pct_genesis:>14.2f}%")

print()
print(f"O primeiro miner recebe {first_annual:,.0f} Ewatt/ano")
print(f"Com 1K miners, a emissao cai para {first_annual / 1e6:,.0f} Ewatt/ano = 1 milionesimo")
print(f"Com 1M miners, a emissao e {first_annual / 1e12:,.0f} Ewatt/ano = 1 trilionesimo")

print()
print("=" * 90)
print("CENARIO: Rede cresce de 0.01 a 10K miners em 15 anos")
print("=" * 90)

K_sim = 1.9e19  # recalibrado para gerar ~1B Ewatt/ano com 1 miner

def te_no_ano(ano):
    if ano < 0.5:
        return int(REF_COMMIT * 0.01)
    elif ano < 1:
        return int(REF_COMMIT * 0.1)
    elif ano < 2:
        return int(REF_COMMIT * 1)      # 1 miner
    elif ano < 3:
        return int(REF_COMMIT * 3)      # 3 miners
    elif ano < 5:
        return int(REF_COMMIT * 10)     # 10 miners
    elif ano < 8:
        return int(REF_COMMIT * 100)    # 100 miners
    elif ano < 12:
        return int(REF_COMMIT * 1000)   # 1K miners
    else:
        return int(REF_COMMIT * 10000)  # 10K miners

supply = GENESIS
total_mined = 0
print(f"\n{'Ano':>5} | {'Miners':>8} | {'Ewatt/ano':>18} | {'Supply':>15} | {'% aa':>10} | {'Custo/Ewatt':>12}")
print("-" * 90)

for ano in range(0, 16):
    if ano == 0:
        print(f"{ano:>5} | {'0':>8} | {0:>18.2f} | {supply:>15.2f} | {'0%':>10} | {'$0.00':>12}")
        continue
    
    te = te_no_ano(ano)
    r_block = K_sim / (te * te)
    r_year = r_block * BLOCKS_PER_YEAR
    supply += r_year
    total_mined += r_year
    growth = r_year / (supply - r_year) * 100 if supply > r_year else 0
    
    # Custo energetico: 1 miner a 1 GB/s gasta ~$175/ano
    # Cada miner recebe uma fracao de R proporcional ao seu ce/te
    # Custo_por_Ewatt = custo_do_miner / (R_individual × preco_mercado)
    # Simplificando: se o miner tem 1 GB/s, ce ≈ 1e9
    # R_individual = ce_total / te × R_block = 1e9 / te × R_block (se 1 miner so)
    # Custo ≈ $175/ano / (R_individual × BLOCKS_PER_YEAR)
    miners = te // REF_COMMIT
    if miners > 0 and r_year > 0:
        r_per_miner = r_year / miners
        custo_energia = 175.0  # $/ano para 1 GB/s DDR
        custo_por_ewatt = custo_energia / r_per_miner if r_per_miner > 0 else float('inf')
    else:
        custo_por_ewatt = float('inf')
    
    miners_display = f"{miners}" if miners > 0 else "0"
    custo_str = f"${custo_por_ewatt:.2f}" if custo_por_ewatt < 1e6 else "inf"
    
    supply_str = f"{supply:.2f}"
    if supply > 1e12:
        supply_str = f"{supply/1e12:.2f}T"
    elif supply > 1e9:
        supply_str = f"{supply/1e9:.2f}B"
    elif supply > 1e6:
        supply_str = f"{supply/1e6:.2f}M"
    
    r_year_str = f"{r_year:.2f}"
    if r_year > 1e12:
        r_year_str = f"{r_year/1e12:.2f}T"
    elif r_year > 1e9:
        r_year_str = f"{r_year/1e9:.2f}B"
    elif r_year > 1e6:
        r_year_str = f"{r_year/1e6:.2f}M"
    
    print(f"{ano:>5} | {miners_display:>8} | {r_year_str:>18} | {supply_str:>15} | {growth:>9.2f}% | {custo_str:>12}")

print()
print(f"Total minerado em 15 anos: {total_mined:.2f} Ewatt")
print(f"Supply final: {supply:.2f} Ewatt")
