# Security Policy

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Contact the project maintainer directly:

- **Email**: security@ewatts.org
- **PGP Key**: Available at https://ewatts.org/pgp-key.txt

You should receive a response within 48 hours. If not, follow up via the same channel.

## Scope

The following are considered in-scope for security reports:

- Consensus vulnerabilities (chain splits, double-spends, coin inflation)
- Privacy vulnerabilities (deanonymization, transaction linking, amount disclosure)
- Network vulnerabilities (eclipse attacks, sybil attacks, DDoS)
- Cryptographic implementation bugs (MLSAG, Pedersen, range proofs)

## Out of Scope

- Attacks requiring physical access to a mining node
- Social engineering of users
- 51% attacks (by design, testnet has no economic penalty)

## Disclosure Policy

We follow a 90-day responsible disclosure window:

1. Reporter submits vulnerability via secure channel
2. We acknowledge receipt within 48 hours
3. We develop and test a fix
4. Fix is deployed to testnet
5. 90 days after notification, the issue is publicly disclosed

## Rewards

This is a testnet project with no monetary value. No bug bounties are offered at this time.
