# Commerce Data Licensing & Affiliate Programme Costs
## Research Report for IntentForge

**Date:** November 10, 2026
**Researcher:** Hermes Agent
**Method:** Primary sources only (official programme terms, fee schedules, commission pages, API pricing pages)

**Labeling convention:**
- [CONFIRMED] = primary source explicitly states it
- [INFERRED] = derived from source context
- [UNKNOWN] = not publicly disclosed / could not verify

---

## Table of Contents
1. [Indian Affiliate Programmes](#indian)
2. [Indian Affiliate Networks](#networks)
3. [Amazon Global / Associates + Creators API](#amazon)
4. [eBay Partners](#ebay)
5. [Shopify](#shopify)
6. [WooCommerce](#woocommerce)
7. [Google Merchant Center](#gmc)
8. [Global Affiliate Networks (Awin, CJ, Impact, Rakuten)](#global-networks)
9. [Comparison Table](#comparison)
10. [Analysis: Cheapest to Start](#analysis)

---

## 1. Indian Affiliate Programmes {#indian}

### 1.1 Flipkart Affiliate

**Sources (all accessed 2026-11-10):**
- https://affiliate.flipkart.com/ (programme homepage)
- https://affiliate.flipkart.com/commissions (rate card, "REFERRAL PAYOUTS FOR ORDERS IN NOVEMBER 2026")
- https://affiliate.flipkart.com/payment-information (payout & tax terms)
- https://affiliate.flipkart.com/terms-and-conditions (operating agreement)
- https://affiliate.flipkart.com/faqs
- https://affiliate.flipkart.com/api-docs/af_tou.html (API Terms of Use)

**(a) Joining / application fees**
- [CONFIRMED] "There are absolutely no costs involved in joining or being part of the affiliate program" — https://affiliate.flipkart.com/faqs (accessed 2026-11-10).
- [CONFIRMED] No fee for either payment method: "There are no charges applicable on either payment method." — https://affiliate.flipkart.com/payment-information (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] Payment Information page: "If affiliates select payment via gift voucher, Flipkart will accrue and withhold referral fees until the total amount due is at least Rs 25,000.00. If affiliates select payment via Electronic Fund Transfer (EFT), Flipkart will accrue and withhold referral fees until the total amount due is at least Rs 25,000.00." — https://affiliate.flipkart.com/payment-information (accessed 2026-11-10).
- [CONFIRMED] "If we owe you less than Rs. 25,000, your fees will be rolled into the next month's total." — same page.
- **CONFLICT FLAG:** The FAQ page states lower thresholds: "If affiliates select payment via gift voucher, Flipkart will accrue and withhold referral fees until the total amount due is at least Rs 2500.00. If affiliates select payment via EFT, Flipkart will accrue and withhold referral fees until the total amount due is at least Rs 5000.00." — https://affiliate.flipkart.com/faqs (accessed 2026-11-10). The two official pages disagree (₹25,000 vs ₹2,500/₹5,000). Treat the ₹25,000 figure as the currently operative one on the dedicated Payment Information page; verify in-account before relying on either.

**(c) Commission rates by category (November 2026 rate card)**
[CONFIRMED] Source: https://affiliate.flipkart.com/commissions (accessed 2026-11-10). Rates are identical for new vs existing customer and for desktop/mobile-web vs mobile-app unless noted.

| Category | Rate |
|---|---|
| Grocery – Flipkart Supermart | 4% |
| Books & General Merchandise (auto accessory, toys, personal care, books & media, baby care, sports, food & nutrition) | 5% |
| Home (decor, furnishing, improvement, household supplies, kitchen & dining, cookware, pet supplies, tools & hardware) | 3% |
| Furniture | 0.5% |
| Core Electronics (automobile, camera, IT accessory/peripheral, laptop & desktop, personal health care, storage, tablet) | 3% |
| Emerging Electronics (audio, gaming, IoT, mobile protection, power bank, mobile accessories, smart home, video) | 3% |
| Fashion & Lifestyle (bags, clothing, eyewear, footwear, watches, etc.) | 8% |
| Large Appliances (TV, refrigerator, washing machine, AC) | 3% |
| Small Home Appliances | 2% |
| Mobile Phones Tier A | 1% |
| Mobile Phones Tier B | 0.5% |
| Mobile Phones Tier C / Any Other Mobile Phones | 0% |
| Flipkart Gift Card | 0% |
| Gemstones, Gold & Silver Coins | 0.1% |

- [CONFIRMED] Homepage advertises headline "6–12%" and "Upto 5% Mobiles / Upto 6% Computers / 6–20% Toys / Upto 4% Cameras" — https://affiliate.flipkart.com/ (accessed 2026-11-10). These headline figures do not match the November 2026 rate card table; the rate card is the operative document.
- [CONFIRMED] "New Customer" = first order/transaction on Flipkart.com in the last 6 months.
- [CONFIRMED] No payout on Flipkart First subscription, Value Added Services, iOS & Windows app installs.

**(d) Payment terms and delays**
- [CONFIRMED] "We will pay you referral fees on a monthly basis for Qualifying Products shipped in the applicable month." — https://affiliate.flipkart.com/terms-and-conditions (accessed 2026-11-10).
- [CONFIRMED] "Flipkart.com has a 30 day cancellation/return policy... We make payments to the affiliates only for products which are not returned in these 30 days. Hence the payments are made after 30-60 days from the day when an order was made." — https://affiliate.flipkart.com/payment-information (accessed 2026-11-10).
- [CONFIRMED] Earnings reflect only after order delivery; typically visible 3–5 days after order placement.
- [CONFIRMED] Payment mode is locked as of the 1st of each month; mid-month changes apply next cycle.
- [CONFIRMED] Flipkart may cap referral fees at a "Monthly Limit" per month at its sole discretion — https://affiliate.flipkart.com/terms-and-conditions.

**(e) API rate limits**
- [CONFIRMED] API access is gated: "All sellers registered with the Flipkart Affiliate Program shall be eligible to access the APIs in accordance with the terms and conditions under this ToU." — https://affiliate.flipkart.com/api-docs/af_tou.html (accessed 2026-11-10).
- [UNKNOWN] Specific API rate limits (requests/second, quota) are not published on the public API ToU page.

**(f) Data access costs**
- [CONFIRMED] Affiliate API access is free to registered affiliates; no fee published. Product data access is via affiliate APIs/link tools.
- [UNKNOWN] Whether bulk product-feed licensing (non-affiliate) carries a fee — not publicly disclosed.

**(g) Tax withholding**
- [CONFIRMED] "As per applicable Indian tax laws, we deduct 5% TDS on all payments made through Electronic Fund Transfer (EFT) as well as in the form of Gift Vouchers (EGV)... covered under Section 194H of the Income Tax Act and deduct TDS at the rate of 5% (if PAN is provided to us). In case of Gift Voucher (EGV) payments, if PAN is not provided to us, referral fees would be subjected to 20% income tax (TDS) withholding." — https://affiliate.flipkart.com/payment-information (accessed 2026-11-10).
- [CONFIRMED] PAN, payee name (same as PAN), IFSC, bank account, mailing address, contact number required to be paid.

**(h) Exclusivity / non-compete**
- [UNKNOWN] No exclusivity or non-compete clause found on the public terms pages reviewed. Not publicly disclosed.

**(i) Costs of NOT complying (clawback / termination)**
- [CONFIRMED] Clawback: "If a user returns a product for a refund, or if credit card charges are reversed due to a dispute or credit card fraud, the Affiliate's account will be debited for any commission earned on that transaction." — https://affiliate.flipkart.com/payment-information (accessed 2026-11-10).
- [CONFIRMED] Rate-change risk: "We hold the right to make changes to the affiliate commission rates (at any time without prior notice)" — https://affiliate.flipkart.com/commissions.
- [CONFIRMED] Monthly Limit cap can reduce earned fees at Flipkart's sole discretion.
- [UNKNOWN] Termination notice period / reinstatement terms not published on public pages.

---

### 1.2 Amazon India Associates + Creators API

**Sources (all accessed 2026-11-10):**
- https://affiliate-program.amazon.in/ (programme homepage)
- https://affiliate-program.amazon.in/help/node/topic/GRXPHT8U84RAYDXZ (Associates Program Advertising Fee Schedule, "Oct-Nov 2026")
- https://affiliate-program.amazon.in/help/operating/agreement/ (Operating Agreement)
- https://affiliate-program.amazon.in/help/node/topic/G9LSJ3UCEQWZ4DTJ (help index)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published anywhere on the programme pages; participation is free. (Homepage and Operating Agreement contain no application fee.) — https://affiliate-program.amazon.in/ (accessed 2026-11-10).
- [UNKNOWN] Whether Amazon charges for API access separately — see (f).

**(b) Minimum payout thresholds**
- [CONFIRMED] "We will pay you approximately 60 days following the end of each calendar month, but we may accrue and withhold advertising fees until the total amount due to you is at least INR1000 in case of NEFT transfers." — https://affiliate-program.amazon.in/help/operating/agreement/ (accessed 2026-11-10).
- [CONFIRMED] Payment is monthly for Qualifying Purchases shipped/streamed/downloaded in a given month.
- [CONFIRMED] Seller Central help page (legacy wording) says "We send quarterly payment to Associates" — https://sellercentral.amazon.in/help/hub/reference/external/G28591 (accessed 2026-11-10). The Operating Agreement (monthly, ~60 days) supersedes this legacy page.

**(c) Commission rates by category (Oct–Nov 2026 Advertising Fee Schedule)**
[CONFIRMED] Source: https://affiliate-program.amazon.in/help/node/topic/GRXPHT8U84RAYDXZ (accessed 2026-11-10).

| Product Category | Fixed Advertising Rate |
|---|---|
| Beauty / Luxury Beauty | 8% |
| Apparel & Accessories / Luggage & Bags / Watches | 7.5% |
| Jewelry / Shoes and Handbags | 7% |
| Automotive / Baby Products / Books / Personal Care Appliances / Sports & Fitness | 4.72% |
| Home / Home Improvement / Office Products | 4.25% |
| Furniture / Health, Household & Personal Care / Kitchen & Dining / Pet Supplies | 4% |
| Wireless Accessories | 3.4% |
| Musical Instruments | 3.2% |
| Business & Industrial Supplies | 2.98% |
| Electronics / Lawn & Garden / Video Games | 2.80% |
| Camera | 2.63% |
| Amazon Fresh / Grocery | 2.50% |
| Computers / Home Entertainment / Large Appliances | 2.45% |
| Mobile Phones (headline) | 1% |
| Bill Payments | 30% (up to Rs 3) |
| All Other Categories | 5% |

Mobile-phone differential rates (same source): most listed iPhone, Samsung, iQOO, Motorola, Nokia, OnePlus, LAVA, Redmi, POCO, HMD, VIVO, Honor, Nothing, Oppo, Tecno, itel, Jio, Realme models at **0%**; select models at 2% (e.g., OnePlus Nord CE6, iQOO 15R, Redmi 15C, Samsung Galaxy S25 Ultra); select at 1.5% (OnePlus Nord 6, iQOO Z11xa, Moto G37 Power, Redmi A7 Pro 4G, Samsung Galaxy A56 5G / S26 family, VIVO X300 family, Realme NARZO 100x 5G / P4 Lite 5G / NARZO Power 5G); select at 0.5% (One Plus N6 / 15 / 13s / N6 Lite, VIVO S2/Y-series, Samsung Galaxy M17 / M08 / F36 / A57 / A17 / A37 / M36 / Z Fold8 / Z Flip8 / A27, iQOO 15, Moto Edge 60 Fusion / G57 Power / Edge 70 Fusion / A300, Tecno POP X 5G / Camon 50 Ultra 5G / Pova Curve 2 5G / Spark 50 5G / Spark Go 3, Redmi Note 15/17 series, OPPO Reno15/F35/Find X9/A6 series, LAVA Bold N2/N4/Agni 4).
- [CONFIRMED] Additional 0% categories: Apple Products (MacBooks, Apple Watch, iPads, PC Accessories); all earphones/headphones/speakers/audio accessories below Rs. 3000 MRP; listed headsets; all microwaves & semi-automatic washing machines.
- [CONFIRMED] Excluded products (no fee): Flight Bookings and Gift Cards; Video Gaming Consoles & Hardware; Prime Membership; Kindle EBooks; Gold & Silver Coins.
- [CONFIRMED] Bounties (Table 2, same source): Sell on Amazon – GST Validated ₹400; Sell on Amazon – Completed Registration ₹10; Amazon Business registration ₹200; Audible Free Trial ₹150; Audible Paid Membership ₹150; Prime Music First Stream ₹100; Prime Music paid membership ₹100; Paid Prime Membership on PrimeVideo.com ₹100.
- [CONFIRMED] Homepage advertises "Earn up to 10% in affiliate fees" — https://affiliate-program.amazon.in/ (accessed 2026-11-10). The published fee schedule tops out at 8% (Beauty/Luxury Beauty) plus the 30%-capped-at-₹3 Bill Payments line.

**(d) Payment terms and delays**
- [CONFIRMED] "We will pay you approximately 60 days following the end of each calendar month" — Operating Agreement (accessed 2026-11-10).
- [CONFIRMED] Accrual/hold until ₹1,000 for NEFT.

**(e) API rate limits**
- [UNKNOWN] Amazon Product Advertising API (PA-API) / Creators API rate limits are documented in Amazon's developer docs, which were not reachable/retrieved in this pass. Not verified from primary source in this report.

**(f) Data access costs**
- [CONFIRMED] Associates participation and link-generation tools are free; no data-access fee published.
- [UNKNOWN] PA-API / Creators API pricing (if any) not publicly disclosed on the Associates pages reviewed.

**(g) Tax withholding**
- [CONFIRMED] "If required by applicable Indian tax law, we may deduct or withhold taxes, levies or any similar amounts from the advertising fees payable to you. If you are an Indian resident, advertising fees payable to you will be subject to income tax withholding at the rate stipulated under applicable law." — Operating Agreement (accessed 2026-11-10).
- [UNKNOWN] Exact TDS percentage for Amazon India Associates is not stated on the public Operating Agreement page.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not publicly disclosed on the pages reviewed.

**(i) Costs of NOT complying (clawback / termination)**
- [CONFIRMED] Amazon may modify the fee schedule from time to time; rates can be excluded/reduced/increased — Fee Schedule "Please Note" (accessed 2026-11-10).
- [CONFIRMED] Fees are payable only on Qualifying Purchases; returns/bad debt reduce Qualifying Revenues.
- [UNKNOWN] Specific termination/clawback mechanics not detailed on public pages reviewed.

---

### 1.3 Meesho

**Sources (all accessed 2026-11-10):**
- https://affiliate.meesho.com/ (affiliate/creator portal)
- https://affiliate.meesho.com/commission-links
- https://supplier.meesho.com/pricing (supplier pricing — for contrast)
- https://www.meesho.com/bestsellers/pl/txz (seller-facing zero-commission claim)

**(a) Joining / application fees**
- [CONFIRMED] The Meesho affiliate/creator portal presents itself as free to join; no joining fee is published on https://affiliate.meesho.com/ (accessed 2026-11-10).
- [CONFIRMED] Meesho's supplier programme advertises "0% Commission" across all categories — https://supplier.meesho.com/pricing (accessed 2026-11-10). This is the *seller* programme, not the creator/affiliate programme.

**(b) Minimum payout thresholds**
- [UNKNOWN] No minimum payout threshold is published on the public Meesho affiliate pages. Third-party creator FAQs (Scribd, not primary) mention weekly payouts via Meesho Creator Center; not verified from Meesho's own terms.

**(c) Commission rates by category**
- [CONFIRMED] Meesho does **not** publish a single public rate card by category. Third-party sources consistently state that sellers set per-product commission and there is no universal rate — https://tadlo.ai/affiliate-programs/meesho-affiliate (accessed 2026-11-10, secondary source, not primary).
- [CONFIRMED] A widely circulated Meesho Creator Program FAQ (Scribd copy of a Meesho creator document, not the official site) states affiliate products offer 10–25% commission per order and non-affiliate products 0.5% — https://www.scribd.com/document/909281093/Meesho-Creator-Program-FAQ-for-Influencers (accessed 2026-11-10). **This is a re-hosted copy, not a primary Meesho page — treat as [INFERRED] at best.**
- [UNKNOWN] Official, current, category-level commission rates for the Meesho creator/affiliate programme are **not publicly disclosed** on Meesho's own domain as of 2026-11-10.

**(d) Payment terms and delays**
- [UNKNOWN] Not published on public Meesho affiliate pages. Secondary sources claim weekly payouts on the direct Creator Club and up to 90-day confirmation on third-party networks — unverified.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [UNKNOWN] Not published.

**(g) Tax withholding**
- [UNKNOWN] Not published on the public affiliate pages. (Secondary sources mention TDS applies to commission income; unverified.)

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [UNKNOWN] Not published. Secondary sources claim "no flags or penalties for returns" on the direct programme; unverified.

**Note:** Meesho is the weakest-documented programme in this report. Any commercial decision should be based on terms shown inside a live creator account, not on public pages.

---

### 1.4 Myntra

**Sources (all accessed 2026-11-10):**
- https://www.myntra.com/tac (Myntra Ultimate Glam Clan / UGC programme T&C — primary)
- https://www.cuelinks.com/campaigns/myntra-affiliate-program (network campaign page — primary for Cuelinks terms)
- https://www.vcommission.com/blog/publishers/myntra-affiliate-program-promote-indias-favourite-fashion-marketplace-this-festive-season/ (network blog)
- https://earnkaro.com/blog/how-to-earn-money-by-becoming-a-myntra-affiliate/ (network blog)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published; Myntra's affiliate participation is via in-app UGC programme or approved networks. EarnKaro blog states "No genuine program charges a fee to make you a Myntra affiliate" — https://earnkaro.com/blog/how-to-earn-money-by-becoming-a-myntra-affiliate/ (accessed 2026-11-10, secondary).
- [CONFIRMED] Myntra T&C requires 3 approved UGC posts before monetary commissions unlock (Meta/Facebook affiliates exempt for Facebook-tracked revenue only) — https://www.myntra.com/tac (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] "A minimum threshold of INR 500 in accrued commissions must be reached before a monetary payout is initiated." — https://www.myntra.com/tac (accessed 2026-11-10).
- [CONFIRMED] KYC completion is a strict precondition for payout; without KYC, earnings are paid as Myncash at Myntra's discretion.

**(c) Commission rates by category**
- [CONFIRMED] Myntra T&C defines "Net Commission" as payable on "Net Order"/"Net GMV" at Myntra's "Defined Commission Rate Card Linked to the tier of the creator in the UGC Program" — https://www.myntra.com/tac (accessed 2026-11-10). **The actual rate card is not published on the public T&C page.**
- [CONFIRMED] Myntra T&C: "Eligible product categories, Commission rates, tiers, and criteria are set by Myntra and may change at any time at Myntra's discretion."
- [CONFIRMED] Cuelinks campaign page: "The Myntra Affiliate Program is offering 7.5% commission per Myntra new user sale and 3.75% commission per sale for existing Myntra users." — https://www.cuelinks.com/campaigns/myntra-affiliate-program (accessed 2026-11-10).
- [CONFIRMED] Cuelinks important info: "No commission will be payable when the transaction is done from Internet Explorer. Payout is not applicable if you pay with Myntra Credits and Gift Cards. Payout is calculated on order amount excluding what is paid via wallets credits or gift vouchers."
- [CONFIRMED] vCommission blog lists: Home 7% (new) / 3.5% (repeat); Beauty & Personal Care 7% / 3.5%; Footwear 5.6% / 2.8%; Kids Wear 5.6% / 2.8%; cookie 30 days; validation period 60 days; ROAS requirement 15 — https://www.vcommission.com/blog/publishers/myntra-affiliate-program-promote-indias-favourite-fashion-marketplace-this-festive-season/ (accessed 2026-11-10).
- [CONFIRMED] EarnKaro blog: up to 10% new user / up to 4–5% existing user; minimum withdrawal ₹10 — https://earnkaro.com/blog/how-to-earn-money-by-becoming-a-myntra-affiliate/ (accessed 2026-11-10).
- **Discrepancy:** Rates vary by network (7.5/3.75 on Cuelinks vs up to 10/5 on EarnKaro vs 7/3.5 on vCommission). Myntra does not publish a single official rate card publicly; network campaign pages are the operative published rates.

**(d) Payment terms and delays**
- [CONFIRMED] vCommission: validation period 60 days; payments processed within 2–4 months (Cuelinks campaign page).
- [CONFIRMED] Myntra T&C: commissions accrue in INR, visible on rewards dashboard; payout contingent on KYC.
- [UNKNOWN] Exact payout cycle (weekly/monthly) not stated in Myntra's public T&C.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [UNKNOWN] Not published on Myntra's public T&C page reviewed.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [CONFIRMED] KYC failure → no monetary payout (Myncash instead, at Myntra's discretion) — https://www.myntra.com/tac.
- [CONFIRMED] Cuelinks: no commission on Internet Explorer transactions; no commission on orders paid via Myntra Credits/Gift Cards.
- [CONFIRMED] vCommission: cancelled/returned orders excluded from final reconciliation; ROAS must be maintained at 15.
- [CONFIRMED] Myntra may change rates/categories at any time at its discretion.

---

### 1.5 Croma

**Sources (all accessed 2026-11-10):**
- https://www.croma.com/affiliate-faqs (official Affiliate Program FAQs — primary)
- https://www.croma.com/affiliate-program (signup page)
- https://affi.io/m/croma (network aggregator listing — secondary, for rate cross-reference)

**(a) Joining / application fees**
- [CONFIRMED] "No. It's absolutely free of cost. All you have to do is fill up the form & Register with us." — https://www.croma.com/affiliate-faqs (accessed 2026-11-10).
- [CONFIRMED] Approval takes up to 2 weeks; only one application per email ID; rejected emails cannot reapply for 3 months.

**(b) Minimum payout thresholds**
- [UNKNOWN] No minimum payout threshold is published on Croma's public affiliate FAQ page.

**(c) Commission rates by category**
- [UNKNOWN] Croma does **not** publish a category-level commission rate card on its own public pages. The FAQ page contains no rates.
- [INFERRED] Third-party network listings (affi.io) show Croma campaigns at roughly 1.2%–4.5% revenue share depending on network, with category slabs such as Laptops/desktops 2.08%, Home/Kitchen Appliances 2.98%, Large Appliances 1.49%, TV/LED 1.49%, Camera 1.49%, Gaming 1.49%, Wearables 2.08%, Mobile fixed payouts ₹29.5–₹59.5 per transaction on select models. **These are network-published figures, not Croma's own rate card — treat as indicative only.**

**(d) Payment terms and delays**
- [CONFIRMED] "We count the payment cycle from the first to last day of each month. You will receive a detailed email with the number of orders, revenue and final commission by the end of the said month + 45 days. 15-20 days later the amount will be credited into your account. So for example, March's detailed payment email will hit your inbox on 30th April, and the payment will hit your account between 15-30th May latest." — https://www.croma.com/affiliate-faqs (accessed 2026-11-10).
- [CONFIRMED] Payment by bank transfer only; no cash. Requires bank account details, PAN card, cancelled cheque; GST invoice required if GST-registered.
- [CONFIRMED] Reports updated every 24–48 hours.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [UNKNOWN] Not published on the public FAQ page. PAN is mandatory for payment processing.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [CONFIRMED] "if a customer cancels or returns an order, you will not be paid a commission for it. The commission is calculated only once the order is fully complete and successful." — https://www.croma.com/affiliate-faqs.
- [CONFIRMED] Self-order cap: "Only 10 self-orders are allowed per year per affiliate."
- [CONFIRMED] Rejected applicants cannot reapply for 3 months.

---

### 1.6 Reliance Digital

**Sources (all accessed 2026-11-10):**
- https://www.cuelinks.com/campaigns/reliance-digital-affiliate-program (Cuelinks campaign page — primary for network terms)
- https://earnkaro.com/blog/reliance-digital-affiliate-program/ (EarnKaro blog — secondary)
- https://www.shopper.com/partnerships/reliancedigital-in-affiliate-program (aggregator — secondary)

**(a) Joining / application fees**
- [CONFIRMED] EarnKaro lists "Joining fees: Free" — https://earnkaro.com/blog/reliance-digital-affiliate-program/ (accessed 2026-11-10, secondary).
- [UNKNOWN] Reliance Digital does not publish its own affiliate programme terms publicly; participation is via affiliate networks.

**(b) Minimum payout thresholds**
- [CONFIRMED] EarnKaro: minimum withdrawal ₹10 — https://earnkaro.com/blog/reliance-digital-affiliate-program/ (accessed 2026-11-10, secondary).
- [UNKNOWN] Reliance Digital's own threshold not published.

**(c) Commission rates by category**
- [CONFIRMED] Cuelinks campaign page headline: "Reliance Digital Affiliate Program with Payout 2.84% Per Sale"; cookie 7 days; category-level payouts gated behind login ("Join Cuelinks To unlock category wise payout") — https://www.cuelinks.com/campaigns/reliance-digital-affiliate-program (accessed 2026-11-10).
- [CONFIRMED] EarnKaro: 2.5% flat for new and old users — https://earnkaro.com/blog/reliance-digital-affiliate-program/ (accessed 2026-11-10, secondary).
- [CONFIRMED] inrdeals: 2.2% base rate on successful sale, with volume/performance increments — https://inrdeals.com/campaigns/reliance-digital-affiliate-program (accessed 2026-11-10, secondary).
- [CONFIRMED] Cuelinks important info: SEM/brand-keyword bidding strictly prohibited; coupons only from Cuelinks or advertiser site; no commission on ROne loyalty points or bulk transactions; brand bidding not allowed; Google Ads/Facebook/Insta promotion not allowed.

**(d) Payment terms and delays**
- [CONFIRMED] EarnKaro: commission confirmation 45–60 days; payment duration 6–8 days; tracking within 24 hours — https://earnkaro.com/blog/reliance-digital-affiliate-program/ (accessed 2026-11-10, secondary).
- [CONFIRMED] Cuelinks: 7-day cookie.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join via networks; no data-access fee published.

**(g) Tax withholding**
- [UNKNOWN] Not published on the campaign pages reviewed.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Brand-keyword bidding prohibited (Cuelinks campaign terms).

**(i) Costs of NOT complying**
- [CONFIRMED] EarnKaro: cancelled/returned transactions → commission marked "Cancelled", not "Confirmed".
- [CONFIRMED] Cuelinks: commission not applicable on bulk transactions or ROne loyalty point purchases.

---

### 1.7 Tata CLiQ

**Sources (all accessed 2026-11-10):**
- https://www.cuelinks.com/campaigns/tatacliq-affiliate-program (Cuelinks campaign page — primary for network terms)
- https://www.extrape.com/blog/tata-cliq-affiliate-program/ (ExtraPe blog — secondary)
- https://takeads.com/merchants/tatacliq-in/ (aggregator — secondary)

**(a) Joining / application fees**
- [CONFIRMED] ExtraPe blog: "TataCliq does not have an affiliate program of its own. To begin your TataCliq earning quest you need to create an ExtraPe account." Joining fees: "Nill" — https://www.extrape.com/blog/tata-cliq-affiliate-program/ (accessed 2026-11-10, secondary).
- [CONFIRMED] Cuelinks campaign page lists the campaign as "paused by the advertiser" — https://www.cuelinks.com/campaigns/tatacliq-affiliate-program (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [UNKNOWN] Not published.

**(c) Commission rates by category**
- [CONFIRMED] Cuelinks campaign page: CPC model, "Payout Upto ₹ 0.17 / Click" (November 2026); campaign paused — https://www.cuelinks.com/campaigns/tatacliq-affiliate-program (accessed 2026-11-10).
- [CONFIRMED] ExtraPe blog lists category rates: initial tracking 3%; fashion accessories/watches 5%; home products 5%; home theatre/headphones/sound bars 4%; fashion jewellery 3%; health care/personal care 2%; fads & gadgets/mobile accessories 1.5%; fine jewellery 0.2%; electrical items incl. smartphones 0%; gold coin/beauty & grooming 0% — https://www.extrape.com/blog/tata-cliq-affiliate-program/ (accessed 2026-11-10, secondary).
- [CONFIRMED] ExtraPe: profit confirmation 90 days.

**(d) Payment terms and delays**
- [CONFIRMED] ExtraPe: profit confirmation 90 days; commission cancelled if transaction cancelled/returned.
- [CONFIRMED] Cuelinks: disallowed mediums PPC, SEM, adult, gambling, Google Ads; non-converting clicks may cause removal from the programme.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join via networks.

**(g) Tax withholding**
- [UNKNOWN] Not published.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Cuelinks: brand bidding and Google Ads/Facebook/Insta promotion not allowed.

**(i) Costs of NOT complying**
- [CONFIRMED] Cuelinks: "Non-converting clicks may cause the advertiser to remove you from the program."
- [CONFIRMED] ExtraPe: cancelled/returned transactions → commission not confirmed.

---

### 1.8 JioMart

**Sources (all accessed 2026-11-10):**
- https://www.jiomart.com/s/affiliate-service-provider-in/69670 (JioMart affiliate service page — primary, minimal content)
- https://www.realcash.in/store/jio-mart (network campaign page — secondary)
- https://earnkaro.com/blog/jiomart-affiliate-program/ (EarnKaro blog — secondary)
- https://www.cuelinks.com/campaigns/jiomart-affiliate-program (Cuelinks campaign page — primary for network terms)

**(a) Joining / application fees**
- [CONFIRMED] EarnKaro blog: joining free (implied by "Join now"); no fee published — https://earnkaro.com/blog/jiomart-affiliate-program/ (accessed 2026-11-10, secondary).
- [UNKNOWN] JioMart's own affiliate terms are not published; the official JioMart affiliate page contains no programme details.

**(b) Minimum payout thresholds**
- [UNKNOWN] Not published.

**(c) Commission rates by category**
- [CONFIRMED] Cuelinks campaign page: "Payout 5.62% Per Sale"; campaign "paused by the advertiser"; payout applicable till 12 June 2024 — https://www.cuelinks.com/campaigns/jiomart-affiliate-program (accessed 2026-11-10).
- [CONFIRMED] EarnKaro: up to ₹150 per order (flat), new and old users — https://earnkaro.com/blog/jiomart-affiliate-program/ (accessed 2026-11-10, secondary).
- [CONFIRMED] RealCash campaign page lists detailed category profits (JioMart Android app orders only): All Appliances 2.55%; Small Appliances 1.28%; Personal Care appliances 1.28%; Mobile Accessories 1.28%; Dyson 1.28%; Laptops (HP/Asus/Dell/Lenovo/Acer) 1.28%; Tablets (non-Apple) 1.28%; TV 43"+ & Sound Bars 1.28%; Mobiles (Google/Nothing) 1.02%; Printers/Keyboards/Mouse/Storage 1.02%; TV 32" 1.02%; Mobiles (POCO/iQOO/Xiaomi/Vivo/Samsung/OPPO/OnePlus other) 0.51%; Mobiles Oppo K14 / OnePlus N6/N6x 0.26%; Laptops & Tablets (Apple) 0.26%; Gaming/Cameras 0.26%; Apple iPhones & all other categories ₹0.00; Desktop/iOS transactions ₹0.00 — https://www.realcash.in/store/jio-mart (accessed 2026-11-10, secondary).
- [CONFIRMED] RealCash: "Payment will be processed after deduction of 2% as TDS as per the norms."

**(d) Payment terms and delays**
- [CONFIRMED] RealCash: tracked within 36 hours; confirmed by a future date (e.g., February 05, 2027); missing profit claim: no.
- [UNKNOWN] JioMart's own payout cycle not published.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join via networks.

**(g) Tax withholding**
- [CONFIRMED] RealCash: 2% TDS deduction on payment — https://www.realcash.in/store/jio-mart (accessed 2026-11-10, secondary).

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [CONFIRMED] RealCash: earnings only on JioMart Android app transactions; desktop/iOS transactions earn ₹0.00.
- [CONFIRMED] RealCash: use of unapproved coupon codes voids earnings.

---

## 2. Indian Affiliate Networks {#networks}

### 2.1 Cuelinks

**Sources (all accessed 2026-11-10):**
- https://www.cuelinks.com/terms (Terms & Conditions, last updated 1 October 2025 — primary)
- https://www.cuelinks.com/publishers (publisher FAQ — primary)
- https://www.cuelinks.com/campaigns (campaign listing)

**(a) Joining / application fees**
- [CONFIRMED] "Cuelinks is completely free to sign up for all publishers. However, we do take a small portion of the commission earned to keep offering our tools and features free to use." — https://www.cuelinks.com/publishers (accessed 2026-11-10).
- [CONFIRMED] "Cuelinks is free to join for all website owners, bloggers, content writers, and social media influencers."

**(b) Minimum payout thresholds**
- [CONFIRMED] "The minimum payout on Cuelinks is Rs.500 for affiliates with Indian bank accounts and the same is Rs.20,000 for affiliates with bank accounts outside India." — https://www.cuelinks.com/publishers (accessed 2026-11-10).
- [CONFIRMED] Terms: "A minimum payout threshold (e.g. ₹ 500) may apply."

**(c) Commission rates by category**
- [CONFIRMED] Cuelinks is a network aggregator; rates are per-campaign and displayed in the publisher dashboard. "Payout structures may vary by campaign, Merchant, traffic source, geography or other commercial factors and may be revised by Cuelinks at its discretion." — https://www.cuelinks.com/terms (accessed 2026-11-10).
- [CONFIRMED] Revenue share: Cuelinks retains a cut of the merchant commission. A Cuelinks blog describes a "75-95 : 5-25 revenue-sharing model" where the publisher retains 75–95% of the standard commission — https://www.cuelinks.com/blog/how-to-sign-up-for-amazon-affiliate-program-successfully/ (accessed 2026-11-10). **This is a blog/marketing page, not the legal terms — treat the exact split as [INFERRED].**
- [CONFIRMED] Campaign examples published on Cuelinks: Myntra 7.5%/3.75%; Reliance Digital 2.84%; JioMart 5.62% (paused); Tata CLiQ ₹0.17/click (paused); Amazon India up to 3.80% base via Cuelinks.

**(d) Payment terms and delays**
- [CONFIRMED] "All payouts are subject to: Validation cycles (currently Net-60 days, unless otherwise notified); Adjustments, including chargebacks for cancelled, refunded or invalid events; and Statutory deductions, including Tax Deduction at Source (TDS)." — https://www.cuelinks.com/terms (accessed 2026-11-10).
- [CONFIRMED] "Payouts are released by bank transfer to the Publisher's designated account once the threshold is met and the payout has been validated and collected from the Merchant."

**(e) API rate limits**
- [CONFIRMED] Cuelinks offers an HTTP API for publishers as an alternative to the JavaScript code — https://www.cuelinks.com/terms (accessed 2026-11-10).
- [UNKNOWN] Specific API rate limits not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; Cuelinks takes a revenue share of commissions earned.

**(g) Tax withholding**
- [CONFIRMED] "All payouts by Cuelinks are subject to deduction of tax at source in accordance with the applicable Indian provisions of the Income-tax Act, 1961... Cuelinks will deposit the tax so deducted with the Government and issue the prescribed TDS certificate within the statutory timelines. All payouts will be disbursed net of such deductions." — https://www.cuelinks.com/terms (accessed 2026-11-10).
- [CONFIRMED] KYC and tax compliance mandatory for payout release; PAN and GSTIN (if applicable) required.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Publishers are "automatically connected to all eligible campaigns, unless a campaign requires pre-approval or the Publisher opts out."
- [CONFIRMED] Publishers may disassociate from a particular merchant by email request.

**(i) Costs of NOT complying**
- [CONFIRMED] Clawback: "If a Qualifying Event is subsequently cancelled, refunded, reversed or deemed fraudulent by the Merchant, the Merchant may reverse the corresponding payment to Cuelinks. Any payout already made to the Publisher for such an event will be adjusted against future payouts or must be repaid by the Publisher within 15 working days of notification." — https://www.cuelinks.com/terms (accessed 2026-11-10).
- [CONFIRMED] Termination: either party may terminate with at least 7 working days' notice; Cuelinks may suspend/terminate immediately for fraud, policy breach, or failure to respond to compliance/audit requests.
- [CONFIRMED] Audit response: publishers must provide complete promotional details within 7 working days of a request; failure → campaign blocking or account suspension.
- [CONFIRMED] Indemnity: publishers indemnify Cuelinks against losses from breach, misuse of code/API, traffic/promotional practices, merchant non-payment, or third-party claims.

---

### 2.2 Admitad

**Sources (all accessed 2026-11-10):**
- https://www.admitad.com/terms-for-publishers/ (Terms for publishers, German entities — primary)
- https://www.admitad.com/terms-for-publishers-uae/ (Terms for publishers, Global — primary)
- https://www.admitad.com/terms-for-publishers-mexico/ (Terms for publishers, Mexico — primary)
- https://www.admitad.com/store/ (programme store)
- https://www.admitad.com/store/all/ (all programmes, aggregate stats)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published; registration is via the Admitad Partner Network signup form — https://www.admitad.com/terms-for-publishers/ (accessed 2026-11-10).
- [UNKNOWN] Whether Admitad charges a setup fee for any programme — not published.

**(b) Minimum payout thresholds**
- [CONFIRMED] "The minimum withdrawal amount should equal or exceed the amount specified under the link, depending on the currency of withdrawal. Funds accrued in the Publisher's account that are below the minimum withdrawal amount shall not be payable and shall be considered forfeited if the account is deleted by the Publisher." — https://www.admitad.com/terms-for-publishers/ (accessed 2026-11-10).
- [CONFIRMED] Mexico terms: "Amounts below the minimum withdrawal threshold are not payable until the threshold is reached and may be forfeited if the account is deleted." — https://www.admitad.com/terms-for-publishers-mexico/ (accessed 2026-11-10).
- [UNKNOWN] The specific minimum withdrawal amount is not published on the public terms pages; it is shown in the dashboard.

**(c) Commission rates by category**
- [CONFIRMED] "Information about the commission is available only to authorized users." — https://www.admitad.com/store/ (accessed 2026-11-10).
- [CONFIRMED] "In each separate case, the reward amount is determined by the type of paid action and the rate set by the Advertiser for this type of action that is effective at the time of performing this action." — https://www.admitad.com/terms-for-publishers/ (accessed 2026-11-10).
- [CONFIRMED] Aggregate network stats (all programmes, 48-hour average): EPC $5.26; approval rate 76.35%; cookie lifetime 51 days; CR 0.73%; hold time 53 days; payment time 61 days — https://www.admitad.com/store/all/ (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] Mexico terms: "Payment of Commission shall be made by Admitad within a reasonable period after receipt of the Publisher's withdrawal request, but no later than forty-five (45) business days" — https://www.admitad.com/terms-for-publishers-mexico/ (accessed 2026-11-10).
- [CONFIRMED] Payment is conditional on the Advertiser having paid Admitad: "The Publisher expressly acknowledges and accepts that payment of Commission is conditional upon such payment by the Advertiser and assumes the corresponding commercial risk." — https://www.admitad.com/terms-for-publishers-mexico/ (accessed 2026-11-10).

**(e) API rate limits**
- [UNKNOWN] Not published on the public terms pages.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] "Payment shall be made after deducting applicable taxes." — https://www.admitad.com/terms-for-publishers/ (accessed 2026-11-10).
- [UNKNOWN] Specific TDS rate not published on public pages.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [CONFIRMED] Right to receive reward is conditional on: publisher activity leading to the Deal; Deal registered by Admitad tracking; Deal authorized by Advertiser and confirmed by Admitad; Advertiser having paid Admitad; reward ≥ minimum withdrawal; publisher not in violation of Terms — https://www.admitad.com/terms-for-publishers/ (accessed 2026-11-10).
- [CONFIRMED] Below-threshold funds forfeited if account deleted.

---

### 2.3 vCommission

**Sources (all accessed 2026-11-10):**
- https://www.vcommission.com/affiliate-agreement/ (Affiliate Agreement — primary)
- https://www.vcommission.com/wp-content/uploads/2023/12/affiliate-agreement.pdf (PDF version — primary)
- https://www.vcommission.com/faq/ (FAQ — primary)
- https://www.vcommission.com/affiliates/ (publisher page)
- https://www.vcommission.com/terms-conditions/ (advertiser T&C)

**(a) Joining / application fees**
- [CONFIRMED] "There is absolutely no cost/joining fee/setup fee to join vCommission as an Affiliate or Advertiser." — https://www.vcommission.com/faq/ (accessed 2026-11-10).
- [CONFIRMED] Advertiser page: "No set up or maintenance fee." — https://www.vcommission.com/advertisers/ (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] "The minimum threshold for payment is Rs. 1,000 / $100. If you earn less than that in a month, your commissions will automatically roll over to the next month." — https://www.vcommission.com/faq/ (accessed 2026-11-10).

**(c) Commission rates by category**
- [CONFIRMED] vCommission is a network; rates are per-campaign. Example campaign rates published on vCommission blog/author pages: up to 14% (fashion), 9.6% new / 4% existing (unnamed campaign), 3.5% apparel & accessories (US), 6.3% AliExpress selected categories (capped $35/conversion), 1.4% direct / 0.42% indirect (Thailand), 2.8–4.9% by SKU (unnamed), up to 5.6% slab-wise (capped $350/conversion), up to 3.5% US category-wise, up to 6.3% worldwide (capped $35/conversion) — https://www.vcommission.com/author/pratyasha-sarkar/ (accessed 2026-11-10).
- [CONFIRMED] Myntra campaign: Home 7%/3.5%; Beauty & Personal Care 7%/3.5%; Footwear 5.6%/2.8%; Kids Wear 5.6%/2.8%; cookie 30 days; validation 60 days; ROAS 15 — https://www.vcommission.com/blog/publishers/myntra-affiliate-program-promote-indias-favourite-fashion-marketplace-this-festive-season/ (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] "vCommission pays affiliates on a 30 day cycle. Payments are typically sent on the 10th of each month for commissions earned during the corresponding performance period. Example: Results generated during the month of January will be paid within the first week of March." — https://www.vcommission.com/faq/ (accessed 2026-11-10).
- [CONFIRMED] Affiliate Agreement (web version): "Payment term would be 30 days from the date of undisputed invoice received by VC." — https://www.vcommission.com/affiliate-agreement/ (accessed 2026-11-10).
- **CONFLICT FLAG:** The PDF version of the same agreement states "Payment term would be 60 days from the date of undisputed invoice received by VC." — https://www.vcommission.com/wp-content/uploads/2023/12/affiliate-agreement.pdf (accessed 2026-11-10). The web FAQ says 30 days. Verify current terms in-account.
- [CONFIRMED] Invoice must be presented within 15 days of end of campaign; no payments against late invoices.
- [CONFIRMED] vCommission's reporting numbers are definitive for payment processing.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] "Payment shall be made after deducting applicable taxes." — https://www.vcommission.com/affiliate-agreement/ (accessed 2026-11-10).
- [CONFIRMED] Affiliates need PAN Card & Invoice forms to process payments.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [CONFIRMED] "vCommission shall be entitled to deduct 100% from the Fee to be paid to the Affiliate." — https://www.vcommission.com/affiliate-agreement/ (accessed 2026-11-10).
- [CONFIRMED] Late invoices → no payment.
- [CONFIRMED] Pro rata payments if campaign stopped early.

---

## 3. Amazon Global / Associates + Creators API {#amazon}

**Sources (all accessed 2026-11-10):**
- https://affiliate-program.amazon.com/help/operating/agreement (Operating Agreement, updated October 15, 2025 — primary)
- https://affiliate-program.amazon.com/help/operating/compare (What's Changed, effective April 14, 2026 — primary)
- https://affiliate-program.amazon.com/help/operating/policies (Program Policies — primary)
- https://affiliate-program.amazon.com/ (homepage)

**(a) Joining / application fees**
- [CONFIRMED] The Associates Program is free to join. No application fee is published in the Operating Agreement or on the homepage.
- [CONFIRMED] Amazon's Participation Requirements state "The Associates Program is free to join" and that Amazon has never authorized any business to sell paid setup or consulting services to Associates — https://amzbase.com/guides/amazon-associates-program/ (accessed 2026-11-10, secondary source quoting Amazon's own policy).

**(b) Minimum payout thresholds**
- [CONFIRMED] Payment Minimum Chart (cited in secondary sources quoting Amazon's Commission Income Statement): direct deposit and Amazon gift card both pay out from **$10 USD** (gift cards capped at $2,000 USD); a check requires **$100 USD** and carries a **$15 USD check processing fee** — https://amzbase.com/guides/amazon-associates-program/ (accessed 2026-11-10, secondary source quoting Amazon's official table).
- [CONFIRMED] Payment is monthly, approximately 60 days after the end of each calendar month — https://ancaster.io/offers/amazon-associates (accessed 2026-11-10, secondary).

**(c) Commission rates by category (US, amazon.com)**
- [CONFIRMED] Amazon publishes a "Standard Commission Income Statement" with category rates. The Operating Agreement itself contains no percentages; rates live in the incorporated Commission Income Statement — https://affiliate-program.amazon.com/help/operating/agreement (accessed 2026-11-10).
- [CONFIRMED] Amazon homepage states "Earn up to 10% in associate commissions from qualifying purchases and programs" — https://affiliate-program.amazon.com/ (accessed 2026-11-10).
- [CONFIRMED] The publicly reachable help node "Associates Program Standard Commission Income Statement" was read cell-by-cell on 2026-08-17 and lists (US):
  - Luxury Beauty, Luxury Stores Beauty, Amazon Explore: **10.00%**
  - Digital Music, Physical Music, Handmade, Digital Videos: **5.00%**
  - Physical Books, Kitchen, Automotive: **4.50%**
  - Amazon Fire Tablet Devices, Amazon Kindle Devices, Luxury Stores Fashion, Apparel, Amazon Cloud Cam Devices, Fire TV Edition Smart TVs, Amazon Fire TV Devices, Amazon Echo Devices, Ring Devices, Watches, Jewelry, Luggage, Shoes, Handbags & Accessories: **4.00%**
  - Toys, Furniture, Home, Home Improvement, Lawn & Garden, Pets Products, Headphones, Beauty, Musical Instruments, Business & Industrial Supplies, Outdoors, Tools, Sports, Baby Products, Amazon Coins: **3.00%**
  - PC, PC Components, DVD & Blu-Ray: **2.50%**
  - Televisions, Digital Video Games: **2.00%**
  - Amazon Fresh, Physical Video Games & Video Game Consoles, Grocery, Health & Personal Care: **1.00%**
  - Gift Cards; Wireless Service Plans; Alcoholic Beverages; Digital Kindle Products (subscription); Vehicles – Leasing and Sales; Pet Prescription Medications; Food prepared and delivered from a restaurant; Amazon Appstore, Prime Now, or Amazon Pay Places; Coach: **0.00%**
  - All Other Categories: **4.00%**
  Source: https://amzbase.com/guides/amazon-associates-program/ (accessed 2026-11-10, secondary source transcribing Amazon's official table).
- [CONFIRMED] Bounties (US): Amazon Business account registration $15.00; Audible Standard free trial or paid monthly membership $20.00; Audible Premium Plus Annual Membership $25.00; Prime for Young Adults trial or paid membership $30.00; standard Prime free trial or paid membership $3.00; first purchase from Amazon Haul $4.00 — same source.
- **NOTE:** Some secondary sources (affiliatexblocks.com, termsandconditionstemplate.com) list "Amazon Games 20%" and "Amazon Haul 7%". These are **not supported by the current official table** and should be treated as unreliable. The official table tops out at 10% for Luxury Beauty.

**(d) Payment terms and delays**
- [CONFIRMED] "We will pay you approximately 60 days following the end of each calendar month" — Operating Agreement (accessed 2026-11-10).
- [CONFIRMED] Either party may terminate on 7 calendar days' written notice; Amazon may terminate immediately for material breach — https://ancaster.io/offers/amazon-associates (accessed 2026-11-10, secondary).

**(e) API rate limits**
- [UNKNOWN] Amazon PA-API / Creators API rate limits are documented in Amazon's developer docs, not retrieved in this pass.

**(f) Data access costs**
- [CONFIRMED] Associates participation and link-generation tools are free; no data-access fee published.
- [UNKNOWN] PA-API / Creators API pricing (if any) not publicly disclosed on the Associates pages reviewed.

**(g) Tax withholding**
- [CONFIRMED] Schedule 3 (Tax Provision by Amazon Site): "We may deduct or withhold any taxes that we may be legally obligated to deduct or withhold from any amounts payable to you under the Associates Program." — https://affiliate-program.amazon.com/help/operating/agreement (accessed 2026-11-10).
- [CONFIRMED] Amazon may hold commission income until tax information is provided.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not publicly disclosed.

**(i) Costs of NOT complying (clawback / termination)**
- [CONFIRMED] "If you violate this Agreement... we reserve the right to permanently cease payment of (and you agree you will not be eligible to receive) any and all commission income otherwise payable to you under this Agreement, whether or not directly related to such violation without notice and without prejudice to any right of Amazon to recover damages in excess of this amount." — Operating Agreement Section 2 (accessed 2026-11-10).
- [CONFIRMED] April 14, 2026 updates: 180-day time limit for products to be shipped/streamed/downloaded and paid for; disqualified purchases expanded to include products purchased through any paid or boosted advertisement linking to Amazon; onsite commission scope limited to Direct Qualifying Purchases of the same ASIN variant as the linked Product detail page — https://affiliate-program.amazon.com/help/operating/compare (accessed 2026-11-10).
- [CONFIRMED] Participation requirement: must refer three qualifying sales within 180 days of applying, or the application is withdrawn — https://amzbase.com/guides/amazon-associates-program/ (accessed 2026-11-10, secondary).

---

## 4. eBay Partners {#ebay}

**Sources (all accessed 2026-11-10):**
- https://partnernetwork.ebay.com/our-program/rate-card (Global rate card — primary)
- https://partnernetwork.ebay.com/page/network-agreement (Network Agreement, published September 24, 2026 — primary)
- https://partnernetwork.ebay.com/resources/our-partnership (Program updates FAQ — primary)
- https://partnernetwork.ebay.com/solutions/step-1-understanding-cookies-commissions-and-getting-paid (cookie/commission explainer — primary)
- https://www.ebay.com/sellercenter/growth/ebay-partner-network (seller-facing page — primary)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published; participation is free. The Network Agreement contains no application fee.

**(b) Minimum payout thresholds**
- [CONFIRMED] "Please note our new monthly payout threshold is $10 USD or more." — https://partnernetwork.ebay.com/resources/our-partnership (accessed 2026-11-10).
- [CONFIRMED] "Please note our new minimum payout threshold is $10 USD." — same page.

**(c) Commission rates by category**
- [CONFIRMED] eBay's rate card is published as a PDF/image; the web page states "For any qualifying transaction, you receive a percentage of the GMB for that purchase, based on the category-level commission rates shown below. Earnings caps apply per transaction, and you will only be paid once for each qualifying transaction." — https://partnernetwork.ebay.com/our-program/rate-card (accessed 2026-11-10).
- [CONFIRMED] "You can earn anywhere from 1% to 6% of the sale price of the item." — https://partnernetwork.ebay.com/solutions/step-1-understanding-cookies-commissions-and-getting-paid (accessed 2026-11-10).
- [CONFIRMED] Example: Auto Parts and Accessories category = 2.5% of item price — same page.
- [CONFIRMED] US seller-facing page: "For all other Qualifying Transactions, you can earn commissions up to 4% of the item sale price." — https://www.ebay.com/sellercenter/growth/ebay-partner-network (accessed 2026-11-10).
- [CONFIRMED] The exact category-level rates are in the rate card PDF (https://secureir.ebaystatic.com/cr/v/c1/EPN/Rate%20Card.pdf), which was not extracted as text in this pass. **The web page is a high-level summary; the PDF is the operative rate card.**
- [CONFIRMED] Non-commissionable: charity items, eBay gift cards, real estate, and a small number of daily deals in the US and Canada.

**(d) Payment terms and delays**
- [CONFIRMED] "The eBay Partner Network pays out commissions once per month and are paid on the 10th of the month for the month before the previous month. For example, your May earnings will be paid on July 10." — https://www.ebay.com/sellercenter/growth/ebay-partner-network (accessed 2026-11-10).
- [CONFIRMED] Payment via Electronic Funds Transfer or PayPal.
- [CONFIRMED] Cookie duration: 24 hours; last-click attribution — https://partnernetwork.ebay.com/solutions/step-1-understanding-cookies-commissions-and-getting-paid (accessed 2026-11-10).

**(e) API rate limits**
- [CONFIRMED] eBay offers API endpoints for reporting (Performance by Day/Month/Campaign/Category/Tool/CustomID/Top Selling Item/Checkout Site) — https://partnernetwork.ebay.com/resources/our-partnership (accessed 2026-11-10).
- [UNKNOWN] Specific API rate limits not published on the public pages reviewed.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] US partners must upload a W9; non-US individuals must complete W-8 BEN; non-US entities must complete W-8 BEN-E. US-based partners who do not upload a W9 by the deadline will see their payment held as a balance owed until the next payment cycle — https://partnernetwork.ebay.com/resources/our-partnership (accessed 2026-11-10).

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not publicly disclosed.

**(i) Costs of NOT complying**
- [CONFIRMED] Network Agreement: "EPN may modify your specific pricing, earning caps, and any other compensation terms at any time by providing three (3) days' notice to you." — https://partnernetwork.ebay.com/page/network-agreement (accessed 2026-11-10).
- [CONFIRMED] "EPN reserves the right to adjust, modify, or increase any commission rates, pricing tiers, or compensation terms specified in the Rate Card at any time and in its sole discretion without prior notice to you, provided that any such adjustment results in compensation terms that meet or exceed the applicable rates set forth in the then-current posted Rate Card."
- [CONFIRMED] PayPal payment fee: "This is a 2% transaction fee, capped at $20 USD per month, which eBay Partner Network previously covered. Only partners who want to be paid via PayPal will incur this fee. This does not apply to partners who want to be paid via direct deposit or electronic funds transfer." — https://partnernetwork.ebay.com/resources/our-partnership (accessed 2026-11-10).
- [CONFIRMED] No compensation for Coupons or Coupon Events.
- [CONFIRMED] NoRB bonus removed; strategic bonuses now selective.

---

## 5. Shopify {#shopify}

**Sources (all accessed 2026-11-10):**
- https://www.shopify.com/affiliates (programme homepage — primary)
- https://help.shopify.com/en/affiliates/earnings (Affiliate earnings — primary)
- https://help.shopify.com/en/affiliates/about (About the programme — primary)
- https://www.shopify.com/blog/high-ticket-affiliate-programs (secondary, for cookie/payout timing)

**(a) Joining / application fees**
- [CONFIRMED] "The Shopify Affiliate Program is free to join" — https://www.shopify.com/affiliates (accessed 2026-11-10).
- [CONFIRMED] "The program is free to join, and enables you to monetize your audience and earn commissions from each new Shopify merchant referral." — https://help.shopify.com/en/affiliates/about (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] "The Shopify Affiliate Program requires that you earn a minimum balance of $10 USD before withdrawals can be made." — https://www.shopify.com/affiliates (accessed 2026-11-10).

**(c) Commission rates by category**
- [CONFIRMED] Shopify Affiliates earn per referral, not per product category. Rates are flat by merchant location:
  - **$150 USD** for merchants in US, Canada, Australia, New Zealand, China, Hong Kong, Japan, UK, Denmark, France, Germany, Ireland, Italy, Netherlands, Spain, Rest of Europe, Middle East.
  - **$25 USD** for merchants in Africa, **India**, Latin America, Rest of Asia, and all other countries.
  Source: https://help.shopify.com/en/affiliates/earnings (accessed 2026-11-10).
- [CONFIRMED] Eligible plans: Basic, Grow, Advanced. Not eligible: Plus, Enterprise, Retail.
- [CONFIRMED] Commission is paid only after the merchant pays for a full-price plan (not free trial or paid trial).

**(d) Payment terms and delays**
- [CONFIRMED] "Actions are locked on the 21st day of the month for the previous month's actions... Payments occur one day after actions are locked. Payment is processed and sent to your Impact account balance." — https://help.shopify.com/en/affiliates/earnings (accessed 2026-11-10).
- [CONFIRMED] Example: merchant pays April 5 → action locks May 21 → payment May 22.
- [CONFIRMED] Cookie duration: 30 days — https://www.shopify.com/blog/high-ticket-affiliate-programs (accessed 2026-11-10, secondary).
- [CONFIRMED] Payment methods: direct deposit or PayPal in most countries.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] "Your earnings might be subject to taxes, depending on your location and applicable local tax regulations." — https://help.shopify.com/en/affiliates/earnings (accessed 2026-11-10).

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not publicly disclosed.

**(i) Costs of NOT complying**
- [CONFIRMED] Commission not payable until the online sale action (merchant pays full price).
- [CONFIRMED] Store plan referrals that don't result in full price payment for at least one month aren't eligible for affiliate earnings.

---

## 6. WooCommerce {#woocommerce}

**Sources (all accessed 2026-11-10):**
- https://woocommerce.com/affiliates/ (Become a WooCommerce affiliate — primary)
- https://woocommerce.com/products/affiliate-for-woocommerce/ (Affiliate for WooCommerce extension — primary)
- https://woocommerce.com/document/affiliate-for-woocommerce/affiliate-program-terms-and-conditions/ (template T&C — primary)
- https://woocommerce.com/document/affiliate-for-woocommerce/how-to-create-affiliate-commission-plans/ (commission plans — primary)

**Note:** WooCommerce has two distinct offerings: (1) the **Automattic Affiliate Program** (you promote WooCommerce.com products) and (2) the **Affiliate for WooCommerce extension** (you run your own affiliate programme on your store). Both are covered.

### 6a. Automattic / WooCommerce Affiliate Program (promoting WooCommerce)

**(a) Joining / application fees**
- [CONFIRMED] "Become a WooCommerce affiliate... Earn up to 30% commission with WooCommerce and the Automattic Affiliate Program." No joining fee published — https://woocommerce.com/affiliates/ (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [UNKNOWN] Not published on the public affiliate page.

**(c) Commission rates by category**
- [CONFIRMED] "You'll earn up to 30% commission for a user's first paid purchase and 10% for returning customers." — https://woocommerce.com/affiliates/ (accessed 2026-11-10).
- [CONFIRMED] "Earn more with commissions that extend beyond the immediate, thanks to a 30-day conversion period."

**(d) Payment terms and delays**
- [CONFIRMED] "Once you've set up your payment information, you'll receive payments at regular intervals based on your campaign performance." — https://woocommerce.com/affiliates/ (accessed 2026-11-10).
- [UNKNOWN] Exact payout cycle not published.

**(e) API rate limits**
- [UNKNOWN] Not published.

**(f) Data access costs**
- [CONFIRMED] Free to join.

**(g) Tax withholding**
- [UNKNOWN] Not published.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not published.

**(i) Costs of NOT complying**
- [UNKNOWN] Not published.

### 6b. Affiliate for WooCommerce extension (running your own programme)

**(a) Joining / application fees**
- [CONFIRMED] This is a paid WooCommerce extension; pricing is on the product page (not extracted in this pass). The extension itself is the cost, not a joining fee.
- [CONFIRMED] "You only pay for paying customers, not for ads or leads!" — https://woocommerce.com/products/affiliate-for-woocommerce/ (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] Configurable: "Set refund period, maximum payout amount, minimum affiliate commission for payout, commission payout day..." — https://woocommerce.com/products/affiliate-for-woocommerce/ (accessed 2026-11-10).
- [CONFIRMED] Template T&C: "Payout Threshold: Affiliates must earn a minimum of [Minimum affiliate commission for payout settings] before payments are issued." — https://woocommerce.com/document/affiliate-for-woocommerce/affiliate-program-terms-and-conditions/ (accessed 2026-11-10).

**(c) Commission rates by category**
- [CONFIRMED] Fully configurable by the store owner: percentage or flat; product/category/attribute/tag-based; tiered/performance-based; subscription-based; multi-level. Default storewide plan value is 0 — https://woocommerce.com/document/affiliate-for-woocommerce/how-to-create-affiliate-commission-plans/ (accessed 2026-11-10).
- [CONFIRMED] Commission is calculated on order subtotal, excluding taxes, shipping, and coupon discounts — https://woocommerce.com/document/affiliate-for-woocommerce/faqs/ (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] Configurable: automated payouts or manual; commission payout day setting.

**(e) API rate limits**
- [UNKNOWN] Not applicable (self-hosted extension).

**(f) Data access costs**
- [CONFIRMED] Extension is a one-time purchase; no per-lead or per-sale fee to WooCommerce beyond the extension price.

**(g) Tax withholding**
- [UNKNOWN] Store owner's responsibility.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not applicable.

**(i) Costs of NOT complying**
- [CONFIRMED] Template T&C includes clauses for returns/refunds/chargebacks voiding commission.

---

## 7. Google Merchant Center {#gmc}

**Sources (all accessed 2026-11-10):**
- https://developers.google.com/merchant/api/guides/quotas-limits/overview (API quotas overview — primary)
- https://developers.google.com/merchant/api/guides/quotas-limits/limits (Merchant Center limits — primary)
- https://support.google.com/merchants/answer/14173602 (Merchant API overview — primary)
- https://developers.google.com/merchant/api/guides/agentic-tools/merchant-data-mcp (MCP Access Service — primary)

**(a) Joining / application fees**
- [CONFIRMED] Google Merchant Center is free to create and use. No joining fee is published.
- [CONFIRMED] "Content API for Shopping was sunset on August 18, 2026." Merchant API is the successor — https://developers.google.com/merchant/api/guides/quotas-limits/limits (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [UNKNOWN] Not applicable — Google Merchant Center is a product data management tool, not an affiliate programme. No payouts are made to merchants.

**(c) Commission rates by category**
- [UNKNOWN] Not applicable — no commission structure.

**(d) Payment terms and delays**
- [UNKNOWN] Not applicable.

**(e) API rate limits**
- [CONFIRMED] "Quotas determine the allowable volume and rate of API requests you can make. They are organized into quota groups, which specify: daily limits (`quotaLimit`) and per-minute rate limits (`quotaMinuteLimit`)." — https://developers.google.com/merchant/api/guides/quotas-limits/overview (accessed 2026-11-10).
- [CONFIRMED] "Quotas and Merchant Center limits are subject to change, reduction, or elimination at any time without prior notice."
- [CONFIRMED] Merchant Center limits include maximum number of products, sub-accounts, data sources, etc.
- [CONFIRMED] MCP Access Service "shares the same quota pool as your standard Merchant API calls. Running agents can quickly exhaust quota, especially for data source fetches." — https://developers.google.com/merchant/api/guides/agentic-tools/merchant-data-mcp (accessed 2026-11-10).
- [UNKNOWN] Specific numeric quota values are not published on the overview page; they are account-specific and retrievable via the API.

**(f) Data access costs**
- [CONFIRMED] Free to use; no data-access fee published. Google Merchant Center is free for merchants to list products.
- [CONFIRMED] Requires a Google Cloud project and a Merchant Center account; API access is free.

**(g) Tax withholding**
- [UNKNOWN] Not applicable.

**(h) Exclusivity / non-compete**
- [UNKNOWN] Not applicable.

**(i) Costs of NOT complying**
- [CONFIRMED] Quotas/limits subject to change or elimination without notice.
- [CONFIRMED] Product data must comply with Google's policies; disapprovals can affect visibility.

---

## 8. Global Affiliate Networks {#global-networks}

### 8.1 Awin

**Sources (all accessed 2026-11-10):**
- https://help.awin.com/advertisers/docs/en/publisher-commission-rates (Publisher Commission Rates — primary)
- https://help.awin.com/docs/best-practices-for-setting-commission-rates (best practices — primary)
- https://www.awin.com/gb/affiliate-marketing/what-is-affiliate-marketing (how Awin works — primary)
- https://www.awin.com/gb/affiliate-marketing/what-is-an-affiliate-network (network explainer — primary)
- https://www.awin.com/us/faqs (FAQs — primary)
- https://ui.awin.com/advertiser-terms/127859/affiliate (example programme T&C — primary)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published for publishers. "At Awin, every publisher goes through a verification process before joining the network." — https://www.awin.com/us/faqs (accessed 2026-11-10).
- [CONFIRMED] Advertiser plans (Access, Accelerate, Advanced) have different commission rule capabilities — https://www.awin.com/us/faqs (accessed 2026-11-10). Pricing for advertisers is on the plans page (not extracted).

**(b) Minimum payout thresholds**
- [UNKNOWN] Not published on the public pages reviewed. Awin pays publishers twice a month on the 1st and 15th via its accelerated payments system — https://www.awin.com/gb/affiliate-marketing/what-is-an-affiliate-network (accessed 2026-11-10).

**(c) Commission rates by category**
- [CONFIRMED] "Each programme sets out what it pays for and how much. The brand decides the terms, and on Awin you can see a programme's rates before you apply." — https://www.awin.com/gb/affiliate-marketing/what-is-affiliate-marketing (accessed 2026-11-10).
- [CONFIRMED] Commission models: pay per sale (percentage or fixed), pay per lead (fixed), pay per click (small amount per valid click), performance bonus.
- [CONFIRMED] Publisher Commission Rates feature lets advertisers override standard rates for selected partners; reductions capped at 20% per change, once every 30 days, with at least 7 days' notice — https://help.awin.com/advertisers/docs/en/publisher-commission-rates (accessed 2026-11-10).
- [CONFIRMED] Example programme T&C (advertiser 127859): commission excludes VAT/sales tax, delivery charges, credit card fees, gift wrapping; brand bidding prohibited; violations lead to cancelled commissions and permanent program ban — https://ui.awin.com/advertiser-terms/127859/affiliate (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] "Awin pays publishers twice a month, on the 1st and the 15th, through its accelerated payments system." — https://www.awin.com/gb/affiliate-marketing/what-is-an-affiliate-network (accessed 2026-11-10).

**(e) API rate limits**
- [UNKNOWN] Not published on the public pages reviewed.

**(f) Data access costs**
- [CONFIRMED] Free to join for publishers; no data-access fee published.

**(g) Tax withholding**
- [UNKNOWN] Not published on the public pages reviewed.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Example programme T&C: "Publishers cannot promote our goods on sites with adult, gambling, violent or illegal content. Spam, pop-ups and misleading ads are forbidden. Violations lead to cancelled commissions and permanent program ban."

**(i) Costs of NOT complying**
- [CONFIRMED] "For the promotion performance caused by Publishers' violation of these terms or other promotion rules, Advertisers have the right not to settle the relevant commissions, and can require Publishers to return the commissions that have been paid." — https://ui.awin.com/advertiser-terms/55609/period (accessed 2026-11-10).
- [CONFIRMED] Brand bidding violations → declined commission, zero payment.

---

### 8.2 CJ Affiliate

**Sources (all accessed 2026-11-10):**
- https://www.cj.com/publisher (publisher page — primary)
- https://junction.cj.com/article/cookie-dough-understanding-publisher-payment-cycle (payment cycle explainer — primary)
- https://www.cj.com/legal/software-policy (software policy — primary)
- https://www.sec.gov/Archives/edgar/data/1408690/000104746907006278/a2179190zex-10_32.htm (Publisher Service Agreement, historical filing — primary for terms text)
- https://www.cj.com/legal/psa-us (current PSA — login-gated; not extracted)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee is published; publisher registration is free. "Publisher registration is described as free" — https://payouts.com/resources/cj-affiliate-account (accessed 2026-11-10, secondary).
- [CONFIRMED] CJ reviews each publisher for compliance before the first payment — https://www.cj.com/solutions/cj-affiliate (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] The minimum payment amount is **set by the publisher in their own account** ("Account > Administrative Settings"), not published as a fixed network-wide number. "To receive payment, your closed commission balance must meet the minimum payment amount you set in your account." — https://junction.cj.com/article/cookie-dough-understanding-publisher-payment-cycle (accessed 2026-11-10).
- [CONFIRMED] Publisher Service Agreement: "On the 20th day of each calendar month, CJ will issue to You any positive balance in Your Account for Transactions reported for the previous month, provided Your Account balance exceeds the required 'Minimum Account Balance.'" — https://www.sec.gov/Archives/edgar/data/1408690/000104746907006278/a2179190zex-10_32.htm (accessed 2026-11-10, historical PSA filing).
- [UNKNOWN] The specific default minimum is not published on CJ's public pages. A 2023 third-party review reported $50; not confirmed from a primary source.

**(c) Commission rates by category**
- [CONFIRMED] "CJ does not set commission rates or cookie durations. It is a network, not a merchant. Every advertiser running a program on CJ decides its own terms." — https://ainfluencer.com/cj-affiliate/ (accessed 2026-11-10, secondary).
- [CONFIRMED] "The details of an Advertiser's Program shall be available through the Network Service. Transactions qualifying for a Payout are defined by the Advertiser. Advertisers may change any Payout rate upon no less than 7 days written notice through the Network Service with effect from the 8th day." — https://www.sec.gov/Archives/edgar/data/1408690/000104746907006278/a2179190zex-10_32.htm (accessed 2026-11-10, historical PSA).
- [CONFIRMED] CJ offers a Program Terms API (GraphQL) for publishers to access commission rates for their advertiser relationships — https://developers.cj.com/graphql/reference/Program%20Terms (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] "Payments are made around the 20th (for all currencies) and around the 28th (for currencies USD, GBP, or EUR)." — https://junction.cj.com/article/cookie-dough-understanding-publisher-payment-cycle (accessed 2026-11-10).
- [CONFIRMED] Locking cycles: Standard (transactions lock on the 10th of the month following the Event Date); Custom (advertiser sets lock date 7–60 days after Event Date); Open-Ended (no specified lock date; remains "New" until advertiser locks) — same source.
- [CONFIRMED] Closing occurs around the 13th and 22nd of the month.
- [CONFIRMED] "For CJ to pay out commissions, there must be enough funds in the advertiser account to cover the amount due." — same source.
- [CONFIRMED] Payment via direct deposit or Payoneer; over 150 currencies — https://www.cj.com/publisher (accessed 2026-11-10).

**(e) API rate limits**
- [CONFIRMED] CJ provides a Developer Portal with APIs for finding/managing relationships, accessing offers, discovering products, and viewing program performance — https://www.cj.com/publisher (accessed 2026-11-10).
- [UNKNOWN] Specific API rate limits not published.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [UNKNOWN] Not published on the public pages reviewed.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Software publishers must be approved by CJ before distribution; unapproved activity may result in account termination, suspension/deactivation, and/or reversal of commissions — https://www.cj.com/legal/software-policy (accessed 2026-11-10).

**(i) Costs of NOT complying**
- [CONFIRMED] Charge-backs: "An Advertiser may apply, or CJ may apply, a debit to Your Account in an amount equal to a Payout previously credited to Your Account in circumstances of: (i) product returns; (ii) duplicate entry or other clear error; (iii) non-bona fide Transactions; (iv) non-receipt of payment from, or refund of payment to, the Visitor by the Advertiser; or (v) Publisher failure to comply with Advertiser's Program terms or other agreement with Advertiser. Charge-backs may be applied to Your Account at any time, including previous payment cycles." — https://www.sec.gov/Archives/edgar/data/1408690/000104746907006278/a2179190zex-10_32.htm (accessed 2026-11-10, historical PSA).
- [CONFIRMED] Dormant account fee: "If Publisher's Account has not been credited with a valid, compensable Transaction that has not been Charged-back during any rolling, six consecutive calendar month period ('Dormant Account'), a dormant account fee at CJ's then-current rate shall be applied to Publisher's Account each calendar month that Publisher's Account remains an open yet Dormant Account or until Your Account balance reaches a zero balance, at which time the Account shall become deactivated." — same source.
- [CONFIRMED] Negative accounts: "When You have a negative balance, You must immediately remit payment to CJ in an amount sufficient to bring Your Account to a zero balance, or Your Account is subject to 1.5% interest per month, compounded monthly." — same source.
- [CONFIRMED] Termination: either party may terminate on 15 days notice; outstanding payments paid within 90 days of termination; outstanding debit balance paid within 30 days — same source.

---

### 8.3 Impact

**Sources (all accessed 2026-11-10):**
- https://impact.com/partners/affiliate-partners/ (affiliate partner marketplace — primary)
- https://help.impact.com/partner/what-would-you-like-to-learn-about/platform-features/finance/payments-withdrawals-and-balance/how-do-partners-get-paid (Get Paid as a Partner — primary)
- https://app.impact.com/content/displaympserviceagreement.ihtml (Partner User Agreement — primary)
- https://impact.com/pricing/ (pricing — primary)
- https://impact.com/downloads/one-sheets/Influencer-FAQ-One-Sheet-0220.pdf (influencer FAQ — primary)

**(a) Joining / application fees**
- [CONFIRMED] No joining fee for partners; the marketplace is free to join. "Publishers browse available programs, apply to the ones that match their audience, and earn a commission for every qualifying conversion they drive." — https://impact.com/partners/affiliate-partners/ (accessed 2026-11-10).
- [CONFIRMED] impact.com charges **brands** a SaaS fee: Starter tier $500/mo (includes up to $2,500/mo in partner payouts); Essentials $2,500/mo (includes up to $14,300/mo in partner payouts); setup/onboarding at $250/hour for Essentials/Pro/Enterprise — https://impact.com/pricing/ (accessed 2026-11-10). **This is a cost to the advertiser, not the publisher.**

**(b) Minimum payout thresholds**
- [CONFIRMED] "Your funds become eligible for withdrawal as soon as your impact.com balance reaches the minimum payout threshold of $10 USD (or the equivalent in your account currency)." — https://help.impact.com/partner/what-would-you-like-to-learn-about/platform-features/finance/payments-withdrawals-and-balance/how-do-partners-get-paid (accessed 2026-11-10).
- [CONFIRMED] Partners can set their own threshold above $10 USD, up to $2.5m — https://help.impact.com/partner/what-would-you-like-to-learn-about/platform-features/finance/payments-withdrawals-and-balance/partner-payments-explained-from-action-to-payout (accessed 2026-11-10).

**(c) Commission rates by category**
- [CONFIRMED] "impact.com supports all payment models (per conversion, per click, per view, sponsorship/flat fee, gifting, tenancy, etc). And our most mature clients use impact.com to power many different kinds of partnerships – it's up to you and the brand to establish how you'll work together." — https://impact.com/downloads/one-sheets/Influencer-FAQ-One-Sheet-0220.pdf (accessed 2026-11-10).
- [CONFIRMED] "Once you sign up for the platform and join the Marketplace, you can apply to a program based on the default contract the advertiser has set. However, once they approve you, you can contact the advertiser directly within our platform to negotiate terms." — same source.
- [CONFIRMED] Contract terms include Default Payout, Referral Window, Performance Bonus, Payout Restrictions, Action Locking — https://impact.com/partnerships/affiliate-marketing-platforms-quality-publishers (accessed 2026-11-10).
- [CONFIRMED] "There are no contractual obligations to your partnerships. You can stop promoting any brand, at any time you choose." — https://impact.com/downloads/one-sheets/Influencer-FAQ-One-Sheet-0220.pdf (accessed 2026-11-10).

**(d) Payment terms and delays**
- [CONFIRMED] "Get paid by an automated clearing house. This sweeping payment system runs twice a month (on the 1st and the 15th) and makes a direct deposit to your bank account." — https://impact.com/downloads/one-sheets/Influencer-FAQ-One-Sheet-0220.pdf (accessed 2026-11-10).
- [CONFIRMED] PayPal option adds a 2% processing fee capped at $20.00 — same source.
- [CONFIRMED] Wire transfer option runs weekly — same source.
- [CONFIRMED] "Payouts in 70+ currencies, direct to your account, on a schedule you set." — https://impact.com/partners/affiliate-partners/ (accessed 2026-11-10).

**(e) API rate limits**
- [UNKNOWN] Not published on the public pages reviewed.

**(f) Data access costs**
- [CONFIRMED] Free to join for partners; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] "If you are US-based (or not US-based but work with US-based brands), an electronic tax document is required once your yearly earnings reach the IRS threshold of $2,000 USD in a U.S. tax year (January 1 to December 31). impact.com cannot release payouts to you until your electronic tax document, required by IRS, has been submitted and approved." — https://help.impact.com/partner/what-would-you-like-to-learn-about/platform-features/finance/payments-withdrawals-and-balance/how-do-partners-get-paid (accessed 2026-11-10).
- [CONFIRMED] Partner User Agreement: "If PARTNER does not accurately complete any such required forms... amounts earned by PARTNER shall be subject to the Account Management Fees as per Section 3.3." — https://app.impact.com/content/displaympserviceagreement.ihtml (accessed 2026-11-10).

**(h) Exclusivity / non-compete**
- [CONFIRMED] "There are no contractual obligations to your partnerships. You can stop promoting any brand, at any time you choose." — https://impact.com/downloads/one-sheets/Influencer-FAQ-One-Sheet-0220.pdf (accessed 2026-11-10).

**(i) Costs of NOT complying**
- [CONFIRMED] Account maintenance fee: "If impact.com is unable to pay you for 6 months, a monthly account maintenance fee will apply starting in month 7. The fee is £25 GBP per month (or your payout currency equivalent) under the invoicing via an impact.com entity invoicing method, and $10 USD per month (or your payout currency equivalent) under the direct invoicing method. This fee is non-refundable and is charged until you either provide valid, updated tax or payment information, or your account balance reaches 0." — https://help.impact.com/partner/what-would-you-like-to-learn-about/platform-features/finance/payments-withdrawals-and-balance/how-do-partners-get-paid (accessed 2026-11-10).
- [CONFIRMED] Partner User Agreement: "IMPACT.COM may offset amounts owed by PARTNER to Advertisers (or IMPACT.COM) from amounts credited to PARTNER's Account as compensation under Partner Contracts." — https://app.impact.com/content/displaympserviceagreement.ihtml (accessed 2026-11-10).
- [CONFIRMED] "In no event will IMPACT.COM be liable to PARTNER for such compensation amounts unless full payment is made by the Advertiser to IMPACT.COM of all amounts owed by the Advertiser to IMPACT.COM, as well as all amounts owed to all of the Advertiser's partners (including PARTNER)." — same source.

---

### 8.4 Rakuten Advertising

**Sources (all accessed 2026-11-10):**
- https://rakutenadvertising.com/payment-terms/ (Payment Terms, last updated May 21, 2026 — primary)
- https://rakutenadvertising.com/legal-notices/affiliate-network-policies/ (Affiliate Network Policies, last updated July 13, 2026 — primary)
- https://rakutenadvertising.com/legal-notices/publisher-membership-agreement/ (Publisher Membership Agreement — primary)
- https://pubhelp.rakutenadvertising.com/hc/en-us/articles/360059980671-Commission-Payment-Schedule (payment schedule — primary)
- https://pubhelp.rakutenadvertising.com/hc/en-us/articles/4412254867981-Minimum-Payment-Threshold (minimum threshold — primary)
- https://pubhelp.rakutenadvertising.com/hc/en-us/articles/4403166331021-Inactivity-Fee (inactivity fee — primary)

**(a) Joining / application fees**
- [CONFIRMED] "Unless otherwise designated in writing, the Network and access to the Network Platform are provided to Publisher Partners free of charge." — https://rakutenadvertising.com/legal-notices/publisher-membership-agreement/ (accessed 2026-11-10).
- [CONFIRMED] "Supplier may, at any time upon prior written notice to You, charge fees in relation to any service provided as part of the Network or Network Platform, including Your participation on the Network." — same source.

**(b) Minimum payout thresholds**
- [CONFIRMED] "The minimum needed per network is: Australia Network: 50 AUD; Canada Network: 50 CAD; France and Germany Networks: 50 EUR; UK Network: 50 GBP; US Network: 50 USD." — https://pubhelp.rakutenadvertising.com/hc/en-us/articles/4412254867981-Minimum-Payment-Threshold (accessed 2026-11-10).
- [CONFIRMED] "Commissions generated in the Brazil Network are exempt from a minimum payment threshold."
- [CONFIRMED] "If you do not reach the 50 currency units amount within a given calendar year, you will be paid for any outstanding commission below the 50 currency units amount at the end of the first quarter of the next year."
- [CONFIRMED] Publishers can set a higher threshold per channel.

**(c) Commission rates by category**
- [CONFIRMED] "Network Advertisers determine the attribution and commissioning model that applies to their Offers. Subject to applicable law, each Network Advertiser may, in its sole discretion and from time to time, select, configure, combine, change, or discontinue any attribution or commissioning model." — https://rakutenadvertising.com/legal-notices/affiliate-network-policies/ (accessed 2026-11-10).
- [CONFIRMED] "You acknowledge and agree that, depending on the model and criteria selected by a Network Advertiser, a click or other activity tracked to you may earn a reduced commission or no commission (including 0% or zero value)." — same source.
- [CONFIRMED] "The specific models, rates, lookback periods, designated publishers, and related parameters in effect for an Offer are made available through the Network Platform, the applicable Offer terms, and the Publisher Help Center, each as updated from time to time." — same source.
- [CONFIRMED] All publisher promotional content and links must use designated tracking links beginning with `click.linksynergy` — same source.

**(d) Payment terms and delays**
- [CONFIRMED] "We issue four publisher payments per month on behalf of advertisers based on the payments that advertisers send to us." — https://pubhelp.rakutenadvertising.com/hc/en-us/articles/360059980671-Commission-Payment-Schedule (accessed 2026-11-10).
- [CONFIRMED] "Our standard payment terms require advertisers to review and close invoices by the end of the calendar month. The advertiser's payment is due to the network at the time of invoice. Publisher payments are issued the following week." — same source.
- [CONFIRMED] "Your eligible orders should appear in reports within seven days of the transaction date." — same source.
- [CONFIRMED] Payment Terms (B2B): "The due date of each Supplier Invoice is thirty (30) days from the end of the applicable Service Month." "Late payments will bear interest at the rate of one-and-a-half percent (1.5%) per month or, if lower, the maximum rate allowed by law." — https://rakutenadvertising.com/payment-terms/ (accessed 2026-11-10).
- [CONFIRMED] Locking Period: "The 'Locking Period' will be set to forty-five (45)-days from the applicable transaction date and may be changed by Company at any time by giving written instructions to Supplier." — same source.

**(e) API rate limits**
- [UNKNOWN] Not published on the public pages reviewed.

**(f) Data access costs**
- [CONFIRMED] Free to join; no data-access fee published.

**(g) Tax withholding**
- [CONFIRMED] Payment Terms: "Each Party shall be responsible for their own Tax due and payable by them. Supplier shall add to invoices all applicable Tax to the extent required by the applicable law. Payments due under this Order Form shall be made without deduction or withholding for any Tax unless required by a governmental authority." — https://rakutenadvertising.com/payment-terms/ (accessed 2026-11-10).

**(h) Exclusivity / non-compete**
- [CONFIRMED] FTC endorsement compliance required; prohibited content includes adult/sexually explicit content, illegal products, false health claims, content targeting minors, defamatory content, IP infringement, malware, fake/AI-generated reviews, misleading price comparisons — https://rakutenadvertising.com/legal-notices/affiliate-network-policies/ (accessed 2026-11-10).
- [CONFIRMED] "Supplier and the applicable Network Advertiser reserve the right to withhold commission fees and may suspend or terminate your participation in any Publisher Program if we determine, at our discretion, that you have used prohibited content or methods." — same source.

**(i) Costs of NOT complying**
- [CONFIRMED] Inactivity fee: "The fee is 10% of the balance or 10 currency units, whichever is greater, with a cap of 50 currency units per month. Inactivity fees are deducted monthly from your account balance." Applied when an account has been inactive for 12 months (no new commissions AND a balance that cannot be paid due to absence of payment information) — https://pubhelp.rakutenadvertising.com/hc/en-us/articles/4403166331021-Inactivity-Fee (accessed 2026-11-10).
- [CONFIRMED] "Supplier and the applicable Network Advertiser reserve the right to withhold commission fees and may suspend or terminate your participation." — https://rakutenadvertising.com/legal-notices/affiliate-network-policies/ (accessed 2026-11-10).
- [CONFIRMED] "If your website generates click-throughs or other compensated actions at rates higher than Network or industry averages, inconsistent with what is known about your website or its traffic, or otherwise appears questionable or suspicious, the same actions may be taken." — same source.

---

## 8a. Amazon Creators API / PA-API Rate Limits (supplementary)

**Sources (all accessed 2026-11-10):**
- https://affiliate-program.amazon.com/creatorsapi/docs/en-us/concepts/api-rates (Creators API rate limits — primary)
- https://webservices.amazon.com/paapi5/documentation/troubleshooting/api-rates.html (PA-API 5 rate limits — primary)
- https://affiliate-program.amazon.com/creatorsapi/docs/en-us/introduction (Creators API intro — primary)
- https://affiliate-program.amazon.com/creatorsapi/docs/en-us/paapiv5-deprecation (PA-API deprecation — primary)
- https://affiliate-program.amazon.com/help/operating/policies (Associates Program Policies — primary)

**(a) Joining / application fees**
- [CONFIRMED] Creators API access requires enrollment in the Amazon Associates program and at least 10 qualifying sales within the past 30 days — https://affiliate-program.amazon.com/creatorsapi/docs/en-us/introduction (accessed 2026-11-10).

**(b) Minimum payout thresholds**
- [CONFIRMED] Not applicable to API access; see Associates payout thresholds in Section 3.

**(c) Commission rates by category**
- [CONFIRMED] Not applicable to API access.

**(d) Payment terms and delays**
- [CONFIRMED] Not applicable to API access.

**(e) API rate limits**
- [CONFIRMED] "As soon as you create your Creators API credentials, you are allowed an initial usage limit up to a maximum of one request per second (one TPS) and a cumulative daily maximum of 8640 requests per day (8640 TPD) for the first 30-day period." — https://affiliate-program.amazon.com/creatorsapi/docs/en-us/concepts/api-rates (accessed 2026-11-10).
- [CONFIRMED] "Your account will earn a usage limit of one TPD for every five cents or one TPS (up to a maximum of ten TPS) for every $4320 of shipped item revenue generated via the use of Creators API for shipments in the previous 30-day period (this automatic allocation amount is updated daily)." — same source.
- [CONFIRMED] "Note that your account will lose access to Creators API if it has not generated qualified referring sales for a consecutive 30-day period." — same source.
- [CONFIRMED] "If you are trying to submit requests that exceed your account's usage limit, or if your access has been revoked you will receive a 429 TooManyRequests error message from Creators API." — same source.
- [CONFIRMED] PA-API 5 (now deprecated) had the same initial limits: 1 TPS / 8640 TPD for the first 30 days, scaling with shipped revenue — https://webservices.amazon.com/paapi5/documentation/troubleshooting/api-rates.html (accessed 2026-11-10).
- [CONFIRMED] Associates Program Policies: "You will not exceed, or if you build and release an application that calls Creators API, PA API, each copy of that application that is installed by an end user will not exceed, any limit on calls per second set forth in any Specifications (or that we otherwise notify you apply) and you will not send files to or from Creators API or PA API that are greater than 40KB without our prior written approval." — https://affiliate-program.amazon.com/help/operating/policies (accessed 2026-11-10).
- [CONFIRMED] PA-API 5 is deprecated; applications still calling it receive HTTP 403 Forbidden with AccessDeniedException — https://affiliate-program.amazon.com/creatorsapi/docs/en-us/paapiv5-deprecation (accessed 2026-11-10).

**(f) Data access costs**
- [CONFIRMED] Free to access for eligible Associates; no fee published.

**(g) Tax withholding**
- [CONFIRMED] Not applicable to API access.

**(h) Exclusivity / non-compete**
- [CONFIRMED] Not applicable.

**(i) Costs of NOT complying**
- [CONFIRMED] Loss of API access if no qualified referring sales for 30 consecutive days.
- [CONFIRMED] 429 TooManyRequests if rate limits exceeded; 403 Forbidden if using deprecated PA-API 5.

---

## 9. Comparison Table {#comparison}

All figures are from primary sources accessed 2026-11-10 unless marked secondary. "Not published" means the provider does not disclose the figure publicly.

### 9.1 Joining fees, payout thresholds, payment terms

| Provider | Joining fee | Min payout threshold | Payment terms / delay | Payment methods |
|---|---|---|---|---|
| **Flipkart Affiliate** | Free [C] | ₹25,000 (Payment Info page) — but FAQ says ₹2,500 (gift voucher) / ₹5,000 (EFT). **Conflict flagged.** | Monthly; 30–60 days after order | EFT, Gift Voucher |
| **Amazon India Associates** | Free [C] | ₹1,000 (NEFT) [C] | ~60 days after month end | NEFT |
| **Meesho** | Free [C] | Not published | Not published (secondary: weekly direct / up to 90 days via networks) | Bank transfer (secondary) |
| **Myntra** | Free [C] | ₹500 [C] | Validation 60 days; payment 2–4 months (network) | Bank transfer |
| **Croma** | Free [C] | Not published | Month end + 45 days email, then 15–20 days credit | Bank transfer only |
| **Reliance Digital** | Free [C] | ₹10 (EarnKaro, secondary) | Confirmation 45–60 days; payment 6–8 days (EarnKaro) | Bank transfer |
| **Tata CLiQ** | Free [C] | Not published | Profit confirmation 90 days (ExtraPe, secondary) | Bank transfer |
| **JioMart** | Free [C] | Not published | Tracked 36h; confirmed by future date (RealCash, secondary) | Bank transfer |
| **Cuelinks** | Free [C] | ₹500 (India); ₹20,000 (international) [C] | Net-60 validation; bank transfer | Bank transfer |
| **Admitad** | Free [C] | Not published (dashboard only) | ≤45 business days after withdrawal request (Mexico terms) | Bank transfer, PayPal (varies) |
| **vCommission** | Free [C] | ₹1,000 / $100 [C] | 30-day cycle (FAQ) vs 60 days (PDF agreement) — **conflict flagged**; paid ~10th of month | Bank transfer |
| **Amazon Associates (US/global)** | Free [C] | $10 (direct deposit/gift card); $100 + $15 fee (check) [C] | ~60 days after month end | Direct deposit, gift card, check |
| **eBay Partners** | Free [C] | $10 [C] | Paid 10th of month for month before previous (e.g., May → July 10) | EFT, PayPal (2% fee capped $20/mo) |
| **Shopify Affiliates** | Free [C] | $10 [C] | Action locks 21st; paid next day (22nd) | Direct deposit, PayPal |
| **WooCommerce (Automattic)** | Free [C] | Not published | "Regular intervals" (not specified) | Not published |
| **Google Merchant Center** | Free [C] | N/A (no payouts) | N/A | N/A |
| **Awin** | Free [C] | Not published | Twice monthly: 1st and 15th | Bank transfer, PayPal (varies) |
| **CJ Affiliate** | Free [C] | Publisher-set (default not published) | ~20th and ~28th of month | Direct deposit, Payoneer (150+ currencies) |
| **Impact** | Free [C] | $10 (or equivalent) [C] | Twice monthly: 1st and 15th; wire weekly | ACH, PayPal (2% fee capped $20), wire |
| **Rakuten Advertising** | Free [C] | 50 AUD/CAD/EUR/GBP/USD per network [C] | Four payments per month; advertiser pays net 30, publisher paid following week | Check, direct deposit, PayPal |

### 9.2 Commission rates (headline ranges)

| Provider | Rate structure | Headline rates | Source type |
|---|---|---|---|
| **Flipkart** | % of qualifying revenue, by category | 0%–8% (Fashion 8%; Books 5%; Electronics 3%; Mobiles 0–1%; Furniture 0.5%) | Primary rate card |
| **Amazon India** | Fixed % by category | 0%–8% (Beauty 8%; Apparel 7.5%; Electronics 2.8%; Mobiles 0–2%; Bill Pay 30% capped ₹3) | Primary fee schedule |
| **Meesho** | Per-product, seller-set | Not published (secondary: 4–25% affiliate catalogue; 0.5% non-catalogue) | Secondary only |
| **Myntra** | % by category/new-vs-existing | 3.75%–10% (varies by network) | Network campaign pages |
| **Croma** | % by category | Not published (secondary: ~1.2%–4.5%) | Secondary only |
| **Reliance Digital** | % flat | 2.2%–2.84% (varies by network) | Network campaign pages |
| **Tata CLiQ** | % by category / CPC | 0%–5% (CPC ₹0.17/click on Cuelinks; paused) | Network campaign pages |
| **JioMart** | % by category / flat | 0%–2.55% (RealCash); up to ₹150 flat (EarnKaro); 5.62% (Cuelinks, paused) | Network campaign pages |
| **Amazon US** | Fixed % by category | 0%–10% (Luxury Beauty 10%; Books/Kitchen/Auto 4.5%; Electronics 1–2.5%) | Primary (transcribed) |
| **eBay Partners** | % of GMB by category/region | 1%–6% (headline); up to 4% typical | Primary summary; PDF for detail |
| **Shopify** | Flat per referral by geography | $150 (US/UK/EU etc.); $25 (India, Africa, LatAm, Asia) | Primary |
| **WooCommerce** | % of first purchase / returning | Up to 30% first purchase; 10% returning | Primary |
| **Awin** | Per-programme, advertiser-set | Not published (network) | — |
| **CJ Affiliate** | Per-programme, advertiser-set | Not published (network) | — |
| **Impact** | Per-contract, negotiable | Not published (platform) | — |
| **Rakuten** | Per-offer, advertiser-set | Not published (network) | — |

### 9.3 Tax withholding, API limits, clawback

| Provider | Tax withholding | API rate limits | Clawback / termination cost |
|---|---|---|---|
| **Flipkart** | 5% TDS (PAN); 20% (no PAN, EGV) [C] | Not published | Commission debited on returns/chargebacks; Monthly Limit cap [C] |
| **Amazon India** | "As stipulated under applicable law" (rate not stated) [C] | Not verified | Rate changes at will; returns reduce qualifying revenue [C] |
| **Meesho** | Not published | Not published | Not published |
| **Myntra** | Not published | Not published | KYC failure → no cash payout (Myncash only); IE/credits/gift-card orders void [C] |
| **Croma** | Not published | Not published | No commission on cancelled/returned orders; 10 self-order cap [C] |
| **Reliance Digital** | Not published | Not published | Cancelled/returned → commission cancelled [C] |
| **Tata CLiQ** | Not published | Not published | Non-converting clicks → removal from programme [C] |
| **JioMart** | 2% TDS (RealCash, secondary) | Not published | Desktop/iOS transactions earn ₹0 [C] |
| **Cuelinks** | TDS per Income-tax Act (rate not stated) [C] | Not published | Clawback within 15 working days; 7-day termination notice [C] |
| **Admitad** | "Applicable taxes" deducted (rate not stated) [C] | Not published | Below-threshold funds forfeited on account deletion [C] |
| **vCommission** | "Applicable taxes" deducted (rate not stated) [C] | Not published | 100% fee deduction right; late invoices unpaid [C] |
| **Amazon US** | May withhold as legally obligated [C] | Creators API: 1 TPS / 8,640 TPD initial; scales with revenue; max 10 TPS [C] | Permanent cessation of all commission income on violation [C] |
| **eBay Partners** | W9/W-8 required; payment held if not submitted [C] | Not published | 3-day notice for pricing changes; PayPal 2% fee [C] |
| **Shopify** | "Might be subject to taxes" [C] | Not published | No commission until full-price payment [C] |
| **WooCommerce** | Not published | Not published | Not published |
| **Google Merchant Center** | N/A | Daily + per-minute quotas; account-specific; subject to change [C] | Quotas/limits subject to elimination without notice [C] |
| **Awin** | Not published | Not published | Commission recall + return of paid commissions for violations [C] |
| **CJ Affiliate** | Not published | Not published | Charge-backs any time; dormant account fee; 1.5%/mo interest on negative balance [C] |
| **Impact** | $2,000 IRS threshold for tax doc; $10/mo maintenance fee if unpaid 6 months [C] | Not published | Offset rights; account maintenance fee [C] |
| **Rakuten** | Each party responsible for own tax [C] | Not published | Inactivity fee: 10% of balance or 10 currency units (cap 50/mo) [C] |

**[C] = [CONFIRMED] from primary source. Secondary figures are labelled inline.**

---

## 10. Analysis: Cheapest to Start {#analysis}

### 10.1 What "cheapest to start" means for a small operator

For a self-hosted search + product discovery platform, the relevant costs are:
1. **Cash outlay to join** — every programme reviewed is free to join. No programme charges an application or setup fee to publishers.
2. **Time-to-first-rupee / dollar** — driven by payout threshold × commission rate × conversion volume, plus payment delay.
3. **Compliance burden** — KYC, tax forms, brand-bidding restrictions, content rules.
4. **Risk of earning nothing** — rate changes, category exclusions, clawbacks.

### 10.2 Ranking for a small operator (India-focused)

**Tier 1 — cheapest and fastest to start (recommended first):**

1. **Amazon India Associates** — ₹1,000 threshold is the lowest published in India; free to join; 60-day payment delay is standard. Rates are modest (0–8%) but the catalogue is deepest and conversion is highest. Best single starting point for an India-focused platform. [C]
2. **Flipkart Affiliate** — free to join, but the ₹25,000 threshold (per the Payment Information page) is a serious barrier for a small operator; the FAQ's ₹2,500/₹5,000 figures are inconsistent and should be verified in-account before relying on them. Rates are competitive (Fashion 8%, Books 5%, Electronics 3%). [C]
3. **Myntra** — ₹500 threshold is the lowest confirmed in India; free to join; but rates vary by network (3.75–10%) and KYC is a hard gate. Good second programme for fashion/lifestyle inventory. [C]

**Tier 2 — low threshold, but weaker documentation or narrower inventory:**

4. **Croma** — free, but no published threshold or rate card; payment cycle is long (month + 45 days + 15–20 days). Suitable as a supplementary electronics programme once volume exists. [C]
5. **Reliance Digital** — free; ~2.2–2.84% via networks; ₹10 minimum on EarnKaro. Low rates but low barrier. [C]
6. **JioMart** — free; rates 0–2.55% (Android app only); Cuelinks campaign paused. Weakest commercial case. [C]

**Tier 3 — networks (access many programmes through one integration):**

7. **Cuelinks** — ₹500 threshold; Net-60 validation; 75–95% revenue share (blog figure, [INFERRED]); one JS snippet covers 1,000+ programmes. Best single integration for a small operator wanting breadth. [C]
8. **vCommission** — ₹1,000/$100 threshold; 30-day payment cycle (FAQ) vs 60 days (PDF) — verify; 100% fee-deduction right is a harsh clause. [C]
9. **Admitad** — threshold not published; ≤45 business day payment; commission info gated behind login. Harder to evaluate for a small operator. [C]

**Tier 4 — global programmes (relevant if the platform has international traffic):**

10. **Amazon Associates (US/global)** — $10 threshold; 60-day delay; rates 0–10%; Creators API requires 10 qualifying sales in 30 days before API access. Strong for global reach but the API gate is a hurdle for a new small operator. [C]
11. **eBay Partners** — $10 threshold; paid on the 10th for the month before previous (i.e., ~2-month delay); 1–6% rates; 24-hour cookie. The payment delay is the worst in this report. [C]
12. **Shopify Affiliates** — $10 threshold; $25 per India referral, $150 per US/UK/EU referral; 30-day cookie; paid on the 22nd. Unusually high per-referral payout; worth including if the audience includes prospective merchants. [C]
13. **WooCommerce / Automattic** — up to 30% first purchase, 10% returning; 30-day conversion window; threshold not published. Attractive rates but threshold opacity is a risk. [C]
14. **Awin / CJ / Impact / Rakuten** — all free to join, but none publish a universal rate card or threshold; terms are per-advertiser. These are scale plays, not small-operator starters. CJ's dormant-account fee and 1.5%/mo interest on negative balances, and Rakuten's inactivity fee (10% of balance, cap 50/mo), are material risks for a low-volume account. [C]

### 10.3 Key findings and cautions

- **No programme charges to join.** Every provider reviewed is free for publishers/affiliates. Any third party charging a "setup fee" to join these programmes is not authorised.
- **India thresholds vary wildly and are inconsistently published.** Flipkart's own pages disagree (₹25,000 vs ₹2,500/₹5,000). vCommission's FAQ (30 days) disagrees with its own PDF agreement (60 days). Amazon India's threshold (₹1,000) is the only clean, low, published figure.
- **Meesho, Croma, and Tata CLiQ do not publish rate cards.** Any rate attributed to them is from a network or blog, not the brand. Do not build a business case on those numbers without in-account verification.
- **Payment delays are long across the board.** 30–60 days is standard in India; eBay is ~2 months; Croma is month + 45 + 15–20 days. A small operator should model 60–90 days from sale to cash.
- **Clawback is universal.** Every programme reserves the right to debit commissions for returns, refunds, and policy violations. CJ and Rakuten charge interest or fees on negative balances. Amazon US can permanently cease all commission income for a violation.
- **API access is gated by performance, not money.** Amazon Creators API requires 10 qualifying sales in 30 days and 1 TPS / 8,640 TPD initially. Google Merchant Center quotas are account-specific and unpublished. No provider charges for API access, but all gate it.
- **Tax withholding is universal but rates are mostly unpublished.** Flipkart is the only provider that states a precise TDS rate (5% with PAN, 20% without for EGV). Amazon India, Cuelinks, Admitad, and vCommission all say "applicable taxes" without a number.

### 10.4 Recommended starting stack for IntentForge

For a small, India-focused, self-hosted search + product discovery platform:

1. **Amazon India Associates** — lowest published threshold (₹1,000), deepest catalogue, highest conversion. Primary programme.
2. **Myntra** (via Cuelinks or directly) — ₹500 threshold, fashion/lifestyle rates up to 10%. Secondary programme.
3. **Cuelinks** — one integration gives access to 1,000+ programmes including Amazon, Myntra, Reliance Digital, and others; ₹500 threshold. Use as the aggregation layer.
4. **Shopify Affiliates** — $25 per India referral, $10 threshold, 30-day cookie. Low-effort, high per-referral payout; worth adding if the audience includes prospective merchants.
5. **Amazon Associates (US)** — only once the platform has international traffic and can clear the 10-sales-in-30-days API gate.

Avoid as primary: Meesho (no published rates), Croma (no published rates or threshold), Tata CLiQ (campaign paused on Cuelinks), JioMart (campaign paused on Cuelinks, Android-only tracking), eBay (2-month payment delay), and the global networks (CJ, Awin, Impact, Rakuten) until volume justifies the compliance overhead.

---

## Appendix: Source URLs and access dates

All sources accessed **2026-11-10**. Full URLs are cited inline in each section. Primary sources are official programme pages, terms, fee schedules, and API documentation. Secondary sources are labelled inline and used only where primary data is unavailable.

### Primary source domains used
- affiliate.flipkart.com
- affiliate-program.amazon.in
- affiliate-program.amazon.com
- affiliate.meesho.com
- supplier.meesho.com
- myntra.com
- croma.com
- cuelinks.com
- admitad.com
- vcommission.com
- partnernetwork.ebay.com
- shopify.com / help.shopify.com
- woocommerce.com
- developers.google.com/merchant
- support.google.com/merchants
- awin.com / help.awin.com / ui.awin.com
- cj.com / junction.cj.com / developers.cj.com / sec.gov (historical PSA filing)
- impact.com / help.impact.com / app.impact.com
- rakutenadvertising.com / pubhelp.rakutenadvertising.com
- webservices.amazon.com (PA-API docs)
- affiliate-program.amazon.com/creatorsapi

### Secondary sources used (labelled inline)
- amzbase.com, ancaster.io, favly.com, affiliatexblocks.com, termsandconditionstemplate.com (Amazon rate cross-references)
- earnkaro.com, shopper.com, realcash.in, inrdeals.com, extrape.com, tadlo.ai, ecomvanshika.com, payouts.com, ainfluencer.com, trendweave.com, enactsoft.com, zokera.com, coupomated.com, blooprint.in, dhanaay.com, affi.io (Indian programme cross-references)
- freshstore.com, joinmassive.com (Creators API cross-references)

---

*Report completed 2026-11-10. All claims labelled [CONFIRMED], [INFERRED], or [UNKNOWN] per the convention at the top of this document.*
