#!/usr/bin/env python3
"""
P9-1: Graph Reconstruction / Chain Analysis Attack
===================================================
Simula um analista externo tentando reconstruir a relação entre carteiras
a partir de dados públicos da blockchain: timestamps, amounts, stealth keys.

Métrica: taxa de acerto do analista (quanto menor, melhor a privacidade).

Uso: python3 scripts/privacy_chain_analysis.py
"""

import json
import random
import hashlib
from collections import defaultdict

SEED = 42
random.seed(SEED)

# ─── Config ───────────────────────────────────────────────────────────
NUM_WALLETS = 100
NUM_TXS = 5000
NUM_HOPS = 50          # transações intermediárias entre wallets reais
AMOUNT_MIN = 1
AMOUNT_MAX = 10000

# Categorias de transações
DIRECT_SEND = 0.30     # 30%: wallet A envia diretamente para B
HOPPED = 0.40          # 40%: A → intermediário → B
MIXED = 0.20           # 20%: A → vários intermediários → B
NOISE = 0.10           # 10%: transações aleatórias sem relação

# ─── Simulação ────────────────────────────────────────────────────────
def simular_chain():
    """Gera dados sintéticos de blockchain com padrões conhecidos."""
    wallets = {f"wallet_{i:03d}": {"balance": random.randint(10000, 100000)}
               for i in range(NUM_WALLETS)}
    
    def send(w, amt):
        if wallets[w]["balance"] >= amt:
            wallets[w]["balance"] -= amt
            return True
        return False
    
    def recv(w, amt):
        wallets[w]["balance"] += amt

    txs = []
    ground_truth = []  # (from, to) para cada relação real

    # Criar algumas relações reais entre wallets
    real_relations = []
    for _ in range(200):  # 200 relações reais
        a = random.choice(list(wallets.keys()))
        b = random.choice(list(wallets.keys()))
        if a != b:
            real_relations.append((a, b))

    # Gerar transações
    for tx_id in range(NUM_TXS):
        tx_type = random.random()
        timestamp = 1000000 + tx_id * 3 + random.randint(0, 5)  # ~3s entre txs

        if tx_type < DIRECT_SEND and real_relations:
            # Ralação direta: A → B
            a, b = random.choice(real_relations)
            amt = random.randint(AMOUNT_MIN, min(AMOUNT_MAX, max(100, wallets[a]["balance"])))
            if not send(a, amt):
                continue
            recv(b, amt)

            stealth_dest = hashlib.sha256(f"{a}{b}{tx_id}".encode()).hexdigest()[:64]

            txs.append({
                "id": tx_id, "from": stealth_dest, "amount": amt,
                "timestamp": timestamp, "type": "direct"
            })
            ground_truth.append((a, b, stealth_dest, amt, timestamp))

        elif tx_type < DIRECT_SEND + HOPPED and real_relations:
            # Com hop intermediário: A → intermediário → B
            a, b = random.choice(real_relations)
            inter = random.choice([w for w in wallets if w != a and w != b])
            amt = random.randint(AMOUNT_MIN, min(AMOUNT_MAX, max(100, wallets[a]["balance"] // 2)))
            if not send(a, amt):
                continue
            recv(inter, amt)

            stealth1 = hashlib.sha256(f"{a}{inter}{tx_id}".encode()).hexdigest()[:64]
            txs.append({
                "id": tx_id, "from": stealth1, "amount": amt,
                "timestamp": timestamp, "type": "hop_in"
            })
            # Segundo hop
            amt2 = random.randint(1, max(1, amt))
            if not send(inter, amt2):
                amt2 = wallets[inter]["balance"]
                if amt2 <= 0:
                    continue
                wallets[inter]["balance"] = 0
            recv(b, amt2)

            stealth2 = hashlib.sha256(f"{inter}{b}{tx_id}".encode()).hexdigest()[:64]
            txs.append({
                "id": tx_id, "from": stealth2, "amount": amt2,
                "timestamp": timestamp + 1, "type": "hop_out"
            })
            ground_truth.append((a, b, stealth1, amt, timestamp))

        elif tx_type < DIRECT_SEND + HOPPED + MIXED and real_relations:
            # Múltiplos intermediários: A → I1 → I2 → I3 → B
            a, b = random.choice(real_relations)
            inters = random.sample([w for w in wallets if w != a and w != b],
                                   min(3, NUM_WALLETS - 2))
            amt = random.randint(AMOUNT_MIN, min(AMOUNT_MAX, max(100, wallets[a]["balance"] // 3)))
            prev = a
            for j, inter in enumerate(inters):
                amt_j = random.randint(1, max(1, amt // (len(inters) - j)))
                wallets[prev]["balance"] -= amt_j
                wallets[inter]["balance"] += amt_j
                stealth = hashlib.sha256(f"{prev}{inter}{tx_id}{j}".encode()).hexdigest()[:64]
                txs.append({
                    "id": tx_id, "from": stealth, "amount": amt_j,
                    "timestamp": timestamp + j, "type": "mix"
                })
                prev = inter
            wallets[prev]["balance"] -= 1
            wallets[b]["balance"] += 1
            stealth_final = hashlib.sha256(f"{prev}{b}{tx_id}".encode()).hexdigest()[:64]
            txs.append({
                "id": tx_id, "from": stealth_final, "amount": 1,
                "timestamp": timestamp + len(inters), "type": "mix"
            })
            ground_truth.append((a, b, stealth_final, 1, timestamp + len(inters)))

        else:
            # Ruído: transação aleatória sem relação real
            a = random.choice(list(wallets.keys()))
            b = random.choice(list(wallets.keys()))
            if a != b:
                amt = random.randint(1, 100)
                wallets[a]["balance"] -= amt
                wallets[b]["balance"] += amt
                stealth = hashlib.sha256(f"noise{tx_id}".encode()).hexdigest()[:64]
                txs.append({
                    "id": tx_id, "from": stealth, "amount": amt,
                    "timestamp": timestamp, "type": "noise"
                })

    return txs, ground_truth, wallets


# ─── Heurísticas de análise ───────────────────────────────────────────
def heuristic_amount(txs):
    """
    Amount correlation: outputs com o mesmo valor podem ser da mesma tx.
    Heurística: se amount X aparece como saída em tx1 e tx2 próximas,
    podem estar ligadas.
    """
    links = defaultdict(set)
    amount_groups = defaultdict(list)
    for tx in txs:
        amount_groups[tx["amount"]].append(tx)

    for amount, group in amount_groups.items():
        if len(group) > 1 and amount > 1 and amount % 1 == 0:
            for i in range(len(group)):
                for j in range(i + 1, len(group)):
                    if abs(group[i]["timestamp"] - group[j]["timestamp"]) < 30:
                        links[group[i]["from"]].add(group[j]["from"])
    return links


def heuristic_timing(txs):
    """
    Timing correlation: txs próximas no tempo podem estar ligadas.
    """
    links = defaultdict(set)
    sorted_txs = sorted(txs, key=lambda x: x["timestamp"])
    for i in range(len(sorted_txs) - 1):
        if sorted_txs[i + 1]["timestamp"] - sorted_txs[i]["timestamp"] < 3:
            links[sorted_txs[i]["from"]].add(sorted_txs[i + 1]["from"])
    return links


def heuristic_value_flow(txs):
    """
    Value flow: se amount X sai em tx1 e amount próximo a X entra em tx2 próxima,
    podem estar ligadas.
    """
    links = defaultdict(set)
    for i in range(len(txs)):
        for j in range(i + 1, min(i + 10, len(txs))):
            if abs(txs[i]["amount"] - txs[j]["amount"]) < 10:
                if abs(txs[i]["timestamp"] - txs[j]["timestamp"]) < 60:
                    links[txs[i]["from"]].add(txs[j]["from"])
    return links


def evaluate_heuristic(links, ground_truth, label):
    """Avalia a taxa de acerto de uma heurística."""
    stealth_truth = set()
    for a, b, stealth, amt, ts in ground_truth:
        stealth_truth.add(stealth)

    true_positives = 0
    false_positives = 0
    total_possible = len(stealth_truth)

    for stealth, connected in links.items():
        if stealth in stealth_truth:
            # Verificar se as conexões estão corretas
            for c in connected:
                if c in stealth_truth:
                    true_positives += 1
                else:
                    false_positives += 1
        else:
            false_positives += len(connected)

    if true_positives + false_positives == 0:
        rate = 0.0
    else:
        rate = true_positives / (true_positives + false_positives) if (true_positives + false_positives) > 0 else 0.0

    print(f"  {label}:")
    print(f"    True positives:  {true_positives}")
    print(f"    False positives: {false_positives}")
    print(f"    Precision:       {rate:.2%}")
    print(f"    Recall:          {true_positives / max(1, total_possible):.2%}")
    return rate


# ─── Main ─────────────────────────────────────────────────────────────
if __name__ == "__main__":
    print("=" * 60)
    print("P9-1: Graph Reconstruction / Chain Analysis Attack")
    print("=" * 60)
    print()

    print(f"Gerando chain sintética...")
    print(f"  Wallets: {NUM_WALLETS}")
    print(f"  Transações: {NUM_TXS}")
    print(f"  Relações reais: 200")
    print()

    txs, ground_truth, wallets = simular_chain()
    print(f"  Transações geradas: {len(txs)}")
    print(f"  Stealth addresses reais: {len(set(g for _, _, g, _, _ in ground_truth))}")
    print()

    print("Aplicando heurísticas de análise...")
    print()

    links_amount = heuristic_amount(txs)
    links_timing = heuristic_timing(txs)
    links_flow = heuristic_value_flow(txs)

    print("Resultados:")
    print()

    r1 = evaluate_heuristic(links_amount, ground_truth, "Amount correlation")
    r2 = evaluate_heuristic(links_timing, ground_truth, "Timing correlation")
    r3 = evaluate_heuristic(links_flow, ground_truth, "Value flow")

    print()
    print("-" * 60)

    best_rate = max(r1, r2, r3)
    print(f"  Melhor heurística: {best_rate:.2%}")
    print(f"  Taxa de acerto global: {(r1+r2+r3)/3:.2%}")
    print()

    # Conclusão
    if best_rate < 0.05:
        print("  VEREDITO: Privacidade robusta contra heurísticas básicas.")
        print("  O analista não consegue reconstruir relações significativamente.")
    elif best_rate < 0.15:
        print("  VEREDITO: Privacidade moderada. Algumas relações são inferíveis.")
        print("  Recomendado: adicionar noise amounts e intervalos de tempo aleatórios.")
    else:
        print("  VEREDITO: Privacidade FRACA. Relações reais são inferíveis.")
        print("  Necessário: revisar mecanismo de privacidade (stealth addresses ou tx routing).")

    print()
    print("Nota: esta é uma análise sintética. Resultados reais dependem")
    print("da implementação específica do protocolo e dos padrões de uso.")
    print()

    # Salvar resultados
    result = {
        "test": "P9-1 Graph Reconstruction",
        "config": {"wallets": NUM_WALLETS, "txs": NUM_TXS},
        "heuristics": {
            "amount_correlation": round(r1, 4),
            "timing_correlation": round(r2, 4),
            "value_flow": round(r3, 4),
            "best": round(best_rate, 4),
        }
    }
    with open("/tmp/p9_1_result.json", "w") as f:
        json.dump(result, f, indent=2)
    print("Resultado salvo em /tmp/p9_1_result.json")
