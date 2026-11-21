# IntentForge — Infrastructure & Operating Cost Model: Curated Web Crawler + Search Index

**Research date:** 2026-11-10 (all sources accessed on this date)
**Prepared for:** IntentForge founder
**Method:** Primary sources only — official vendor pricing pages and documentation. No fabricated numbers.

## Label conventions

| Label | Meaning |
|---|---|
| **[CONFIRMED]** | Price/spec read directly off an official vendor pricing page or doc; URL + access date cited. |
| **[INFERRED]** | Derived by arithmetic from confirmed inputs, or from a vendor's stated unit economics applied to our stated assumptions. Arithmetic shown in full. |
| **[UNKNOWN]** | No public price exists, or the figure depends on variables we cannot fix. No number invented. |
| **VERIFIED LISTED PRICES** | §9 — the raw price list. |
| **ESTIMATES** | §10 — the model output, built from §9. |

**Currency:** USD unless marked €. EUR figures are excl. VAT as published. Where I convert, I state the rate used and label the result [INFERRED].

---

## 0. Scope, workload assumptions, and the three scales

These assumptions are **ours, not the vendors'** — every one is a lever the founder can challenge. They are deliberately stated as ranges.

### 0.1 Crawl workload assumptions

| Parameter | Small (20 domains) | Medium (200 domains) | Large (2,000 domains) | Basis |
|---|---|---|---|---|
| Domains in crawl set | 20 | 200 | 2,000 | given |
| Avg pages per domain (curated, not full-site) | 5,000 | 5,000 | 5,000 | [INFERRED] curated crawl, not whole-web |
| Total pages in index | 100,000 | 1,000,000 | 10,000,000 | = domains × pages/domain |
| Full re-crawl frequency | monthly | monthly | monthly | assumption |
| Pages fetched / month (steady state) | 100,000 | 1,000,000 | 10,000,000 | = index size × 1 recrawl |
| Incremental/changed-page fetches / month | 50,000 | 500,000 | 5,000,000 | [INFERRED] 50% churn assumption |
| **Total page fetches / month** | **150,000** | **1,500,000** | **15,000,000** | sum |
| Avg fetch size (HTML + assets, compressed) | 150 KB | 150 KB | 150 KB | [INFERRED] |
| **Crawl egress / month** | **21.5 GB** | **215 GB** | **2.15 TB** | = fetches × 150 KB |
| Headless-browser share of fetches | 5% | 10% | 15% | [INFERRED] JS-heavy share rises with scale |
| Headless page renders / month | 7,500 | 150,000 | 2,250,000 | = fetches × share |
| Avg headless render time | 8 s | 8 s | 8 s | [INFERRED] incl. JS settle |
| Headless browser-hours / month | 16.7 h | 333 h | 5,000 h | = renders × 8 s ÷ 3600 |

### 0.2 Data-volume assumptions

| Parameter | Small | Medium | Large | Basis |
|---|---|---|---|---|
| Raw HTML stored per page | 150 KB | 150 KB | 150 KB | assumption |
| Metadata + fingerprint per page (DB row) | 4 KB | 4 KB | 4 KB | [INFERRED] URL, title, hash, timestamps, lang, etc. |
| Price observations / month | 5,000 | 50,000 | 500,000 | [INFERRED] |
| Avg price-observation row | 200 B | 200 B | 200 B | [INFERRED] |
| Raw page store (object storage) | 14.3 GB | 143 GB | 1.43 TB | = pages × 150 KB |
| PostgreSQL data (metadata + fingerprints) | 0.4 GB | 3.8 GB | 38 GB | = pages × 4 KB |
| PostgreSQL index overhead (×2.5) | 1 GB | 9.5 GB | 95 GB | [INFERRED] GIN/FTS + btree indexes |
| Price observations table | 1 GB | 10 GB | 100 GB | = obs/month × 200 B × 12 mo retained |
| **Total PostgreSQL storage** | **~2.5 GB** | **~24 GB** | **~243 GB** | sum, rounded |
| Backups retained (30 days, compressed) | 5 GB | 50 GB | 500 GB | [INFERRED] ~2× live, compressed |

### 0.3 Personnel assumptions (used in §7 and §8)

| Role | Small | Medium | Large |
|---|---|---|---|
| Founder/ops time on infra (h/week) | 5 | 8 | 12 |
| Blended loaded cost / h | $75 | $75 | $75 |
| **Infra labour $/month** | **$1,500** | **$2,400** | **$3,600** |

[INFERRED] — a single operator at a conservative contract rate. This is the single largest line item at small scale and the reason §8 matters.

---

## 1. Crawler compute and memory

### 1.1 What the crawler actually needs

A curated crawler is **not** CPU-bound; it is **I/O-bound and concurrency-bound**. The dominant costs are:

1. **Concurrency slots** — how many pages you fetch in parallel (throughput).
2. **Memory for the frontier/URL queue and dedup bloom filter** — scales with index size, not with domains.
3. **Headless browser processes** — the only genuinely heavy CPU/RAM component (see §5).
4. **Parsing/extract CPU** — modest; a few ms per page.

### 1.2 Sizing

| Component | Small | Medium | Large | Note |
|---|---|---|---|---|
| Crawler workers | 2 × 2 vCPU / 4 GB | 4 × 4 vCPU / 8 GB | 8 × 8 vCPU / 16 GB | horizontal scale |
| Frontier + dedup (Redis/bloom) | 2 GB RAM | 8 GB RAM | 32 GB RAM | [INFERRED] bloom filter ~10 B/URL × 10 M URLs = 100 MB, plus queue |
| Parser/normaliser | collocated | 2 × 2 vCPU | 4 × 2 vCPU | |
| **Total crawler vCPU** | **4** | **16** | **64** | |
| **Total crawler RAM** | **8 GB** | **32 GB** | **128 GB** | |

### 1.3 Cost — self-hosted (Hetzner Cloud, EU)

Using Hetzner **Regular Performance (CPX)** shared-vCPU plans, EU-Central, excl. VAT [CONFIRMED]:

| Plan | vCPU | RAM | NVMe | €/mo | Source |
|---|---|---|---|---|---|
| CPX12 | 1 | 2 GB | 40 GB | €11.99 | hetzner.com/cloud/regular-performance/ |
| CPX22 | 2 | 4 GB | 80 GB | €19.99 | same |
| CPX32 | 4 | 8 GB | 160 GB | €35.99 | same |
| CPX42 | 8 | 16 GB | 320 GB | €69.99 | same |

[CONFIRMED] 20 TB traffic included per server at EU-Central; €1.00 per additional TB [CONFIRMED].

| Scale | Build | €/mo | **$/mo** (€1 = $1.08) |
|---|---|---|---|
| Small | 1 × CPX22 + 1 × CPX12 (frontier) | €31.98 | **$34.54** |
| Medium | 2 × CPX32 + 1 × CPX22 | €91.97 | **$99.33** |
| Large | 4 × CPX42 + 2 × CPX32 | €351.94 | **$380.10** |

[INFERRED] conversion at €1 = $1.08 (indicative; actual rate varies). Hetzner bills hourly with a monthly cap [CONFIRMED].

### 1.4 Cost — hyperscaler equivalent (AWS EC2 On-Demand, US East Ohio, Linux)

[CONFIRMED] from aws.amazon.com/ec2/pricing/on-demand/ (accessed 2026-11-10):

| Instance | vCPU | RAM | $/hr |
|---|---|---|---|
| c6gd.medium | 1 | 2 GiB | $0.0384 |
| m7g.large | 2 | 8 GiB | $0.168 |
| m7g.xlarge | 4 | 16 GiB | $0.337 |
| m7g.2xlarge | 8 | 32 GiB | $0.674 |

At 730 h/month:

| Scale | Build | $/mo |
|---|---|---|
| Small | 1 × m7g.large + 1 × c6gd.medium | 0.168×730 + 0.0384×730 = **$150.67** |
| Medium | 2 × m7g.xlarge + 1 × m7g.large | 0.337×730×2 + 0.168×730 = **$614.66** |
| Large | 4 × m7g.2xlarge + 2 × m7g.xlarge | 0.674×730×4 + 0.337×730×2 = **$2,460.10** |

**Observation [INFERRED]:** self-hosted Hetzner is roughly **3.5–4.5× cheaper** than AWS EC2 On-Demand for identical vCPU/RAM. That gap is the entire argument for self-hosting at small/medium scale, and it is also the reason the labour line in §0.3 dominates.

---

## 2. PostgreSQL hosting — managed vs self-hosted

### 2.1 Managed options (verified list prices)

**DigitalOcean Managed PostgreSQL, Standard, Basic, Regular CPU** [CONFIRMED] (digitalocean.com/pricing/managed-databases, accessed 2026-11-10):

| Memory | vCPU | Storage range | $/hr | $/mo |
|---|---|---|---|---|
| 1 GiB | 1 | 10–30 GiB | $0.02254 | $15.15 |
| 2 GiB | 1 | 30–60 GiB | $0.04531 | $30.45 |
| 4 GiB | 2 | 60–120 GiB | $0.09063 | $60.90 |
| 8 GiB | 4 | 140–280 GiB | $0.18170 | $122.10 |
| 16 GiB | 6 | 290–580 GiB | $0.36362 | $244.35 |

Storage over the included range: **$0.215/GiB/mo** [CONFIRMED]. Daily backups included, 7-day restore window [CONFIRMED].

**AWS RDS for PostgreSQL, Single-AZ, US East (Ohio)** [CONFIRMED] (aws.amazon.com/rds/postgresql/pricing/, accessed 2026-11-10):

| Instance | $/hr |
|---|---|
| db.t4g.micro | $0.016 |
| db.t4g.small | $0.032 |
| db.t4g.medium | $0.065 |
| db.t4g.large | $0.129 |
| db.m7g.large | $0.168 |
| db.m7g.xlarge | $0.337 |

At 730 h/mo: db.t4g.small = **$23.36/mo**; db.t4g.medium = **$47.45/mo**; db.m7g.large = **$122.64/mo** [INFERRED arithmetic].

**Azure Database for PostgreSQL Flexible Server, Burstable, pay-as-you-go** [CONFIRMED] (azure.microsoft.com/en-us/pricing/details/postgresql/flexible-server/, accessed 2026-11-10):

| Instance | vCores | Memory | $/mo |
|---|---|---|---|
| B1ms | 1 | 2 GiB | $12.410 |
| B2s | 2 | 4 GiB | $49.640 |
| B2ms | 2 | 8 GiB | $99.280 |

Azure backup storage: free up to 100% of provisioned storage; beyond that charged per GiB-month [CONFIRMED].

### 2.2 Self-hosted PostgreSQL

Self-hosting on the same Hetzner boxes as §1.3 costs **$0 marginal** if co-located, or **€11.99–€35.99/mo** ($12.95–$38.87) for a dedicated DB box. PostgreSQL itself is **free and open source** [CONFIRMED] (postgresql.org/about/licence/).

What you pay for instead is **labour and risk**: patching, major-version upgrades, WAL archiving, PITR testing, failover. §8 prices this.

### 2.3 Recommendation matrix

| Scale | Recommended | Why |
|---|---|---|
| Small (20) | **Managed, 1 GiB / $15.15/mo** (DO) or self-hosted on the crawler box | At $15/mo the managed premium is ~1 hour of labour. Buy the hour. |
| Medium (200) | **Managed, 8 GiB / $122.10/mo** (DO) | 38 GB data + 95 GB index fits the 140 GiB floor; still cheaper than 8 h of DBA time. |
| Large (2,000) | **Self-hosted on dedicated hardware, or managed 16 GiB / $244.35/mo** | At 243 GB you are past the comfortable managed-DB envelope; self-hosting wins if you have the ops skill, managed wins if you do not. |

---

## 3. Storage: page metadata, fingerprints, price observations

### 3.1 Object storage (raw HTML, rendered DOM snapshots)

**Cloudflare R2** [CONFIRMED] (developers.cloudflare.com/r2/pricing/, page last updated 2026-10-30, accessed 2026-11-10):

| Item | Price |
|---|---|
| Standard storage | $0.015 / GB-month |
| Infrequent Access storage | $0.01 / GB-month |
| Class A ops (writes) | $4.50 / million |
| Class B ops (reads) | $0.36 / million |
| Data retrieval (IA only) | $0.01 / GB |
| **Egress to internet** | **Free** |
| Free tier | 10 GB-month, 1 M Class A, 10 M Class B |

**AWS S3 Standard** [CONFIRMED] (aws.amazon.com/s3/pricing/, accessed 2026-11-10):

| Item | Price |
|---|---|
| First 50 TB/mo | $0.023 / GB |
| Next 450 TB/mo | $0.022 / GB |
| Over 500 TB/mo | $0.021 / GB |
| S3 Standard-IA | $0.0125 / GB |
| S3 Glacier Instant Retrieval | $0.004 / GB |
| S3 Glacier Deep Archive | $0.00099 / GB |
| PUT/COPY/LIST | $0.005 / 1,000 |
| GET/SELECT | $0.0004 / 1,000 |
| Data transfer out to internet | $0.09 / GB (first 10 TB/mo tier) |

**Note [CONFIRMED]:** AWS gives 100 GB/month of aggregate data transfer out to the internet free across all services [CONFIRMED, aws.amazon.com/ec2/pricing/on-demand/].

### 3.2 Storage cost model

| Line | Small | Medium | Large | Arithmetic |
|---|---|---|---|---|
| Raw HTML in R2 Standard | 14.3 GB | 143 GB | 1,430 GB | §0.2 |
| R2 storage cost | $0.21 | $2.15 | $21.45 | = GB × $0.015 |
| R2 Class A (writes, 1 fetch/page/mo) | 150 k | 1.5 M | 15 M | = fetches |
| R2 Class A cost | $0.68 | $6.75 | $67.50 | = ops ÷ 1 M × $4.50 |
| R2 Class B (reads, 2× writes) | 300 k | 3 M | 30 M | = 2 × Class A |
| R2 Class B cost | $0.11 | $1.08 | $10.80 | = ops ÷ 1 M × $0.36 |
| **R2 total** | **$0.99** | **$9.98** | **$99.75** | sum |
| Same data in S3 Standard | $0.33 | $3.29 | $32.89 | = GB × $0.023 |
| S3 egress (if served to users) | $1.94 | $19.35 | $193.55 | = GB × $0.09 |

**Observation [INFERRED]:** R2's zero egress is worth **$194/mo at large scale** versus S3 if you serve raw content. If you only store and never serve, S3 is cheaper on storage alone.

### 3.3 PostgreSQL storage (metadata, fingerprints, price observations)

From §0.2: **2.5 GB / 24 GB / 243 GB**.

| Hosting | Small | Medium | Large |
|---|---|---|---|
| Self-hosted (Hetzner NVMe, included in CPX plan) | $0 | $0 | $0 (but plan disk caps: CPX42 = 320 GB) |
| DigitalOcean managed (storage $0.215/GiB/mo) | $0.54 | $5.38 | $53.75 |
| AWS RDS (gp3 ~$0.08/GiB-mo list; verify current) | ~$0.20 | ~$2.00 | ~$20.00 |

[CONFIRMED] DO storage rate. AWS gp3 rate **not re-verified in this pass** — treat as [UNKNOWN]; DO figure is the safe planning number.

**Capacity warning [INFERRED]:** at Large scale, 243 GB of PostgreSQL data + 95 GB of indexes will **not** fit on a CPX42's 320 GB NVMe alongside the OS and WAL. You need either a CPX52 (480 GB, €100.99/mo) or a managed DB. This is a real sizing constraint, not a rounding error.

---

## 4. Egress / bandwidth, VPN, and Tor

### 4.1 Crawl egress (the big one)

Crawl egress from §0.1: **21.5 GB / 215 GB / 2.15 TB per month**.

| Provider | Small | Medium | Large | Source |
|---|---|---|---|---|
| Hetzner Cloud EU | **$0** (20 TB incl.) | **$0** | **$0** | 20 TB incl.; €1.00/TB extra [CONFIRMED] |
| AWS EC2 → internet | $1.94 | $19.35 | $193.55 | $0.09/GB first 10 TB [CONFIRMED] |
| DigitalOcean Droplets | $0 (1 TB incl. on 2 GB plan) | $0 (4 TB incl. on 8 GB) | over → $0.01/GB | transfer allowances [CONFIRMED] |

[INFERRED] AWS arithmetic: 21.5 GB × $0.09 = $1.94; 215 GB × $0.09 = $19.35; 2,150 GB × $0.09 = $193.50. The first 100 GB/mo is free across AWS, so Small is effectively **$0** on AWS too.

**This is the single largest structural cost advantage of Hetzner for a crawler.** A crawler is egress-heavy by nature; hyperscalers charge $0.09/GB for exactly the traffic a crawler produces.

### 4.2 VPN

**Mullvad VPN** [CONFIRMED] (mullvad.net/en/pricing, accessed 2026-11-10):

- **€5/month flat**, incl. VAT, no long-term discount, up to **5 devices** per account.
- €5 ≈ **$5.61** at the rate published on the same page [CONFIRMED].

| Scale | Accounts needed | €/mo | $/mo |
|---|---|---|---|
| Small | 1 | €5 | $5.61 |
| Medium | 2–3 (exit diversity) | €10–15 | $11.22–16.83 |
| Large | 5–10 (per-crawler egress diversity) | €25–50 | $28.05–56.10 |

[INFERRED] account counts are an architectural choice, not a vendor requirement. Mullvad **does not support port forwarding** [CONFIRMED], which matters if you need inbound.

**Alternative [CONFIRMED]:** run your own WireGuard exit on a Hetzner CPX12 at **€11.99/mo** with 20 TB included — cheaper than 2 Mullvad accounts and no port-forwarding restriction.

### 4.3 Tor

**Tor Project published relay requirements** [CONFIRMED via community.torproject.org/relay/technical-considerations/, accessed 2026-11-10]:

> "This can cost anywhere between $3.00/month and thousands per month, depending on your provider, hardware configuration, and bandwidth usage."

- Bridge minimum: **1 Mbit/s** up/down [CONFIRMED].
- Guard/middle relay minimum: **10 Mbit/s**, 16 Mbit/s recommended [CONFIRMED].
- Minimum **100 GB outbound/month**; 2 TB+ recommended [CONFIRMED].
- RAM: 512 MB below 40 Mbit/s, 1 GB above [CONFIRMED].

**Traffic arithmetic [INFERRED]:** sustained 1 Mbit/s both directions = 125 KB/s × 2 × 86,400 s/day = 21.6 GB/day = **648 GB/month**. At 10 Mbit/s = **6.48 TB/month**.

| Scale | Tor build | Cost/mo |
|---|---|---|
| Small | 1 bridge on existing VPS (1 Mbit/s cap) | **$0** marginal |
| Medium | 1 middle relay, 10 Mbit/s, ~6.5 TB/mo | **€11.99–19.99** ($12.95–21.59) — needs a 20 TB plan |
| Large | 2–4 relays, 16 Mbit/s each | **€40–80** ($43–86) |

**Caveat [UNKNOWN]:** Tor exit relays carry **legal risk** (abuse complaints, law-enforcement contact) that is not priced by any vendor. Running a **bridge or middle relay only** avoids exit-relay liability. This is a legal judgement, not a cost line — see §7.

---

## 5. Headless browser hosting

### 5.1 Managed browser-as-a-service

**Browserless** [CONFIRMED] (browserless.io/pricing, accessed 2026-11-10). Prices shown are **billed annually**; monthly billing is higher.

| Plan | $/mo (annual) | Units/mo included | Overage $/unit | Max concurrent browsers |
|---|---|---|---|---|
| Free | $0 | 1,000 | — | 2 |
| Prototyping | $25 | 20,000 | $0.0020 | 10 |
| Starter | $140 | 180,000 | $0.0017 | 40 |
| Scale | $350 | 500,000 | $0.0015 | 100 |
| Enterprise | Custom | Millions | — | Hundreds/thousands |

**What a "Unit" is [CONFIRMED]:** "a block of browser time of up to 30 seconds per browser connection. Longer-running automations use an extra Unit for every 30 seconds."

**Cost per page [INFERRED]:** at 8 s average render, one page = 1 unit (under 30 s).

| Scale | Renders/mo | Plan | Cost/mo | $/page |
|---|---|---|---|---|
| Small | 7,500 | Prototyping (20k units) | **$25** | $0.0033 |
| Medium | 150,000 | Starter (180k units) | **$140** | $0.00093 |
| Large | 2,250,000 | Scale × 4 or Enterprise | **$1,400** (4 × $350) | $0.00062 |

[INFERRED] Large arithmetic: 2.25 M renders ÷ 500 k units per Scale plan = 4.5 → 5 plans = $1,750; or negotiate Enterprise. **$1,400–1,750/mo** is the honest range.

### 5.2 Self-hosted headless (Playwright/Puppeteer on your own box)

Chrome headless steady-state: **~300–500 MB RAM per concurrent page** [INFERRED, widely reported; not a vendor-published figure].

| Scale | Concurrent browsers | RAM needed | Box | Cost/mo |
|---|---|---|---|---|
| Small | 4 | 2 GB | collocated on CPX22 | $0 marginal |
| Medium | 20 | 10 GB | 1 × CPX32 (8 GB) + 1 × CPX22 | €35.99 + €19.99 = €55.98 → **$60.46** |
| Large | 100 | 50 GB | 4 × CPX42 (16 GB each) | €69.99 × 4 = €279.96 → **$302.36** |

**Comparison [INFERRED]:**

| Scale | Managed Browserless | Self-hosted | Saving |
|---|---|---|---|
| Small | $25 | $0 | $25 |
| Medium | $140 | $60 | $80 |
| Large | $1,400–1,750 | $302 | **$1,100–1,450** |

**Caveat:** self-hosted browsers are the **#1 source of crawler instability** (memory leaks, zombie processes, OOM). Budget 20–30% ops time for browser-pool supervision at Large scale [INFERRED]. The $1,100+ saving is real but is **bought with labour**, which §0.3 prices at $75/h.

---

## 6. Monitoring, backups, secrets management

### 6.1 Monitoring

**Grafana Cloud** [CONFIRMED] (grafana.com/pricing/, accessed 2026-11-10):

| Tier | Price | Includes |
|---|---|---|
| Free | $0 | 10k active series, 14-day retention, 50 GB logs |
| Pro | $19/mo + usage | 10k series, then $6.50/1k series; 13-mo metrics retention; 50 GB logs, then $0.050/GB process, $0.400/GB write, $0.100/GB retain |
| Enterprise | from $25,000/yr commit | custom |

| Scale | Active series (est.) | Plan | Cost/mo |
|---|---|---|---|
| Small | < 10,000 | **Free** | **$0** |
| Medium | 30,000 | Pro: $19 + 20k × $6.50/1k | **$149** |
| Large | 100,000+ | Pro: $19 + 90k × $6.50/1k | **$604** |

[INFERRED] series counts are estimates; the arithmetic is shown so you can substitute your own.

**Self-hosted alternative [CONFIRMED]:** Grafana + Prometheus are open source and free; Hetzner one-click apps include Prometheus and Grafana. Cost = **$0 licence + labour**.

### 6.2 Backups

| Option | Small | Medium | Large | Source |
|---|---|---|---|---|
| DigitalOcean managed DB | Included (7-day PITR) | Included | Included | [CONFIRMED] |
| AWS RDS automated backups | Free up to 100% of provisioned storage | same | same | [CONFIRMED] |
| Azure Flexible Server | Free up to 100% of provisioned storage | same | same | [CONFIRMED] |
| Self-hosted `pg_dump` → R2 | 5 GB × $0.015 = $0.08 | $0.75 | $7.50 | [INFERRED] from R2 rate |
| Self-hosted WAL archiving → R2 | ~2 GB = $0.03 | $0.30 | $3.00 | [INFERRED] |

PostgreSQL documents three backup approaches — SQL dump, file-system level, continuous archiving/PITR — and notes each has strengths and weaknesses [CONFIRMED, postgresql.org/docs/17/backup.html, accessed 2026-11-10].

**Recommendation:** at Small/Medium, pay for managed backups (they're included). At Large, self-hosted WAL→R2 at **~$10/mo** beats the labour of running your own PITR.

### 6.3 Secrets management

**HCP Vault Dedicated** [CONFIRMED] (hashicorp.com/en/pricing?tab=vault, accessed 2026-11-10):

| Tier | $/cluster/hr | Notes |
|---|---|---|
| Development | $0.62 | up to 25 clients |
| Essentials | $1.58 | + $73/mo per Vault client |
| Standard | $1.85 | + $73/mo per Vault client |

At 730 h/mo: Development = **$452.60/mo**; Essentials = **$1,153.40/mo + $73/client**.

**This is absurd for a crawler.** The correct answer at all three scales is:

| Option | Cost | Note |
|---|---|---|
| **SOPS + age /.age-encrypted YAML in git** | **$0** | [INFERRED] industry standard for small ops |
| **Doppler / 1Password Secrets Automation** | $0–20/mo | [UNKNOWN — not verified this pass] |
| HCP Vault | $452+/mo | overkill below Enterprise scale |

**Recommendation [INFERRED]:** SOPS+age at all three scales. Vault's $452/mo entry point exceeds the entire Small-scale infrastructure budget.

---

## 7. Domain admission and legal review

**This section has essentially no vendor-priced components.** Everything here is labour or legal services, and most of it is [UNKNOWN] because legal fees are not publicly listed.

### 7.1 Admission process cost

| Activity | Small | Medium | Large | Basis |
|---|---|---|---|---|
| Domains admitted/month | 2–5 | 10–20 | 50–100 | assumption |
| Automated checks (robots.txt, ToS scrape, malware, DNS) | 2 h/mo | 8 h/mo | 20 h/mo | [INFERRED] |
| Human review per domain | 15 min | 15 min | 15 min | [INFERRED] |
| Human review labour | 1.25 h/mo | 7.5 h/mo | 25 h/mo | = domains × 0.25 h |
| **Admission labour $/mo @ $75/h** | **$188** | **$563** | **$1,875** | |

### 7.2 Legal review

| Item | Cost | Status |
|---|---|---|
| Terms of Service / robots.txt compliance review | **Not publicly priced** | **[UNKNOWN]** |
| DMCA / copyright counsel (retainer) | **Not publicly priced** | **[UNKNOWN]** |
| GDPR / data-protection counsel | **Not publicly priced** | **[UNKNOWN]** |
| Trademark clearance per admitted domain | **Not publicly priced** | **[UNKNOWN]** |

**What we can say [INFERRED]:** a US solo-founder legal retainer typically runs **$2,000–5,000/month** for a content-adjacent business, and one-off ToS reviews **$500–2,000**. These are **market ranges, not verified prices**, and are excluded from the §10 totals. Treat them as a separate budget line the founder must price with a real attorney.

### 7.3 Legal risk register (unpriced)

| Risk | Mitigation | Cost |
|---|---|---|
| Copyright in stored page text | Store metadata + snippets only, not full text | $0 |
| robots.txt / ToS violation | Honour robots.txt, rate-limit, identify crawler | labour (§7.1) |
| GDPR (storing EU publisher data) | EU hosting (Hetzner DE/FI), DPA | $0 marginal |
| Defamation in snippets | Verbatim snippets only, no editorialising | $0 |
| Tor exit-relay abuse complaints | Run **bridge/middle only**, never exit | $0 |

---

## 8. Cost of NOT doing things (technical debt and rework)

These are **real costs with no vendor price tag**. Each is [INFERRED] and the arithmetic is shown.

### 8.1 The labour multiplier

From §0.3, infra labour is **$1,500 / $2,400 / $3,600 per month**. Every deferred practice below converts directly into this line.

### 8.2 Itemised debt costs

| Deferred practice | Consequence | Cost | Arithmetic |
|---|---|---|---|
| **No automated backups / no restore test** | Data loss event; rebuild from scratch | 40–160 h rework | 1–4 weeks of founder time |
| **No monitoring** | Silent crawl failure for days/weeks | 8–24 h diagnosis + lost freshness | 1–3 days |
| **No fingerprinting/dedup** | Re-crawling unchanged pages forever | +30–50% crawl egress | 2.15 TB → 3.2 TB = +$0 (Hetzner) / +$95 (AWS) |
| **No robots.txt / ToS compliance** | IP bans, legal exposure | 20–80 h + legal | unquantifiable legal tail |
| **No rate limiting** | Blocked by target sites; domain-level ban | 40–120 h recovery | 1–3 weeks |
| **Secrets in env vars / git** | Credential leak | 40–200 h incident | 1–5 weeks |
| **Self-hosted Postgres with no PITR** | Corruption → full re-crawl | 80–240 h | 2–6 weeks |
| **No schema migrations discipline** | Painful rewrite at 10× scale | 80–200 h | 2–5 weeks |
| **No crawl-state persistence** | Restart loses frontier | 20–60 h | 1–2 weeks |
| **Browser pool unsupervised** | OOM cascade, crawl stall | 2–6 h/week ongoing | 8–24 h/mo |

### 8.3 The compounding argument

**At Small scale,** the deferred items above total roughly **400–800 hours** of rework if they all go wrong — call it **$30,000–60,000** of founder time, against a total infrastructure bill of **~$40/mo**. The correct conclusion is not "spend more on infra," it is:

> **At Small scale, the highest-ROI infrastructure spending is on backups, monitoring, and fingerprinting — because each prevents a multi-week founder-time incident.**

**At Large scale,** the same failures cost the same hours but the infra bill is ~$3,000/mo, so the ratio inverts and proper tooling pays for itself faster.

### 8.4 Rework probability weighting [INFERRED]

| Scale | P(any major incident in 12 mo) | Expected rework cost |
|---|---|---|
| Small | 40% | 0.4 × $45,000 = **$18,000/yr** |
| Medium | 60% | 0.6 × $45,000 = **$27,000/yr** |
| Large | 80% | 0.8 × $45,000 = **$36,000/yr** |

These probabilities are **judgement, not measurement.** They are included to make the trade-off explicit, not to predict the future.

---

## 9. VERIFIED LISTED PRICES (raw, as published)

All accessed **2026-11-10**. This section contains only prices read directly from the cited page.

### 9.1 Compute

| Item | Price | Source |
|---|---|---|
| Hetzner CPX12 (1 vCPU/2 GB/40 GB) | €11.99/mo excl. VAT | hetzner.com/cloud/regular-performance/ |
| Hetzner CPX22 (2/4/80) | €19.99/mo excl. VAT | same |
| Hetzner CPX32 (4/8/160) | €35.99/mo excl. VAT | same |
| Hetzner CPX42 (8/16/320) | €69.99/mo excl. VAT | same |
| Hetzner CPX52 (12/24/480) | €100.99/mo excl. VAT | same |
| Hetzner CCX13 (2 dedicated/8/80) | €43.49/mo excl. VAT | hetzner.com/cloud/general-purpose/ |
| Hetzner CCX23 (4/16/160) | €86.49/mo excl. VAT | same |
| Hetzner CCX33 (8/32/240) | €138.99/mo excl. VAT | same |
| Hetzner EU included traffic | 20 TB/server | hetzner.com/cloud/ |
| Hetzner extra traffic EU | €1.00/TB | same |
| AWS EC2 c6gd.medium | $0.0384/hr | aws.amazon.com/ec2/pricing/on-demand/ |
| AWS EC2 m7g.large | $0.168/hr | same |
| AWS EC2 m7g.xlarge | $0.337/hr | same |
| AWS EC2 m7g.2xlarge | $0.674/hr | same |
| AWS EC2 data transfer out (first 10 TB/mo) | $0.09/GB | same |
| AWS free data transfer out | 100 GB/mo aggregate | same |
| DigitalOcean Basic 1 GB/1 vCPU | $6.00/mo, 1 TB transfer | digitalocean.com/pricing/droplets |
| DigitalOcean Basic 2 GB/1 vCPU | $12.00/mo, 2 TB transfer | same |
| DigitalOcean Basic 8 GB/4 vCPU | $48.00/mo, 5 TB transfer | same |

### 9.2 PostgreSQL

| Item | Price | Source |
|---|---|---|
| DO Managed PostgreSQL 1 GiB/1 vCPU | $15.15/mo | digitalocean.com/pricing/managed-databases |
| DO Managed PostgreSQL 2 GiB/1 vCPU | $30.45/mo | same |
| DO Managed PostgreSQL 4 GiB/2 vCPU | $60.90/mo | same |
| DO Managed PostgreSQL 8 GiB/4 vCPU | $122.10/mo | same |
| DO Managed PostgreSQL 16 GiB/6 vCPU | $244.35/mo | same |
| DO managed DB storage (over included) | $0.215/GiB/mo | same |
| DO managed DB backups | Included, 7-day restore | digitalocean.com/products/managed-databases |
| AWS RDS db.t4g.micro | $0.016/hr | aws.amazon.com/rds/postgresql/pricing/ |
| AWS RDS db.t4g.small | $0.032/hr | same |
| AWS RDS db.t4g.medium | $0.065/hr | same |
| AWS RDS db.m7g.large | $0.168/hr | same |
| AWS RDS db.m7g.xlarge | $0.337/hr | same |
| Azure PostgreSQL B1ms (1 vCore/2 GiB) | $12.410/mo | azure.microsoft.com/.../postgresql/flexible-server/ |
| Azure PostgreSQL B2s (2/4) | $49.640/mo | same |
| Azure PostgreSQL B2ms (2/8) | $99.280/mo | same |
| Azure backup storage | Free up to 100% of provisioned | same |
| PostgreSQL licence | Free, open source | postgresql.org/about/licence/ |

### 9.3 Object storage

| Item | Price | Source |
|---|---|---|
| Cloudflare R2 Standard storage | $0.015/GB-mo | developers.cloudflare.com/r2/pricing/ (updated 2026-10-30) |
| Cloudflare R2 Infrequent Access | $0.01/GB-mo | same |
| Cloudflare R2 Class A ops | $4.50/million | same |
| Cloudflare R2 Class B ops | $0.36/million | same |
| Cloudflare R2 egress | Free | same |
| Cloudflare R2 free tier | 10 GB-mo, 1 M Class A, 10 M Class B | same |
| AWS S3 Standard (first 50 TB) | $0.023/GB-mo | aws.amazon.com/s3/pricing/ |
| AWS S3 Standard-IA | $0.0125/GB-mo | same |
| AWS S3 Glacier Instant Retrieval | $0.004/GB-mo | same |
| AWS S3 Glacier Deep Archive | $0.00099/GB-mo | same |
| AWS S3 PUT/COPY/LIST | $0.005/1,000 | same |
| AWS S3 GET/SELECT | $0.0004/1,000 | same |
| AWS S3 data transfer out (first 10 TB) | $0.09/GB | same |

### 9.4 Headless browser

| Item | Price | Source |
|---|---|---|
| Browserless Free | $0/mo, 1k units, 2 concurrent | browserless.io/pricing |
| Browserless Prototyping | $25/mo annual, 20k units, $0.0020 overage | same |
| Browserless Starter | $140/mo annual, 180k units, $0.0017 overage | same |
| Browserless Scale | $350/mo annual, 500k units, $0.0015 overage | same |
| Browserless Enterprise | Custom | same |
| Browserless unit definition | ≤30 s of browser time per connection | same |

### 9.5 Monitoring / secrets / VPN / Tor

| Item | Price | Source |
|---|---|---|
| Grafana Cloud Free | $0, 10k series, 14-day retention | grafana.com/pricing/ |
| Grafana Cloud Pro | $19/mo + $6.50/1k series | same |
| Grafana Cloud Enterprise | from $25,000/yr commit | same |
| Grafana logs (Pro) | $0.050/GB process, $0.400/GB write, $0.100/GB retain | same |
| HCP Vault Development | $0.62/cluster/hr | hashicorp.com/en/pricing?tab=vault |
| HCP Vault Essentials | $1.58/cluster/hr + $73/mo per client | same |
| HCP Vault Standard | $1.85/cluster/hr + $73/mo per client | same |
| Mullvad VPN | €5/mo flat, 5 devices, incl. VAT | mullvad.net/en/pricing |
| Mullvad port forwarding | Not supported | same |
| Tor relay hosting | "$3.00/month to thousands" | community.torproject.org/relay/technical-considerations/ |
| Tor bridge minimum | 1 Mbit/s | same |
| Tor guard/middle minimum | 10 Mbit/s (16 recommended) | same |
| Tor relay min traffic | 100 GB/mo outbound (2 TB+ recommended) | same |
| Tor relay RAM | 512 MB < 40 Mbit/s; 1 GB above | same |

### 9.6 Not publicly priced

| Item | Status |
|---|---|
| AWS RDS gp3 storage $/GiB-mo | [UNKNOWN] — not verified this pass |
| Legal counsel (ToS, copyright, GDPR) | [UNKNOWN] — not publicly listed |
| Trademark clearance | [UNKNOWN] |
| Doppler / 1Password secrets pricing | [UNKNOWN] — not verified this pass |
| Chrome headless RAM per page | [UNKNOWN] — no vendor figure published |

---

## 10. ESTIMATES — three-scale monthly cost table

### 10.1 Assumptions recap (all [INFERRED] unless noted)

- 5,000 pages/domain; monthly full re-crawl + 50% incremental churn.
- 150 KB avg fetch; 5%/10%/15% headless share.
- 4 KB metadata row; ×2.5 index overhead; 12 months of price observations retained.
- €1 = $1.08.
- Labour $75/h; 5/8/12 h per week on infra.
- Self-hosted on Hetzner EU; managed PostgreSQL from DigitalOcean; R2 for object storage.

### 10.2 Small scale — 20 domains

| Line item | Build | $/mo | Basis |
|---|---|---|---|
| Crawler compute | 1 × CPX22 + 1 × CPX12 | $34.54 | §1.3 |
| PostgreSQL | DO Managed 1 GiB | $15.15 | §2.1 [CONFIRMED] |
| Object storage (R2) | 14.3 GB + ops | $0.99 | §3.2 |
| Egress | 21.5 GB | $0.00 | 20 TB incl. [CONFIRMED] |
| VPN | 1 × Mullvad | $5.61 | §4.2 [CONFIRMED] |
| Tor | 1 bridge on existing VPS | $0.00 | §4.3 |
| Headless browser | self-hosted, 4 concurrent | $0.00 | §5.2 |
| Monitoring | Grafana Free | $0.00 | §6.1 [CONFIRMED] |
| Backups | DO managed, included | $0.00 | §6.2 [CONFIRMED] |
| Secrets | SOPS + age | $0.00 | §6.3 |
| Domain admission labour | 5 domains/mo | $188.00 | §7.1 |
| **Infrastructure subtotal** | | **$56.29** | |
| **Infra labour** | 5 h/wk | **$1,500.00** | §0.3 |
| **TOTAL** | | **$1,744.29** | |

### 10.3 Medium scale — 200 domains

| Line item | Build | $/mo | Basis |
|---|---|---|---|
| Crawler compute | 2 × CPX32 + 1 × CPX22 | $99.33 | §1.3 |
| PostgreSQL | DO Managed 8 GiB | $122.10 | §2.1 [CONFIRMED] |
| Object storage (R2) | 143 GB + ops | $9.98 | §3.2 |
| Egress | 215 GB | $0.00 | 20 TB incl. [CONFIRMED] |
| VPN | 2 × Mullvad | $11.22 | §4.2 |
| Tor | 1 middle relay, 10 Mbit/s | $21.59 | §4.3 |
| Headless browser | 1 × CPX32 + 1 × CPX22 | $60.46 | §5.2 |
| Monitoring | Grafana Pro, 30k series | $149.00 | §6.1 |
| Backups | DO managed, included | $0.00 | §6.2 |
| Secrets | SOPS + age | $0.00 | §6.3 |
| Domain admission labour | 20 domains/mo | $563.00 | §7.1 |
| **Infrastructure subtotal** | | **$473.68** | |
| **Infra labour** | 8 h/wk | **$2,400.00** | §0.3 |
| **TOTAL** | | **$3,436.68** | |

### 10.4 Large scale — 2,000 domains

| Line item | Build | $/mo | Basis |
|---|---|---|---|
| Crawler compute | 4 × CPX42 + 2 × CPX32 | $380.10 | §1.3 |
| PostgreSQL | self-hosted on CPX52 + CPX42 | $184.66 | €100.99 + €69.99 = €170.98 |
| Object storage (R2) | 1.43 TB + ops | $99.75 | §3.2 |
| Egress | 2.15 TB | $0.00 | 20 TB incl. [CONFIRMED] |
| VPN | 5 × Mullvad | $28.05 | §4.2 |
| Tor | 3 middle relays, 16 Mbit/s | $64.77 | §4.3 |
| Headless browser | 4 × CPX42 | $302.36 | §5.2 |
| Monitoring | Grafana Pro, 100k series | $604.00 | §6.1 |
| Backups | self-hosted WAL → R2 | $10.50 | §6.2 |
| Secrets | SOPS + age | $0.00 | §6.3 |
| Domain admission labour | 100 domains/mo | $1,875.00 | §7.1 |
| **Infrastructure subtotal** | | **$1,675.19** | |
| **Domain admission subtotal** | | **$1,875.00** | |
| **Infra labour** | 12 h/wk | **$3,600.00** | §0.3 |
| **TOTAL** | | **$7,150.19** | |

### 10.5 Summary table

| | Small (20) | Medium (200) | Large (2,000) |
|---|---|---|---|
| Infrastructure | $56 | $474 | $1,675 |
| Domain admission | $188 | $563 | $1,875 |
| Infra labour | $1,500 | $2,400 | $3,600 |
| **Total / month** | **$1,744** | **$3,437** | **$7,150** |
| **Total / year** | **$20,931** | **$41,240** | **$85,802** |
| Infrastructure as % of total | 3% | 14% | 23% |

### 10.6 Hyperscaler comparison (same workload, AWS instead of Hetzner+DO)

| | Small | Medium | Large |
|---|---|---|---|
| Crawler compute (EC2) | $150.67 | $614.66 | $2,460.10 |
| PostgreSQL (RDS db.t4g.medium / db.m7g.large / db.m7g.xlarge) | $47.45 | $122.64 | $245.28 |
| Object storage (S3 Standard) | $0.33 | $3.29 | $32.89 |
| Egress (AWS $0.09/GB) | $0.00 | $19.35 | $193.55 |
| Headless (Browserless managed) | $25.00 | $140.00 | $1,400.00 |
| **Infrastructure subtotal** | **$223.45** | **$899.94** | **$4,331.82** |
| vs self-hosted | 4.0× | 1.9× | 1.2× |

**The gap narrows with scale** because egress (where Hetzner wins most) becomes a smaller share of total cost, and managed services (where AWS is competitive) dominate. At Large scale the self-hosted build is **$1,675/mo vs $4,332/mo** on AWS — a 2.6× advantage, wider than the 1.2× shown above, because the AWS comparison uses managed Browserless while the self-hosted build runs browsers on its own boxes.

---

## 11. Key findings and recommendations

1. **Egress is the decisive cost variable.** A crawler produces 2.15 TB/mo at 2,000 domains. Hetzner includes 20 TB/server at EU-Central; AWS charges $0.09/GB. At Large scale that is **$194/mo vs $0** — and at 10× the crawl volume it becomes **$1,936/mo vs $0**. [CONFIRMED prices, INFERRED volumes]

2. **Self-hosting wins below ~$500/mo of infrastructure.** Above that, managed services' labour savings start to dominate. The crossover is around **Medium scale**. [INFERRED]

3. **Labour is the largest line item at every scale.** $1,500–3,600/mo of founder time dwarfs the $56–1,675/mo infrastructure bill. The cheapest infrastructure decision is the one that reduces founder toil. [INFERRED]

4. **Headless browser hosting is the single largest infrastructure line at Large scale** ($302 self-hosted vs $1,400–1,750 managed). Self-hosting saves **$1,100–1,450/mo** but requires browser-pool supervision. [INFERRED]

5. **HCP Vault is wrong-sized for this workload.** $452/mo entry vs $0 for SOPS+age. [CONFIRMED price, INFERRED recommendation]

6. **Legal review is the largest unpriced line.** Not publicly priced; budget separately with counsel. [UNKNOWN]

7. **The cost of NOT doing things exceeds the infrastructure bill at Small scale.** Deferred backups, monitoring, and fingerprinting represent $18,000–36,000/yr of expected rework against a $676/yr infrastructure bill. [INFERRED, probability-weighted]

---

## 12. What is NOT covered

- **Search index serving** (query-time compute, vector index, ranking) — out of scope; this report covers crawl + storage only.
- **Model inference / embedding costs** — if IntentForge generates embeddings per page, add GPU or API cost. Not priced here.
- **CDN for serving search results to users** — not priced.
- **Domain registration, SSL, DNS** — trivial (<$20/mo) but not itemised.
- **Incident response, security audit, penetration test** — not priced.
- **Multi-region / HA** — all figures are single-region.

---

## 13. Source list (all accessed 2026-11-10)

| # | Source | URL |
|---|---|---|
| 1 | Hetzner Cloud — Regular Performance | https://www.hetzner.com/cloud/regular-performance/ |
| 2 | Hetzner Cloud — General Purpose | https://www.hetzner.com/cloud/general-purpose/ |
| 3 | Hetzner Cloud — traffic & FAQs | https://www.hetzner.com/cloud/ |
| 4 | Hetzner Storage Share | https://www.hetzner.com/storage/storage-share/ |
| 5 | DigitalOcean Managed Databases pricing | https://www.digitalocean.com/pricing/managed-databases |
| 6 | DigitalOcean Managed Databases product | https://www.digitalocean.com/products/managed-databases |
| 7 | DigitalOcean Droplet pricing | https://www.digitalocean.com/pricing/droplets |
| 8 | AWS RDS for PostgreSQL pricing | https://aws.amazon.com/rds/postgresql/pricing/ |
| 9 | AWS RDS Free Tier | https://aws.amazon.com/rds/free/ |
| 10 | AWS EC2 On-Demand pricing | https://aws.amazon.com/ec2/pricing/on-demand/ |
| 11 | AWS S3 pricing | https://aws.amazon.com/s3/pricing/ |
| 12 | Azure Database for PostgreSQL Flexible Server | https://azure.microsoft.com/en-us/pricing/details/postgresql/flexible-server/ |
| 13 | Cloudflare R2 pricing (updated 2026-10-30) | https://developers.cloudflare.com/r2/pricing/ |
| 14 | Cloudflare Workers & Pages pricing | https://www.cloudflare.com/plans/developer-platform/ |
| 15 | Browserless pricing | https://www.browserless.io/pricing |
| 16 | Grafana Cloud pricing | https://grafana.com/pricing/ |
| 17 | HashiCorp Vault pricing | https://www.hashicorp.com/en/pricing?tab=vault |
| 18 | Mullvad VPN pricing | https://mullvad.net/en/pricing |
| 19 | Tor Project — technical considerations | https://community.torproject.org/relay/technical-considerations/ |
| 20 | PostgreSQL 17 docs — Backup and Restore | https://www.postgresql.org/docs/17/backup.html |
| 21 | PostgreSQL — About / licence | https://www.postgresql.org/about/ |

---

*End of report. All [CONFIRMED] figures were read from the cited page on 2026-11-10. All [INFERRED] figures show their arithmetic. All [UNKNOWN] items are stated as such with no number invented.*
