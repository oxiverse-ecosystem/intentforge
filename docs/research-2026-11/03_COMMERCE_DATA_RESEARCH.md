# Commerce Data Providers & Affiliate Programmes — Evidence Report

**Prepared for:** IntentForge (privacy-respecting independent search + product discovery platform)
**Research date:** 10 November 2026 (all pages accessed on this date unless stated)
**Method:** Primary sources only — official developer documentation, official API references, official terms of service, official affiliate programme agreements, official rate-limit documentation. Every claim is labelled **[CONFIRMED]** (primary source says so), **[INFERRED]** (derived from primary sources), or **[UNKNOWN]** (no primary source found).

---

## 0. Executive summary — the single most important finding

**Almost every commerce data provider in this report forbids the exact thing IntentForge wants to do: build a standing product database / commerce index, redistribute or rehost product data, or aggregate catalogues across merchants.** The restrictions are not incidental — they are the core of the licence grants. A self-hosted search engine that caches product titles, prices, images and descriptions from these sources and serves them to users is, in most cases, outside the permitted use.

The three most load-bearing clauses, quoted verbatim:

1. **Amazon** — Operating Agreement §11(12): *"You will not, without our express prior written approval, sell, resell, redistribute, sublicense, or transfer (i) any Product Advertising Content; or (ii) any application that utilizes the Product Advertising API or Data Feed or that incorporates or displays Product Advertising Content."* [9]
2. **Shopify** — API Terms §3.14: *"not use the Shopify API to conduct any systematic or automated data collection activities (including scraping, data mining, data extraction and data harvesting) or to build any commerce or product index"* [51]; and the Catalog guidelines: *"No redistribution, resale, or bulk export: Don't aggregate, rehost, redistribute, resell, syndicate, or bulk export Catalog results to third parties."* [52]
3. **eBay** — API License Agreement §4.4: *"Commercialize (that is, sell, rent, trade or lease), copy or store the eBay Content, other than for the intermediate purposes allowed by this Agreement"* [45]

**Practical consequence:** a compliant IntentForge cannot simply scrape or API-pull these catalogues into PostgreSQL and serve them. The viable paths are (a) live, per-request proxying with no standing index, (b) link-only monetisation (deep links / affiliate redirects) with no product data stored, or (c) merchant-permission / partner-permission data feeds (Shopify merchant consent, Google Merchant Center feeds you are authorised to manage, WooCommerce stores you own or are authorised by). Each path has its own licence conditions, documented below.

---

## 1. India — Flipkart

### 1.1 Does an official API exist? — Yes [CONFIRMED]

Flipkart operates a documented Affiliate API with Product, Offer and Report API families, in JSON and XML over HTTPS. [2]

| API family | Endpoints (official) |
|---|---|
| Product Feed Listing | `https://affiliate-api.flipkart.net/affiliate/api/<trackingId>.json` / `.xml` |
| Product Feed | `https://affiliate-api.flipkart.net/affiliate/feeds/<trackingId>/category/<category>.json` |
| Delta Feed (changes only) | `https://affiliate-api.flipkart.net/affiliate/deltaFeeds/<trackingId>/category/<category>.json` |
| Search by keyword | `https://affiliate-api.flipkart.net/affiliate/search/json` |
| Search by product ID | `https://affiliate-api.flipkart.net/affiliate/product/json` |
| Feed download (full catalogue ZIP) | `https://affiliate-api.flipkart.net/affiliate/feeds/rawfiles/<category>` |
| All offers / Deals of the Day | `https://affiliate-api.flipkart.net/affiliate/offers/v1/all/json`, `…/dotd/json` |
| Orders report | `https://affiliate-api.flipkart.net/affiliate/report/orders/detail/json` |
| App-install report | `https://affiliate-api.flipkart.net/affiliate/v1/appInstall/json` |

A newer v1.0 variant exists under `/affiliate/1.0/…`. [29][30]

### 1.2 Is product data accessible? — Yes, rich [CONFIRMED]

Product records include: `productId`, `title`, `imageUrls` (multiple resolutions), `categoryPaths`, `maximumRetailPrice`, `flipkartSellingPrice`, `flipkartSpecialPrice`, `productUrl`, `productDescription`, `productBrand`, `inStock`, `isAvailable`, `codAvailable`, `offers`, `discount`, `size`, `color`, `storage`, `productFamily`, `shippingCharges`, `sellerName`, `sellerAverageRating`, `keySpecs`, `detailedSpecs`, `specificationList`. [30]

### 1.3 Rate limits [CONFIRMED]

- **20 calls per second, per affiliate** — stated in both the API Terms of Use ("Affiliate API calls are limited to 20 calls per second") and the FAQ. [1][3]
- Product Feed returns **500 items per request**, non-configurable; paginate via `nextUrl`. [3]
- Search-by-keyword returns **at most 10 products per search**. [3]
- **Product Feed API URLs expire within 10 hours.** [3][29]
- App-install report queryable for a maximum of **3 months**. [3]

### 1.4 Application / approval process [CONFIRMED]

API access requires being a **registered Flipkart affiliate**. To generate a token: register at `https://affiliate.flipkart.com/registerme`, then go to **API > API Token** and click **Generate API Token**. Only one token per affiliate account; generating a new one disables the old. Authentication is via two headers: `Fk-Affiliate-Id` and `Fk-Affiliate-Token`. [4]

### 1.5 Commercial use / redistribution — **restricted** [CONFIRMED]

This is the critical section for IntentForge.

**API Terms of Use — Usage Guidelines** [1]:
> "While using or accessing Flipkart APIs You agree not to:
> - Reverse engineer, decompile, disassemble, or translate the API…
> - **Sub-license, distribute, create derivative works of, assign, or otherwise transfer or provide access to any third-party, except as expressly permitted herein.**
> - Use the same for any illegal, unauthorized, or other improper purposes.
> - **Use it in a similar or competitive platform.**"

**API Terms of Use — License Grant** [1]:
> "The APIs are owned by Flipkart and are licensed to You on a **royalty-free, non-transferable, non-exclusive, non-sub-licensable basis** during the term of this ToU."

**Affiliate Operating Agreement — unsuitable sites** [67]:
> "Unsuitable sites include, but are not limited to, those that:
> - Advertise affiliate links through pop-up windows / pop-unders
> - Advertise affiliate links as a cashback offer…
> - Advertise misleading/wrong offers/deals running on Flipkart…"

**Affiliate Operating Agreement — no self-purchase / no commercial use** [67]:
> "you may not purchase products during sessions initiated through the links on your site for your own use, **for resale or commercial use of any kind**."

**Affiliate Policy (flipkart.com) — price-comparison sites explicitly unsuitable** [32]:
> "Unsuitable sites include, but are not limited to, those that:
> • **list affiliate links on price comparison sites in any manner whatsoever.**"

**But note the carve-out** [67]:
> "For avoidance of doubt, the Search Box Link, which searches the Flipkart Product catalog, or searches of product databases that include Flipkart Products, **such as shopping comparison engines, are still eligible for referral fees.**"

**[INFERRED]** The tension is real and unresolved by the documents: the Affiliate Policy says price-comparison sites are unsuitable, while the Operating Agreement says comparison-engine search is eligible for referral fees. The API FAQ actively promotes comparison shopping: *"it can be used by e-Commerce Websites to enable their users to do comparison shopping by providing comparitive product features, prices, etc."* [3] A search engine that displays Flipkart prices alongside other retailers is exactly the use case the FAQ describes — but the "no similar or competitive platform" and "no sub-license/distribute" clauses in the API ToU are the ones that would be invoked against a standing index. **Do not rely on the FAQ carve-out without written confirmation.**

### 1.6 Fees / commission [CONFIRMED]

- No cost to join. [47]
- Commission varies by category; November 2026 rate card: Fashion & Lifestyle 8%, Books & General Merchandise 5%, Grocery/Supermart 4%, Home 3%, Core/Emerging Electronics 3%, Large Appliances 3%, Small Home Appliances 2%, Furniture 0.5%, Mobile Phones Tier A 1% / Tier B 0.5% / Tier C 0%, Gift Card 0%, Gold/Silver 0.1%. [28]
- Payment thresholds: **₹25,000** for both Gift Voucher and EFT (the FAQ page still shows older ₹2,500/₹5,000 figures — the Payment Information page is the current one). [48]
- **5% TDS** deducted on EFT and Gift Voucher payments if PAN provided; 20% if not. [48]
- Payments made 30–60 days after order, after the 30-day return window. [48]
- Session/attribution window: **30 minutes** from click-through. [47]

### 1.7 Images, descriptions, attribution [CONFIRMED]

- Images are returned as URLs in multiple resolutions (`default`, `low` 200×200, `mid` 400×400, `high` 800×800). [30]
- `productUrl` has the affiliate tracking ID appended automatically. [30]
- The API ToU requires that all API URLs carry your tracking ID; sub-ID tracking parameters can be appended. [3][4]
- **No explicit image-redistribution licence** is granted in the API ToU. [1] The licence is "non-transferable, non-exclusive, non-sub-licensable" and covers the API, not the content. [INFERRED] Images are licensed only for display on your site in connection with a Special Link.

### 1.8 Geographic restrictions [CONFIRMED]

Governing law: India; courts at Bangalore, Karnataka have exclusive jurisdiction. [1] The programme is operated by Flipkart Internet Private Limited. [67]

### 1.9 Alternatives if access is unavailable [CONFIRMED]

Indian affiliate networks that carry Flipkart inventory: **Cuelinks** [35], **Admitad** [36], **vCommission** [26]. These networks provide product feeds and link conversion, and are the practical route if direct Flipkart API access is denied.

---

## 2. India — Amazon India (amazon.in)

### 2.1 Does an official API exist? — Yes, but it changed in 2026 [CONFIRMED]

**PA-API 5.0 is deprecated.** Amazon's own deprecation notice states:
> "The Amazon Product Advertising API 5.0 (PA-API 5) has been **deprecated** and is being replaced by the **Creators API**." [5]

The successor is the **Creators API**, a REST API at `https://creatorsapi.amazon/catalog/v1/...` with operations `SearchItems`, `GetItems`, `GetVariations`, `GetBrowseNodes`. [43][74] It is available for the India marketplace (locale `www.amazon.in`). [43][76]

### 2.2 Is product data accessible? — Yes [CONFIRMED]

High-level resources: `BrowseNodeInfo`, `BrowseNodes`, `Images` (small/medium/large, primary and variants), `ItemInfo` (title, brand, description), `ParentASIN`, `SearchRefinements`, `VariationSummary`, and `OffersV2`. [73]

`OffersV2` returns: `listings[].availability` (type: `IN_STOCK`, `OUT_OF_STOCK`, `PREORDER`, etc.), `listings[].price` (money amount, currency, displayAmount, pricePerUnit, savingBasis, savings), `listings[].condition`, `listings[].dealDetails`, `listings[].isBuyBoxWinner`, `listings[].merchantInfo`. [56]

**Important limitation:** *"OffersV2 provides only featured listings as shown to retail customers on Amazon Detail Page buybox… If there are no featured offers, OffersV2 will show status as out of stock."* [56]

### 2.3 Application / approval process — **gated, multi-step** [CONFIRMED]

Prerequisites [43]:
1. Be enrolled in the Amazon Associates program for your target marketplace.
2. **Have at least 10 qualifying sales within the past 30 days** to access the PA API through the Creators API.
3. Register for API access through Associates Central.
4. Generate API credentials.

Registration is manual and approval-based [69]:
> "Before you register for the Creators API, you must have an Amazon Associates account that has been **reviewed and received final acceptance** into the Amazon Associates Program."
> "**Creators API sign up is available only to associates who have referred qualified sales and have been accepted into the program.**"

Only the **primary account owner** can sign up. Maximum **two applications per store**, each with up to **two sets of credentials**. [69]

The eligibility gate is enforced at runtime: `AccessDeniedException` / `AssociateNotEligible` — *"The current eligibility criteria is that the account must have made 10 qualified sales in the trailing 30 days."* [78]

### 2.4 Rate limits [CONFIRMED]

- Initial allocation: **1 TPS and 8,640 TPD** for the first 30 days. [70]
- Ongoing: **1 TPD for every $0.05, or 1 TPS (max 10 TPS) for every $4,320 of shipped item revenue** in the trailing 30 days, updated daily. [70]
- **Access is lost if no qualified referring sales for a consecutive 30-day period.** [70]
- Throttling returns `429 TooManyRequests` / `ThrottleException`, with optional `retryAfterSeconds`. [78]
- Access tokens valid **1 hour**; the token endpoint itself is rate-limited. [78][75]
- Recommended cache TTLs: **Offers 1 hour; BrowseNodeInfo 1 hour; all other fields (BrowseNodes, DetailPageURL, Images, ItemInfo) 1 day.** [75]

### 2.5 Commercial use / redistribution — **heavily restricted** [CONFIRMED]

**Operating Agreement §11 — Limited License** [9]:
> "Subject to the terms of this Operating Agreement and solely for the limited purposes of advertising Products on, and directing end users to, the Amazon Site in connection with the Program, we hereby grant you a **limited, revocable, non-transferable, non-sublicensable, non-exclusive, royalty-free license** to (a) copy and display the Content solely on your site…"

**§11(12) — the no-redistribution clause** [9]:
> "You will not, without our express prior written approval, **sell, resell, redistribute, sublicense, or transfer** (i) any Product Advertising Content; or (ii) any application that utilizes the Product Advertising API or Data Feed or that incorporates or displays Product Advertising Content."

**§11(13) — caching limits** [9]:
> "You will not store or cache Product Advertising Content consisting of an image, but you may store a link to Product Advertising Content consisting of an image for up to 24 hours.
> You may store other Product Advertising Content that does not consist of images for caching purposes for up to 24 hours, but if you do so you must immediately thereafter refresh and re-display the Product Advertising Content by making a call to the PA API or retrieving a new Data Feed…
> Unless otherwise notified by us, you may store individual Amazon Standard Identification Numbers (ASINs) for an indefinite period until the termination of this License.
> Notwithstanding the foregoing, **if your application includes a client application, the client application may not store or cache Product Advertising Content.**"

**Participation Requirements — additional prohibitions** [10]:
> "9. You will not sell, resell, redistribute, sublicense, or transfer any Content or any application that uses, incorporates, or displays any Content…"
> "10. You will promptly remove from your site and delete or otherwise destroy any Content that is no longer displayed on the Amazon Site or that we notify you is no longer available for your use."
> "28. You will not display or otherwise use any of our customer reviews or star ratings, in part or in whole, on your site unless you have obtained a link to that customer review or star rating through the Product Advertising API…"
> "29. You will not purchase any Product(s) through Special Links for use by you or for resale or commercial use of any kind… Further, you will not offer any Products on your site for resale or commercial use of any kind."
> "31. Unless otherwise agreed by Amazon, **your site must not have price tracking and/or price alerting functionality.**"

**[INFERRED]** Clause 31 is directly relevant to IntentForge: a search engine that tracks and compares prices over time is, on its face, "price tracking functionality". This is a material risk.

### 2.6 Commission (India, Oct–Nov 2026) [CONFIRMED]

From the official Advertising Fee Schedule [31]:

| Category | Rate |
|---|---|
| Beauty / Luxury Beauty | 8% |
| Apparel & Accessories / Luggage & Bags / Watches | 7.5% |
| Jewelry / Shoes and Handbags | 7% |
| Automotive / Baby / Books / Personal Care Appliances / Sports & Fitness | 4.72% |
| Home / Home Improvement / Office Products | 4.25% |
| Furniture / Health & Personal Care / Kitchen & Dining / Pet Supplies | 4% |
| Wireless Accessories | 3.4% |
| Musical Instruments | 3.2% |
| Business & Industrial Supplies | 2.98% |
| Electronics / Lawn & Garden / Video Games | 2.80% |
| Camera | 2.63% |
| Amazon Fresh / Grocery | 2.50% |
| Computers / Home Entertainment / Large Appliances | 2.45% |
| Mobile Phones | 1% (many models 0%, 0.5%, 1.5%, 2%) |
| Bill Payments | 30% (up to ₹3) |
| All Other Categories | 5% |

Excluded products include flight bookings, gift cards, video gaming consoles, Prime membership, Kindle eBooks. [31]

Payment: ~60 days after month end; withheld until **₹1,000** for NEFT. [9]

### 2.7 Geographic restrictions [CONFIRMED]

The Operating Agreement is with **Amazon Seller Services Private Ltd**; the Amazon Site is defined as `www.amazon.in`. [9] Credentials work globally across marketplaces provided you have a valid Partner Tag and Creators API access approved in that region. [71]

### 2.8 Alternatives [CONFIRMED]

- **Amazon global programmes**: the same Creators API is available in US, UK, DE, FR, JP, CA, IT, ES, IN, BR, MX, AU, AE, SG and others. [43][76]
- **Indian affiliate networks** carrying Amazon inventory: Cuelinks [35], Admitad [36], vCommission [26].

---

## 3. India — Meesho

### 3.1 Does an official API exist? — **No public product API** [CONFIRMED]

Meesho's only documented API is a **partner-gated Supplier / Order-Management API** (production host `merchant.meesho.com`, test host `merchant.meeshotest.in`), authenticated with Meesho-issued signed request headers. Credentials (client-id, secret-key) are issued by Meesho during onboarding. **No public self-service OpenAPI is published.** [62]

The supplier panel is at `https://supplier.meesho.com/`. [62]

### 3.2 Is product data accessible? — Not via a public API [CONFIRMED]

The supplier API is for **onboarded Meesho suppliers and integration partners to sync catalog and inventory, fetch and update orders, retrieve shipping labels, and process returns/RTO** — not for reading Meesho's marketplace catalogue. [62]

Third-party scraper APIs exist (e.g. ReefAPI, QuickCommerce API) but these are **unofficial**, not licensed by Meesho, and their use would violate Meesho's Terms. [INFERRED]

### 3.3 Affiliate programme [CONFIRMED]

Meesho runs a **creator/affiliate programme** at `https://affiliate.meesho.com/` — "Promote Meesho products & earn commission", "10K+ Creators Live", with a "Join The Program" call to action. [12] No public API documentation or product feed is published for it. [12]

### 3.4 Commercial use / redistribution [CONFIRMED]

Meesho's Terms and Conditions state the platform is operated by Meesho Technologies Private Limited and that users must be 18+ and able to contract under the Indian Contract Act, 1872. [61] The Terms do not grant any product-data licence. [61]

### 3.5 Geographic restrictions [CONFIRMED]

"Platform is controlled and operated from India and Company makes no representation that the content, information or materials made available herein are appropriate or will be available for use in other locations." [61]

### 3.6 Alternatives [CONFIRMED]

- **Admitad** carries Meesho affiliate inventory with an XML product feed. [36]
- **Cuelinks** and **vCommission** also carry Meesho campaigns. [35][26]

---

## 4. India — Myntra

### 4.1 Does an official API exist? — **No public developer API** [CONFIRMED]

Myntra does not publish a public developer API programme; its developer/API hosts resolve behind Akamai but are not openly documented. [13] (Third-party API directories confirm: "Myntra does not publish a public developer API program".)

### 4.2 Affiliate programme — exists, but **not direct-entry** [CONFIRMED]

Myntra's own T&C describe a **UGC Affiliate Program** under "Myntra Ultimate Glam Clan", run on the Myntra Mobile App, where an "Affiliate" is a user authorised by Myntra to earn commissions. Eligibility: a Creator is automatically promoted to Affiliate status upon approval of their **third (3rd) unique post**. A minimum of **INR 500** in accrued commissions must be reached before payout. KYC via a service provider is mandatory. [13]

Off-app links (WhatsApp, Telegram, Instagram, YouTube) are enabled for affiliates. [13]

**There is no public product feed or API.** Myntra affiliate partnerships are managed through **approved affiliate networks** (Cuelinks, Admitad, vCommission, EarnKaro). [42][68]

### 4.3 Commercial use / redistribution [CONFIRMED]

The T&C govern participation in the UGC programme only; no product-data licence is granted. [13] Myntra's commission rates and tiers "are set by Myntra and may change at any time at Myntra's discretion." [13]

### 4.4 Geographic restrictions [CONFIRMED]

Myntra Designs Private Limited operates the platform; the programme is India-focused. [13]

### 4.5 Alternatives [CONFIRMED]

- **Cuelinks** Myntra campaign: CPS, payout up to 5.40% per sale (Oct 2026), validation 60 days, payment 90 days after validation. [42]
- **Admitad** and **vCommission** also carry Myntra. [36][26]

---

## 5. India — Croma

### 5.1 Does an official API exist? — **No public product API** [CONFIRMED]

Croma runs an affiliate programme but publishes no product API or feed. Its affiliate FAQs describe a dashboard-based programme only. [11]

### 5.2 Affiliate programme [CONFIRMED]

- Sign-up at `https://www.croma.com/affiliate-program`; **free of cost**. [11]
- Approval by "NAP" (Network Affiliate Partner); criteria: "Ensure that all your social media/blog information is true and organic and all your account information is filled out well and correctly." [11]
- Approval takes **up to 2 weeks**; only **one application per email ID**, rejected IDs cannot reapply for **3 months**. [11]
- Eligibility: "individual blogger/influencer/content creator with a social media page where you can promote products". [11]
- After approval: dashboard with link/widget selection, account management, performance reports. [11]
- You can promote "any page link or a specific product on www.croma.com direct PDP, PLP, campaign LP, Offers etc." [11]
- Reports updated every **24–48 hours**. [11]
- Payment cycle: monthly; detailed email by end of month + 45 days; amount credited 15–20 days later. [11]
- Payment requires **bank account details, PAN card, cancelled cheque**; GST invoice if you have a GST number. [11]
- **Only 10 self-orders allowed per year per affiliate.** [11]

### 5.3 Commercial use / redistribution [CONFIRMED]

No product-data licence is granted. The programme is link/widget-based. [11]

### 5.4 Geographic restrictions [CONFIRMED]

Croma is a Tata Sons electronics retail chain operating in India. [11]

### 5.5 Alternatives [CONFIRMED]

- **Cuelinks** Croma campaign: payout up to 5% per sale. [68]
- **Admitad**, **vCommission**, **EarnKaro** also carry Croma. [36][26]

---

## 6. India — Reliance Digital

### 6.1 Does an official API exist? — **No** [CONFIRMED]

No public product API or affiliate API is published.

### 6.2 Affiliate programme — via networks only [CONFIRMED]

Reliance Digital's affiliate programme is available through affiliate networks (Cuelinks lists 2.81% payout, 7-day cookie; EarnKaro lists 2.5%; INRDeals lists 2.2% base). [68] These are network figures, not official Reliance Digital terms. [INFERRED]

### 6.3 Commercial use / redistribution — **explicitly prohibited** [CONFIRMED]

Reliance Digital's Terms of Use state:
> "The license granted to you does not include a license for: (a) **resale of Products or commercial use of the Reliance Digital Platform or Reliance Content**, (b) **any collection and use of product listings, description, or prices**, (c) any use of Reliance Digital Platform, the Services and/or of Reliance Content other than as contemplated in these Terms of Use… (e) **any use of data mining, robots, or similar data gathering and extraction tools** to extract (whether once or many times) any parts of the Reliance Digital Platform, and (e) **creating and/ or publishing your own database that features parts of the Reliance Digital Platform**." [37]

**[CONFIRMED — this is the most explicit prohibition in the entire report.** Clause (b) "any collection and use of product listings, description, or prices" and clause (e) "creating and/or publishing your own database that features parts of the Reliance Digital Platform" are directly on point for a search engine.

### 6.4 Geographic restrictions [CONFIRMED]

Reliance Digital is the electronics retail arm of Reliance Industries, operating in India. [68]

### 6.5 Alternatives [CONFIRMED]

- **Cuelinks**, **Admitad**, **vCommission**, **EarnKaro** carry Reliance Digital. [68][36][26]

---

## 7. India — Tata CLiQ

### 7.1 Does an official API exist? — **No public product API** [CONFIRMED]

No public developer API or product feed is published. Tata CLiQ's affiliate programme is available through affiliate networks (Takeads, Cuelinks, EarnKaro). [41]

### 7.2 Affiliate programme [CONFIRMED]

- **Cuelinks**: CPS, payout up to **5.40% per sale** (Oct 2026), tracking 5 min, validation 60 days, payment 90 days after validation. "Reselling/bulk buying is not allowed." "Brand Bidding / PPC / Meta ads etc is strictly prohibited." [41]
- **Takeads**: CPA, available GEOs IN, "affiliate program is available at Takeads for approved publishers"; publishers must sign up and get their platform approved. [41]

### 7.3 Commercial use / redistribution [CONFIRMED]

No product-data licence is granted. [41]

### 7.4 Geographic restrictions [CONFIRMED]

Tata CLiQ is India-focused; Takeads lists "Available GEOs: IN". [41]

### 7.5 Alternatives [CONFIRMED]

- **Cuelinks**, **Admitad**, **vCommission**, **EarnKaro** carry Tata CLiQ. [41][36][26]

---

## 8. India — JioMart

### 8.1 Does an official API exist? — **No** [CONFIRMED]

No public product API or affiliate API is published. JioMart's Terms and Conditions govern use of the portal and app operated by Reliance Retail Limited. [27]

A page exists at `https://www.jiomart.com/s/affiliate-service-provider-in/69670` titled "affiliate service provider in", but it returns only a phone number (1800 890 1222) — no programme details, no API, no terms. [63]

### 8.2 Commercial use / redistribution [CONFIRMED]

JioMart's Terms and Conditions do not grant any product-data licence. [27]

### 8.3 Geographic restrictions [CONFIRMED]

JioMart is operated by Reliance Retail Limited, India. [27]

### 8.4 Alternatives [CONFIRMED]

- **JioMart Digital** has 100K+ retail partner stores; no affiliate API. [INFERRED]
- No Indian affiliate network in this report lists JioMart as a campaign. [UNKNOWN]

---

## 9. India — Affiliate Networks

### 9.1 Cuelinks [CONFIRMED]

- **API:** Monetization API v3, base URL `https://developers.cuelinks.com/pub_api/v3`, JSON, token auth (`Authorization: Token <key>`). 33 endpoints across 12 resource groups. [35]
- **Capabilities:** campaign search across 10,000+ campaigns with real-time EPC; link conversion (any URL → tracked affiliate link); offers/coupons; channels; transactions; reports; invoices; missing-transaction claims. [35]
- **Access:** API key from the publisher dashboard; scopes `read:campaigns`, `write:links`, etc. [35]
- **Rate limits:** not published on the public docs page. [UNKNOWN]
- **Commercial use:** the API is for monetising your own traffic; no redistribution of campaign data. [INFERRED]
- **Geographic:** India-headquartered; campaigns across 100+ networks. [35]

### 9.2 Admitad [CONFIRMED]

- **API:** `https://api.admitad.com`, OAuth 2.0 Bearer token; some endpoints use API key. [18][19]
- **Capabilities:** transactions, clicks, payments, channels, link builder, revenues, programme statuses, deals, product feeds. [36]
- **Access:** publisher account required; API token generated from the user menu → API Credentials. [19]
- **Advertiser APIs:** limited to Accelerate and Advanced plans. [18]
- **Product feed:** "The XML Feed provides structured product data that affiliates can easily import into their websites, apps, or tools. It's an efficient way to showcase up-to-date offers, prices, and inventory at scale." [36]
- **Geographic:** 3,000+ advertisers across 100+ countries; 100,000+ publishers. [36]

### 9.3 vCommission [CONFIRMED]

- **API:** publisher API key + Network ID; S2S postback support. [26]
- **Capabilities:** campaign listing, link generation, conversion attribution, reporting. [26]
- **Access:** publisher account at `vcommission.com`; API key from Tools → APIs. [26]
- **Geographic:** India, Singapore, UAE, UK, USA; 100K+ affiliates. [26]
- **Payment:** NET30. [26]

---

## 10. Global — Amazon (all marketplaces)

### 10.1 Creators API [CONFIRMED]

Same API as §2. Available in US, CA, MX, BR, UK, DE, ES, FR, IN, IT, AE, TR, JP, SG, AU and others. [43][76]

### 10.2 Rate limits [CONFIRMED]

Identical to §2.4: 1 TPS / 8,640 TPD initial; scaled by shipped revenue; 30-day inactivity kills access. [70]

### 10.3 Licence [CONFIRMED]

The IP License is the **locale's Associates Program Operating Agreement** — for India, `https://affiliate-program.amazon.in/help/operating/agreement`. [72] The no-redistribution and caching clauses in §2.5 apply in every locale. [9]

### 10.4 Geographic restrictions [CONFIRMED]

Each locale has its own Operating Agreement and its own IP License. [72] Your credentials work globally, but you need a valid Partner Tag and Creators API access approved in each target region. [71]

---

## 11. Global — eBay

### 11.1 Does an official API exist? — Yes [CONFIRMED]

eBay's Buy APIs (Browse, Feed, Marketing, Deal) are documented and available to eBay Partner Network members. [14][15]

### 11.2 Is product data accessible? — Yes, but production access is gated [CONFIRMED]

> "Many of the Buy APIs are a (Limited Release). The use of eBay's Buy APIs in production is intended for eBay partners only. **You must apply for production access through the eBay Partner Network.**" [14]

### 11.3 Application / approval process — **multi-step, contractual** [CONFIRMED]

> "Users must meet standard eligibility requirements, get approvals from eBay support organizations, and sign contracts with eBay to access the Buy APIs in production. **Meeting the standard eligibility requirements is not a guarantee that production access will be granted.**" [14]

Required accounts: eBay member account, eBay Developers Program account, plus contracts specific to your business model; MNDAs may be required. [14]

Process: (1) sign up for EPN; (2) submit Buy API Application with mocks and data flows; (3) EPN responds within 10 business days; (4) if approved, open a support ticket with eBay Developer Support; (5) sign contracts; (6) production access enabled. [14]

### 11.4 Rate limits [CONFIRMED]

- Free tier: **5,000 API calls per day**. [15]
- Compatible Application Check: up to **100,000 API calls per day**. [15]
- Above 100K: submit a Developer Questionnaire. [15]
- APIs have different rate limits; unused calls do not roll over. [45]

### 11.5 Commercial use / redistribution — **restricted** [CONFIRMED]

**API License Agreement §3.1 — Authorized Use** [45]:
> "eBay grants you a non-exclusive, non-transferable, non-sublicensable… license to use the Developer Tools… solely for the purpose of facilitating your own or Your Users' use of eBay Services, such licensed uses limited to the following (the 'Authorized Use'):
> 1. Enabling your Application to interact with, and ongoing interactions with, eBay's databases…
> 2. **Making limited intermediate copies of eBay Content only as necessary to perform an activity permitted under this API License Agreement. All intermediate copies must be deleted when they are no longer required for the purpose for which they were created;**
> 3. Rearranging or reorganizing eBay Content within your Application consistent with this API License Agreement;
> 4. Displaying eBay Content consistent with this API License Agreement; and
> 5. Using, displaying or modifying eBay Content as expressly authorized by Your Users and consistent with this API License Agreement."

**§4 — Restricted Activities** [45]:
> "1. **Distribute, publish, or allow access or linking to eBay Services, including any API, from any location or source other than your Application.**
> …
> 7. **Sell, rent, trade, distribute, lease (or otherwise commercialize), copy, store or modify eBay Content, other than for the purposes allowed by this API License Agreement.**"

**Network Agreement — EPN Data** [46]:
> "EPN Transaction Data is owned by eBay and is eBay Confidential Information."
> "You acknowledge and agree that EPN Data is owned by EPN, is EPN's Confidential Information, and **you will only use EPN Data for the sole purpose of participating in the Network.** You will immediately discontinue any use of EPN Data and destroy all EPN Data in your possession upon the earlier of (i) EPN's request or (ii) termination of your account from the Network."

**Network Agreement — Promotional Tools** [46]:
> "5. **Not sell, redistribute, sublicense or transfer the Promotional Tools.**"

### 11.6 Geographic restrictions [CONFIRMED]

eBay contracts with the eBay entity determined by your country of residence. [45]

---

## 12. Global — Shopify

### 12.1 Does an official API exist? — Yes, two relevant ones [CONFIRMED]

**Storefront API** (GraphQL, public or private token) — for storefront product data. [16]
**Catalog APIs** (Global Catalog and Storefront Catalog, UCP/MCP) — for AI agents to discover and retrieve products across all Shopify merchants or within a single store. [52]

### 12.2 Is product data accessible? — Yes [CONFIRMED]

Storefront API tokenless access covers Products, Collections, Selling Plans, Search, Pages/Blogs/Articles, Cart. Token-based access adds Product Tags, Metaobjects/Metafields, Menu, Customers. [16]

Global Catalog: `search_catalog`, `lookup_catalog`, `get_product`. Returns products clustered by Universal Product ID (UPID), with variants, prices, availability, checkout URLs, seller info. [52]

### 12.3 Application / approval process [CONFIRMED]

- Storefront API: public access token via `storefrontAccessTokenCreate` mutation; private token via Headless channel or delegate access token. Apps can have a maximum of **100 active storefront access tokens per shop**. [16]
- Catalog APIs: **agent profile** auth (no API key needed) for Global Catalog; agent profile for Storefront Catalog. [52]
- "Keyless catalog access doesn't support rate limit increases." [52]

### 12.4 Rate limits [CONFIRMED]

- Storefront API: tokenless access has a query complexity limit of **1,000**; public access capacity scales with buyer IP. [16]
- Catalog: "Catalog queries are subject to rate limits." No numbers published. [52]

### 12.5 Commercial use / redistribution — **explicitly prohibited** [CONFIRMED]

**API License and Terms of Use §3.14** [51]:
> "not use the Shopify API to conduct any systematic or automated data collection activities (including scraping, data mining, data extraction and data harvesting) **or to build any commerce or product index**, and, in any event, not request more than the minimum amount of data from a Shopify API needed by your Application to provide the intended Application functionality…"

**§3.19** [51]:
> "not, except with Shopify's prior written consent, develop or distribute any Application that has as its primary purpose the transfer, sharing, selling, disclosing or otherwise providing of Merchant Data to any third party."

**§3.24** [51]:
> "not use any information derived from the Shopify API, Merchant Data, any Merchant Store, the Services, or any aspect or portion thereof (including any anonymous, aggregate, or derived forms of data), to: (i) create, develop, train, fine tune, or improve any machine learning or artificial intelligence systems, models, or technologies, including large language models; or (ii) **reproduce substantial portions of a source's product pages, catalogs or content feeds beyond limited previews used for product discovery**, or route purchases through a checkout not authorized by Shopify or the relevant merchant, except with: (a) Shopify's prior written consent, or (b) solely with respect to Merchant Data or a specific Merchant Store, the relevant Merchant's consent."

**Catalog usage guidelines** [52]:
> "**Use Catalog data only for shopper experiences**: Catalog results are provided to show products to buyers directly in your own agent or application and to help them buy.
> **Caching limits**: Product metadata… cache up to 24 hours. Images… cache up to 24 hours. **Price and availability: cache up to 15 minutes. Commission values: don't cache.**
> **Use images only with their product listing**…
> **Keep attribution intact**: Don't remove or change attribution parameters (`shcgid` and `shclid`), checkout URLs, or merchant attribution in Catalog results.
> **No bulk harvesting**: Don't conduct any systematic or automated data collection activities, or request more than the minimum amount of data necessary.
> **No use for training purposes**: Don't use Catalog results to develop, train, or fine-tune any ML or AI models, or **to build a standing product database or commerce index**, without Shopify's or the merchant's consent.
> **No redistribution, resale, or bulk export**: Don't aggregate, rehost, redistribute, resell, syndicate, or bulk export Catalog results to third parties."

**Global Catalog extension — external offers** [53]:
> "**Display only, with no redistribution or training.** External product data is made available solely for display in your own agent, app, or site in response to a shopper's request. You must not redistribute, resell, sublicense, bulk-export, or syndicate external product data. You must not include it in feeds, datasets, or indexes that you provide to others, or use it to train, fine-tune, or evaluate machine-learning models."

### 12.6 Geographic restrictions [CONFIRMED]

Shopify Inc., Ottawa, Ontario, Canada; governed by the laws of the Province of Ontario. [51]

---

## 13. Global — WooCommerce

### 13.1 Does an official API exist? — Yes, two [CONFIRMED]

**WooCommerce REST API** (`/wp-json/wc/v3/…`) — authenticated, full CRUD on products, orders, coupons, customers. [33]
**Store API** (`/wp-json/wc/store/v1/…`) — unauthenticated, public product data for client-side rendering. [34]

### 13.2 Is product data accessible? — Yes [CONFIRMED]

REST API: products, orders, coupons, customers, shipping zones; JSON; pagination via `?page` and `?per_page` (default 10). [33]

Store API: products with prices, images, categories, stock status, ratings, variations; only **published** products are accessible. [34]

### 13.3 Application / approval process [CONFIRMED]

- REST API: generate keys at **WooCommerce > Settings > Advanced > REST API > Add key**; choose a user and Read/Write/Read-Write permission. [33]
- Store API: unauthenticated; no key needed. [34]

### 13.4 Rate limits [CONFIRMED]

Not documented on the official pages. [UNKNOWN]

### 13.5 Commercial use / redistribution [CONFIRMED]

WooCommerce is open-source (GPL). The REST API and Store API are **self-hosted** — you query your own store, or a store that has given you API keys. There is no WooCommerce Inc. licence restricting what you do with data from a store you are authorised to access. [INFERRED]

**However:** if you query a third-party store's REST API, you are bound by that store's own terms and by the WordPress/WooCommerce ecosystem norms. The Store API is explicitly public and unauthenticated, intended for rendering on the store's own frontend. [34]

### 13.6 Geographic restrictions [CONFIRMED]

None — WooCommerce is a self-hosted open-source plugin. [33]

### 13.7 Alternatives [CONFIRMED]

- **WordPress REST API** — the underlying framework. [33]
- **Shopify** — if you want a hosted platform with a public Storefront API. [16]

---

## 14. Global — Google Merchant Center / Google Shopping

### 14.1 Does an official API exist? — Yes, but it changed in 2026 [CONFIRMED]

**Content API for Shopping was sunset on 18 August 2026.** Starting 1 September 2026, requests experience progressive errors (`HTTP 410 Gone`). Full decommissioning in early 2027. [39]

The successor is the **Merchant API** (`https://merchantapi.googleapis.com`), a redesign of the Content API, with sub-APIs for products, promotions, inventories, reviews, local and regional inventories, conversion sources. [38][49]

### 14.2 Is product data accessible? — Yes, but **only your own** [CONFIRMED]

The Merchant API lets you "programmatically manage your Merchant Center accounts, including products, promotions, and inventories". [38] It is a **write-and-read-your-own** API, not a catalogue of other merchants' products. [INFERRED]

Product data specification: `id`, `title`, `link`, `image_link`, `price`, `description`, `availability`, `condition`, `brand`, `gtin`, `mpn`, etc. [17]

### 14.3 Application / approval process [CONFIRMED]

Five steps: (1) create a Merchant Center account; (2) create a Google Cloud project; (3) set up authentication; (4) register as a developer; (5) insert your first product. [49]

### 14.4 Rate limits [CONFIRMED]

Not published on the pages reviewed. [UNKNOWN]

### 14.5 Commercial use / redistribution — **restricted** [CONFIRMED]

**Merchant API Terms and Conditions** [40]:
> "By using this API, you consent to be bound by the Google APIs Terms of Service ('API ToS')."

**Buy on Google Merchant Terms §2.1** [54]:
> "Merchant grants to Google and its affiliates a **royalty-free, perpetual, irrevocable, non-exclusive, worldwide license** to use the Item Content for the limited purpose of operating, promoting, and improving BoG and other Google shopping services."

**Free listings data use** [58]:
> "**Content from Merchant Center**: Google may still use and display certain products and their details you add, for example, titles and images, as described in the Merchant Center terms of service."

**[INFERRED]** The Merchant Center ToS is a **merchant-to-Google** licence, not a Google-to-you licence. There is no API that lets a third party read Google's aggregated product index. The only way to get product data out of Google is if you are the merchant who uploaded it, or if you are authorised to manage a merchant's account. [UNKNOWN — no primary source found for a third-party read API]

### 14.6 Geographic restrictions [CONFIRMED]

Merchant Center Terms of Service are country-specific; India has its own ToS. [57]

---

## 15. Global — Affiliate Networks

### 15.1 Awin [CONFIRMED]

- **API:** `https://api.awin.com`, OAuth 2.0 Bearer token (except Create Transactions API which uses `x-api-key`). [18][19]
- **Capabilities:** performance data, promotions, transaction validation, programme lists, account lists. [18]
- **Access:** token generated from the Awin UI (`https://ui.awin.com/awin-api`); token is user-level, not account-level. [19]
- **Advertiser APIs:** limited to Accelerate and Advanced plans. [18]
- **Rate limits:** not published on the pages reviewed. [UNKNOWN]

### 15.2 CJ Affiliate (Commission Junction) [CONFIRMED]

- **API:** REST and GraphQL. REST base URLs include `https://link-search.api.cj.com`. [20][21]
- **Publisher APIs:** Product Feed API (GraphQL, search by price/currency/country/UPC), Advertiser Lookup, Link Search, Automated Offer Feed. [20]
- **Link Search rate limit:** **25 calls per minute**; publishers only. [21]
- **Access:** Personal Access Token or Developer Key. [20]
- **Data imports:** Product Feeds can be sent to CJ via FTP or HTTP. [20]

### 15.3 Impact [CONFIRMED]

- **API:** `https://api.impact.com`, REST, Basic Auth (AccountSID + AuthToken). [22]
- **Rate limits** [23]:

| Group | Hourly limit |
|---|---|
| Performance detail (`/Actions`, `/Clicks`, some `/Reports`) | 500 |
| Performance aggregate (`/Reports`) | 250 |
| Product Search (`/Catalogs/ItemSearch`, `/Catalogs/*/Items`) | 3,000 |
| Other | 1,000 |
| Report Export (`/ReportExport`) | 100/day |
| Click Export (`/ClickExport`) | 10/day |

- **Capabilities:** programs, ads, tracking links, catalog items, actions, reports. [22]

### 15.4 Rakuten Advertising [CONFIRMED]

- **API:** Developer Portal at `https://developers.rakutenadvertising.com/`; APIs include Advertisers, Advertiser Search, Advanced Reports, Affiliate Conversion Journey, Coupon Feed, Deep Links, Events, Link Locator, Product Search. [24]
- **Product Search API:** searches product name, primary category, short/long description, advertiser keywords; returns max **1,000 results**. [25]
- **Coupon Feed API:** XML only, max **500 results**. [24]
- **Access:** Publisher Dashboard login credentials; API access token required. [24]
- **Rate limits:** not published on the pages reviewed. [UNKNOWN]

---

## 16. Summary matrix

| Provider | Official API? | Product data? | Commercial use? | Approval? | Geo | Rate limit | Affiliate? | Images/descriptions reusable? | Attribution? |
|---|---|---|---|---|---|---|---|---|---|
| **Flipkart** | Yes | Yes (rich) | Restricted — no redistribution, no competitive platform | Yes (affiliate + token) | India | 20 rps | Yes | Not expressly licensed | Tracking ID appended |
| **Amazon India** | Yes (Creators API) | Yes (rich) | **No redistribution**; no price tracking; 24h cache only | Yes (10 sales/30d + manual) | India | 1 TPS / 8,640 TPD initial | Yes | 24h link cache only | Partner tag required |
| **Amazon global** | Yes | Yes | Same as Amazon India | Same | Per-locale | Same | Yes | Same | Same |
| **Meesho** | **No public product API** | No | No licence granted | Supplier onboarding only | India | Unknown | Yes (creator) | No | No |
| **Myntra** | **No public API** | No | No licence granted | Via networks only | India | Unknown | Yes (UGC) | No | No |
| **Croma** | **No public API** | No | No licence granted | Yes (NAP approval) | India | Unknown | Yes | No | No |
| **Reliance Digital** | **No** | No | **Explicitly prohibited** (no collection of listings/prices; no database) | Via networks only | India | Unknown | Yes | No | No |
| **Tata CLiQ** | **No public API** | No | No licence granted | Via networks only | India | Unknown | Yes | No | No |
| **JioMart** | **No** | No | No licence granted | None found | India | Unknown | None found | No | No |
| **eBay** | Yes (Buy APIs) | Yes | **No redistribution**; intermediate copies only | Yes (EPN + contracts) | Per-residence | 5,000/day free; 100,000/day certified | Yes (EPN) | Intermediate copies only | Campaign ID required |
| **Shopify** | Yes (Storefront + Catalog) | Yes | **No commerce/product index**; no redistribution; no bulk export | Yes (agent profile / token) | Global (Canada) | Complexity 1,000 tokenless | Yes (promoted placements) | 24h metadata; 15min price | `shcgid`/`shclid` must be kept |
| **WooCommerce** | Yes (REST + Store) | Yes | **Permitted for stores you authorise** | Yes (API keys) | None | Unknown | No | Yes (self-hosted) | No |
| **Google Merchant** | Yes (Merchant API) | **Your own only** | Merchant→Google licence only | Yes (GCP + MC account) | Per-country | Unknown | No | No third-party read API | No |
| **Awin** | Yes | Yes (network campaigns) | Not redistribution | Yes (token) | Global | Unknown | Yes | Per-network terms | Per-network terms |
| **CJ Affiliate** | Yes | Yes (product feed) | Not redistribution | Yes (token) | Global | 25/min (Link Search) | Yes | Per-advertiser terms | Per-advertiser terms |
| **Impact** | Yes | Yes (catalog items) | Not redistribution | Yes (AccountSID) | Global | 3,000/hr product search | Yes | Per-brand terms | Per-brand terms |
| **Rakuten** | Yes | Yes (product search) | Not redistribution | Yes (token) | Global | Unknown | Yes | Per-advertiser terms | Per-advertiser terms |
| **Cuelinks** | Yes | Yes (campaigns) | Not redistribution | Yes (API key) | India/Global | Unknown | Yes | Per-campaign terms | Per-campaign terms |
| **Admitad** | Yes | Yes (XML feed) | Not redistribution | Yes (OAuth) | Global | Unknown | Yes | Per-advertiser terms | Per-advertiser terms |
| **vCommission** | Yes | Yes | Not redistribution | Yes (API key) | India/Global | Unknown | Yes | Per-campaign terms | Per-campaign terms |

---

## 17. Recommendations for IntentForge

### 17.1 Do NOT build a standing product index from these sources

The clauses quoted in §0 and throughout this report make it clear that caching product titles, prices, images and descriptions from Amazon, Flipkart, eBay, Shopify, Reliance Digital, or any affiliate network into a PostgreSQL database and serving it to users is outside the permitted use in virtually every case.

### 17.2 Viable architectures

**A. Live proxy / per-request passthrough (no standing index)**
- Call the provider API on each user request; display the response; do not persist product data.
- Complies with: eBay's "intermediate copies only" clause [45]; Amazon's 24-hour cache limit [9]; Shopify's "display only" clause [52].
- Does **not** comply with: Flipkart's "no similar or competitive platform" clause [1] if the UI is a comparison engine; Amazon's "no price tracking" clause [10] if you store price history.

**B. Link-only monetisation (no product data stored)**
- Store only affiliate deep links / campaign IDs; redirect users to the retailer.
- Complies with: all providers' linking requirements.
- Revenue: affiliate commission only.

**C. Merchant-permission data feeds**
- **WooCommerce**: if IntentForge is authorised by a store, the REST API is fully permissible. [33]
- **Shopify**: with a merchant's consent, the Storefront API is permissible. [16] The Catalog API's "merchant's consent" carve-out in §3.24(b) [51] is the relevant permission.
- **Google Merchant Center**: if IntentForge is authorised to manage a merchant's account, the Merchant API is permissible. [38]
- **Affiliate networks**: product feeds from Admitad, CJ, Impact, Rakuten are licensed for the publisher's own use; redistribution is not permitted. [36][20][22][24]

**D. Own-catalogue / first-party data**
- IntentForge's own crawled or merchant-supplied data, with explicit licence terms granting the necessary rights.

### 17.3 Immediate blockers to resolve before build

1. **Amazon price-tracking prohibition** [10] — confirm whether IntentForge's feature set falls under "price tracking and/or price alerting functionality".
2. **Flipkart comparison-engine ambiguity** [32][67] — seek written clarification from Flipkart on whether a search engine is a "price comparison site" (unsuitable) or a "shopping comparison engine" (eligible).
3. **Shopify commerce-index prohibition** [51] — confirm that IntentForge's use case is not "building a commerce or product index".
4. **eBay production access** [14] — the Buy APIs are limited-release and require contracts; plan for a 10-business-day+ approval cycle.
5. **Amazon Creators API eligibility** [43] — 10 qualifying sales in 30 days is a hard gate; IntentForge cannot launch with Amazon data on day one.

### 17.4 Data-retention policy implications

Even where caching is permitted, the limits are tight:
- **Amazon**: 24 hours for non-image content; 24 hours for image links; ASINs indefinitely. [9]
- **Shopify Catalog**: 24 hours metadata; 24 hours images; **15 minutes price and availability**; never cache commission values. [52]
- **eBay**: intermediate copies only, deleted when no longer required. [45]
- **Flipkart**: Product Feed URLs expire in 10 hours. [3]

A PostgreSQL-backed search engine with a standing index cannot meet these limits. The architecture must be designed around them from the start.

---

## Sources

[1] https://affiliate.flipkart.com/api-docs/af_tou.html — Flipkart Affiliate API Terms of Use
[2] https://affiliate.flipkart.com/api-docs/af_overview.html — Flipkart Affiliate APIs Overview
[3] https://affiliate.flipkart.com/api-docs/af_faq.html — Flipkart Affiliate API FAQ
[4] https://affiliate.flipkart.com/api-docs/af_register.html — Flipkart Affiliate API Registration
[5] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/introduction — Amazon Creators API Introduction
[6] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/license-agreement — Amazon Creators API License Agreement page
[7] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/concepts/api-rates — Amazon Creators API Rates
[8] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/onboarding/register-for-creators-api — Register for Creators API
[9] https://affiliate-program.amazon.in/help/operating/agreement — Amazon.in Associates Operating Agreement
[10] https://affiliate-program.amazon.in/help/operating/policies — Amazon.in Associates Participation Requirements
[11] https://www.croma.com/affiliate-faqs — Croma Affiliate FAQs
[12] https://affiliate.meesho.com — Meesho Affiliate Web
[13] https://www.myntra.com/tac — Myntra T&C (UGC Affiliate Program)
[14] https://edp.ebay.com/api-docs/buy/static/buy-requirements.html — eBay Buy APIs Requirements
[15] https://ebaypartnernetwork.ebay.com/solutions/step-4-optimizing-and-automating — eBay Partner Network APIs
[16] https://shopify.dev/docs/api/storefront/latest — Shopify Storefront API
[17] https://support.google.com/merchants/answer/7052112?hl=en — Google Merchant Center Product data specification
[18] https://help.awin.com/apidocs — Awin API docs introduction
[19] https://help.awin.com/apis/apidocs/api-authentication — Awin API authentication
[20] https://developers.cj.com — CJ Developer Portal
[21] https://docs.cj.com/docs/linksearch-api — CJ Link Search API
[22] https://integrations.impact.com/partner-api-reference — Impact Partner API Reference
[23] https://integrations.impact.com/partner-api-reference/readme/rate-limits — Impact API Rate Limits
[24] https://pubhelp.rakutenadvertising.com/hc/en-us/articles/5949692220813-Developer-Portal-Overview — Rakuten Advertising Developer Portal Overview
[25] https://pubhelp.rakutenadvertising.com/hc/en-us/articles/5949953174029-Product-Search-API — Rakuten Advertising Product Search API
[26] https://www.vcommission.com/affiliates — vCommission publishers
[27] https://www.jiomart.com/page/terms-and-conditions — JioMart Terms and Conditions
[28] https://affiliate.flipkart.com/commissions — Flipkart Commissions Nov 2026
[29] https://affiliate.flipkart.com/api-docs/af_prod_ref.html — Flipkart Product APIs Reference
[30] https://affiliate.flipkart.com/api-docs/af_glossary.html — Flipkart Affiliate API Glossary
[31] https://affiliate-program.amazon.in/help/node/topic/GRXPHT8U84RAYDXZ — Amazon.in Advertising Fee Schedule Oct-Nov 2026
[32] https://www.flipkart.com/pages/affiliate-policy — Flipkart Affiliate Policy
[33] https://developer.woocommerce.com/docs/apis/rest-api — WooCommerce REST API
[34] https://developer.woocommerce.com/docs/apis/store-api/resources-endpoints/products — WooCommerce Store API Products
[35] https://developers.cuelinks.com/docs — Cuelinks Monetization API v3
[36] https://www.admitad.com/affiliates — Admitad affiliate publishers
[37] https://www.reliancedigital.in/c/terms-of-use — Reliance Digital Terms of Use
[38] https://developers.google.com/merchant/api — Google Merchant API
[39] https://developers.google.com/shopping-content/guides/deprecation-and-sunset — Google Content API sunset
[40] https://developers.google.com/merchant/api/guides/terms — Google Merchant API Terms and Conditions
[41] https://www.cuelinks.com/campaigns/tata-cliq-affiliate-program — Cuelinks Tata Cliq campaign
[42] https://www.cuelinks.com/campaigns/myntra-affiliate-program — Cuelinks Myntra campaign
[43] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/introduction — Amazon.in Creators API Introduction
[44] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/onboarding/sign-up-as-an-amazon-associate — Amazon.in Creators API sign-up
[45] https://developer.ebay.com/develop/apis/api-license-agreement — eBay API License Agreement
[46] https://ebaypartnernetwork.ebay.com/page/network-agreement — eBay Partner Network Network Agreement
[47] https://affiliate.flipkart.com/faqs — Flipkart Affiliate FAQs
[48] https://affiliate.flipkart.com/payment-information — Flipkart Affiliate Payment Information
[49] https://developers.google.com/merchant/api/overview — Google Merchant API Overview
[50] https://www.croma.com/affiliate-program — Croma Affiliate Program
[51] https://www.shopify.com/legal/api-terms — Shopify API License and Terms of Use
[52] https://shopify.dev/docs/agents/catalog — Shopify Catalog API usage guidelines
[53] https://shopify.dev/docs/agents/catalog/global-catalog-extension — Shopify Global Catalog extension
[54] https://www.google.com/shopping/transactions/static/tos/us/9_00_tos.html — Buy on Google Merchant Terms
[55] https://www.shopify.com/partners/terms — Shopify Partner Program Agreement
[56] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/api-reference/resources/offersV2 — Amazon Creators API OffersV2
[57] https://support.google.com/merchants/answer/160173?hl=en — Google Merchant Center Terms of Service
[58] https://support.google.com/merchants/answer/9199328?hl=en — Google Merchant Center free listings data use
[59] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/concepts/best-programming-practices — Amazon Creators API best practices
[60] https://affiliate-program.amazon.com/creatorsapi/docs/en-us/troubleshooting/error-codes-and-messages — Amazon Creators API error codes
[61] https://www.meesho.com/legal/terms-conditions — Meesho Terms and Conditions
[62] https://supplier.meesho.com — Meesho Supplier Panel
[63] https://www.jiomart.com/s/affiliate-service-provider-in/69670 — JioMart affiliate service provider page
[64] https://www.reliancedigital.in/collection/affiliate5 — Reliance Digital affiliate collection
[65] https://www.tatacliq.com/aboutus — Tata CLiQ About Us
[66] https://www.flipkart.com/pages/affiliate-marketing-tnc — Flipkart affiliate marketing T&C
[67] https://affiliate.flipkart.com/terms-and-conditions — Flipkart Affiliate Terms and Conditions
[68] https://www.cuelinks.com/blog/top-black-friday-affiliate-programs-for-india-2025 — Cuelinks India affiliate programs blog
[69] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/onboarding/register-for-creators-api — Amazon.in Register for Creators API
[70] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/concepts/api-rates — Amazon.in Creators API rates
[71] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/frequently-asked-questions — Amazon.in Creators API FAQ
[72] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/license-agreement — Amazon.in Creators API license agreement
[73] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/api-reference — Amazon.in Creators API reference
[74] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/migrating-to-creatorsapi-from-paapi — Amazon.in migration to Creators API
[75] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/concepts/best-programming-practices — Amazon.in Creators API best practices
[76] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/concepts/understanding-versions — Amazon.in Creators API versions
[77] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/get-started/using-sdk — Amazon.in Creators API SDK
[78] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/troubleshooting/error-codes-and-messages — Amazon.in Creators API error codes
[79] https://affiliate-program.amazon.in/help/node/topic/GLL6HEVVWUKMQDDQ — Amazon.in how to increase request rate
[80] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/concepts — Amazon.in Creators API concepts
[81] https://affiliate-program.amazon.in/creatorsapi/docs/en-us/onboarding — Amazon.in Creators API onboarding
