# How Modern Search Engines Acquire and Index Information

**An evidence-based technical report for IntentForge**
*Research date: 10 November 2026 (India Standard Time)*

---

## Scope, method, and evidence labels

This report covers how major search engines and product-discovery platforms discover URLs, crawl and index content, process sitemaps and robots.txt, handle canonicalization and duplicates, render JavaScript, extract structured data, detect changes and recrawl, ingest product feeds and merchant catalogues, update prices and inventory, generate search results, and combine first-party indexes with external providers.

**Evidence labels used throughout:**

- **[CONFIRMED]** — stated by a primary source (official documentation, official engineering blog, academic paper, patent, standards document, or official policy/terms page).
- **[INFERRED]** — well-supported technical reasoning from documented behaviour; not directly stated by the vendor.
- **[UNKNOWN]** — not publicly disclosed or unverified.

**Access date for all sources cited: 10 November 2026.**

**Primary-source hierarchy used:** official vendor documentation and official engineering/policy blogs first; standards documents (sitemaps.org, RFC, schema.org, IndexNow spec) second; peer-reviewed papers and patents third. Vendor marketing pages are used only where they are the vendor's own factual statement about their system and are labelled as such. Third-party reporting is used only to corroborate or date events, and is labelled as such.

---

## 1. Google Search

### 1.1 URL discovery

**[CONFIRMED]** Google states there is no central registry of web pages, so it must constantly look for new and updated pages and add them to its list of known pages. It calls this process "URL discovery." Three discovery paths are documented:

1. Pages Google has already visited (known URLs).
2. Links extracted from a known page to a new page (e.g., a hub/category page linking to a new blog post).
3. Pages submitted as a list via a sitemap.

*Source: Google Search Central, "In-depth guide to how Google Search works" — https://developers.google.com/search/docs/fundamentals/how-search-works (accessed 2026-11-10; page last updated 2025-12-18 UTC).*

**[CONFIRMED]** Google's developer guide for web developers states: "Googlebot navigates from URL to URL by fetching and parsing links, sitemaps, and redirects." It recommends crawlable `<a>` elements with relevant text or image `alt` attributes, and internal linking so every page is reachable from another findable page.

*Source: Google Search Central, "SEO Guide for Web Developers" — https://developers.google.com/search/docs/fundamentals/get-started-developers (accessed 2026-11-10).*

**[CONFIRMED]** Google's SEO Starter Guide states: "Google primarily finds pages through links from other pages it already crawled," and "the vast majority of the new pages Google finds every day are through links."

*Source: Google Search Central, "SEO Starter Guide" — https://support.google.com/webmasters/answer/7451184 (accessed 2026-11-10).*

**[INFERRED]** The link graph is therefore the primary discovery substrate; sitemaps are a supplementary, non-guaranteed discovery hint. This is consistent with Google's own statement that a sitemap "helps search engines discover URLs on your site, but it doesn't guarantee that all the items in your sitemap will be crawled and indexed."

*Source: Google Search Central, "What Is a Sitemap" — https://developers.google.com/search/docs/crawling-indexing/sitemaps/overview (accessed 2026-11-10; last updated 2025-12-10 UTC).*

### 1.2 Crawler fleet and identification

**[CONFIRMED]** Google publishes a list of its common crawlers with exact user-agent strings, robots.txt tokens, and the products affected. The list includes:

| Crawler | robots.txt token | Primary purpose |
|---|---|---|
| Googlebot (Smartphone / Desktop) | `Googlebot` | Google Search, Discover, Images, Video, News |
| Googlebot Image | `Googlebot-Image` (or `Googlebot`) | Google Images, favicons |
| Googlebot Video | `Googlebot-Video` (or `Googlebot`) | Video features |
| Googlebot News | `Googlebot-News` (or `Googlebot`) | Google News (no separate HTTP UA; crawls with Googlebot strings) |
| Google StoreBot | `Storebot-Google` | Google Shopping surfaces |
| Google-InspectionTool | — | Search Console URL inspection |
| GoogleOther | `Googleother` | Other Google products/analysis |
| GoogleOther-Image / -Video | `Googleother-Image` / `-Video` | Other products |
| Google-CloudVertexBot | — | Vertex AI |
| Google-Extended | `Google-Extended` | LLM training control |

*Source: Google Crawling Infrastructure, "List of Google's common crawlers" — https://developers.google.com/crawling/docs/crawlers-fetchers/google-common-crawlers (accessed 2026-11-10; last updated 2026-10-30 UTC).*

**[CONFIRMED]** All common crawlers "always obey robots.txt rules when crawling automatically." They crawl from IP ranges published in `common-crawlers.json`, and reverse DNS matches `crawl-***-***-***-***.googlebot.com` or `geo-crawl-***-***-***-***.geo.googlebot.com`.

*Source: same page.*

**[CONFIRMED]** Googlebot uses an "evergreen" Chromium renderer — the Chrome version in the user agent tracks the latest Chromium release used by Googlebot.

*Source: Google Search Central blog, "The new evergreen Googlebot" (2019) — https://developers.google.com/search/blog/2019/05/the-new-evergreen-googlebot (accessed 2026-11-10); corroborated by the common-crawlers page.*

### 1.3 Crawl scheduling and prioritization (crawl budget)

**[CONFIRMED]** Google defines a site's *crawl budget* as the set of URLs Google can and wants to crawl, determined by two elements:

- **Crawl capacity limit** (also called *hostload*): limits total time the server spends holding connections open, factoring in parallel connections and duration. Starts at a default conservative value; adjusted automatically based on *crawl health* (response consistency, latency, TTFB stability/improvement) and Google's own finite crawling resources. Goes down on server errors (`5xx`) or rate-limiting (`429`).
- **Crawl demand**: for Googlebot, varies with site size, update frequency, page quality, and relevance relative to other sites. Primary influenceable factors: *perceived inventory* (duplicate/undesired URLs waste budget), *popularity* (more popular URLs crawled more often to keep them fresh), and *staleness* (recrawl frequently enough to pick up changes).

**[CONFIRMED]** A *site* is defined as a unique hostname; `www.example.com` and `code.example.com` have separate crawl budgets.

**[CONFIRMED]** Google states: "Even if the crawl capacity limit isn't reached, if crawl demand is low, Google will crawl your site less."

*Source: Google Crawling Infrastructure, "Optimize your crawl budget" — https://developers.google.com/crawling/docs/crawl-budget (accessed 2026-11-10; last updated 2026-07-22 UTC).*

**[CONFIRMED]** The guide is aimed at large sites (1M+ pages, weekly changes) or medium/larger sites (10k+ pages, daily changes), or sites with many "Discovered – currently not indexed" URLs.

**[CONFIRMED]** Best practices Google documents: consolidate duplicate content; block unimportant URLs with robots.txt; return 404/410 for removed pages; eliminate soft 404s; keep sitemaps current with `<lastmod>`; avoid long redirect chains; improve load speed; support `304 (Not Modified)` conditional requests.

*Source: same page.*

**[CONFIRMED]** Google does not accept payment to crawl a site more frequently or rank it higher.

*Source: "In-depth guide to how Google Search works" (see §1.1).*

### 1.4 Sitemap processing

**[CONFIRMED]** Google supports the sitemaps protocol formats: XML, RSS 2.0, mRSS, Atom 1.0, and plain text (one URL per line). XML is the most versatile (supports image, video, news, hreflang extensions). Limits: 50 MB uncompressed or 50,000 URLs per sitemap; larger sites use a sitemap index. Sitemaps must be UTF-8 encoded.

**[CONFIRMED]** Submission methods: Search Console, the Search Console Sitemap API, or a `Sitemap:` line in robots.txt. Cross-submission across multiple sites is supported via Search Console or robots.txt.

**[CONFIRMED]** Sitemaps are a discovery hint, not a guarantee: "A sitemap helps search engines discover URLs on your site, but it doesn't guarantee that all of the items in your sitemap will be crawled and indexed."

**[CONFIRMED]** Google recommends sitemaps for large sites, new sites with few external links, and sites with rich media/news content. A small site (~500 pages or fewer) that is comprehensively internally linked may not need one.

*Source: Google Search Central, "Build and submit a sitemap" — https://developers.google.com/search/docs/crawling-indexing/sitemaps/build-sitemap (accessed 2026-11-10; last updated 2026-07-08 UTC); "What Is a Sitemap" (see §1.1).*

**[CONFIRMED]** Google reads sitemaps regularly; the crawl-budget guide recommends keeping sitemaps up to date and including `<lastmod>` for updated content.

*Source: "Optimize your crawl budget" (see §1.3).*

### 1.5 robots.txt interpretation

**[CONFIRMED]** A robots.txt file tells crawlers which URLs they can access; it is used mainly to avoid overloading the site with requests. It is *not* a mechanism for keeping a page out of Google — use `noindex` or password protection for that.

**[CONFIRMED]** Key documented behaviours:
- A robots.txt-blocked web page's URL can still appear in search results, but without a description.
- A disallowed URL can still be indexed if linked from other sites (URL and anchor text may appear).
- robots.txt rules may not be supported by all search engines; crawlers interpret syntax differently.
- Don't use robots.txt for canonicalization; don't use the URL removal tool for canonicalization.

**[CONFIRMED]** Google publishes a formal "How Google Interprets the robots.txt Specification" document (updated 2026-10-23), and separate pages for creating/submitting robots.txt and an updated-robots.txt report.

*Source: Google Search Central, "Introduction to robots.txt" — https://developers.google.com/search/docs/crawling-indexing/robots/intro (accessed 2026-11-10; last updated 2025-12-10 UTC); Google Crawling Infrastructure, "How Google Interprets the robots.txt Specification" — https://developers.google.com/crawling/docs/robots-txt/robots-txt-spec (accessed 2026-11-10).*

### 1.6 Canonical URL and duplicate handling

**[CONFIRMED]** Canonicalization is "the process of selecting the representative — canonical — URL of a piece of content." Google determines the primary content (*centerpiece*) of each page; if multiple pages seem the same or have very similar primary content, it clusters them and chooses the page that is "objectively the most complete and useful for search users" as canonical.

**[CONFIRMED]** The canonical page is crawled most regularly; duplicates are crawled less frequently to reduce crawling load.

**[CONFIRMED]** Canonicalization signals, in order of strength:
- **Redirects** — strong signal that the redirect target should become canonical.
- **`rel="canonical"` link annotations** (HTML `<link>` or HTTP header) — strong signal.
- **Sitemap inclusion** — weak signal.
- Other signals: HTTPS over HTTP; hreflang cluster membership.

**[CONFIRMED]** Signals can stack. Google may choose a different canonical than the site prefers — "indicating a canonical preference is a hint, not a rule."

**[CONFIRMED]** Duplicate causes documented: region variants, device variants, protocol variants, site functions (sorting/filtering), accidental variants.

**[CONFIRMED]** The canonical page is the main source for content/quality evaluation; a search result usually points to the canonical unless a duplicate is better suited to the user's context (e.g., mobile page for a mobile user).

*Source: Google Search Central, "What is canonicalization" — https://developers.google.com/search/docs/crawling-indexing/canonicalization (accessed 2026-11-10; last updated 2026-08-20 UTC); "How to specify a canonical URL" — https://developers.google.com/search/docs/crawling-indexing/consolidate-duplicate-urls (accessed 2026-11-10; last updated 2026-07-10 UTC).*

**[CONFIRMED]** Google has a named *Deduplication system* in its ranking systems guide (see §1.13).

*Source: Google Search Central, "A guide to Google Search ranking systems" — https://developers.google.com/search/docs/appearance/ranking-systems-guide (accessed 2026-11-10; last updated 2025-12-10 UTC).*

### 1.7 JavaScript rendering

**[CONFIRMED]** Google processes JavaScript web apps in three phases: **crawling → rendering → indexing**. The documented pipeline:

1. Googlebot fetches a URL from the crawl queue via HTTP, first checks robots.txt. If disallowed, it skips the request and the URL.
2. Googlebot parses the response for URLs in `href` attributes of HTML links and adds them to the crawl queue.
3. Googlebot queues all pages with a `200` status for rendering, unless a robots meta tag/header says not to index. The page may wait on this queue for a few seconds or longer.
4. A headless Chromium renders the page and executes JavaScript.
5. Googlebot parses the *rendered* HTML for links again and queues them for crawling; the rendered HTML is also used to index the page.

**[CONFIRMED]** Google recommends server-side or pre-rendering because not all bots can run JavaScript.

**[CONFIRMED]** Google uses an evergreen Chromium version for rendering.

*Source: Google Search Central, "Understand JavaScript SEO basics" — https://developers.google.com/search/docs/crawling-indexing/javascript/javascript-seo-basics (accessed 2026-11-10; last updated 2026-03-04 UTC).*

**[INFERRED]** The two-pass link extraction (raw HTML, then rendered HTML) means JavaScript-injected links are discoverable, but with added latency and resource cost — consistent with Google's advice that rendering is queued and "can take longer."

### 1.8 Structured data extraction

**[CONFIRMED]** Google supports structured data via schema.org vocabularies in JSON-LD (recommended), microdata, and RDFa. Product structured data has two classes:
- **Product snippets** — for pages where people can't directly purchase (more review options).
- **Merchant listings** — for pages where customers can purchase (more detailed product info: apparel sizing, shipping, returns).

**[CONFIRMED]** Google states that providing both structured data on pages *and* a Merchant Center feed "maximizes your eligibility to experiences and helps Google correctly understand and verify your data." Some experiences combine data from both.

**[CONFIRMED]** Merchant listing eligibility requires: single product per page (or variants); a nested `Offer` object; required properties; and Google "may attempt to verify merchant listing product data before showing the information in search results."

**[CONFIRMED]** For Merchant Center structured data, markup must be present in the HTML returned from the web server — it "can't be generated with JavaScript after the page has loaded." Landing page content including pricing must not dynamically change based on user information (IP, browser type). Structured data must match values shown to the user.

*Source: Google Search Central, "Introduction to Product structured data" — https://developers.google.com/search/docs/appearance/structured-data/product (accessed 2026-11-10); "How To Add Merchant Listing Structured Data" — https://developers.google.com/search/docs/appearance/structured-data/merchant-listing (accessed 2026-11-10); Google Merchant Center Help, "Supported structured data attributes and values" — https://support.google.com/merchants/answer/6386198 (accessed 2026-11-10); "Set up structured data for Merchant Center" — https://support.google.com/merchants/answer/7331077 (accessed 2026-11-10).*

**[CONFIRMED]** Google Search Central maintains a structured data feature gallery (search gallery) listing all supported types.

*Source: https://developers.google.com/search/docs/appearance/structured-data/search-gallery (accessed 2026-11-10).*

### 1.9 Content indexing

**[CONFIRMED]** After crawling, Google analyzes textual content and key content tags/attributes (`<title>`, alt attributes, images, videos) and stores the information in the Google index — "a large database hosted on thousands of computers."

**[CONFIRMED]** "Indexing isn't guaranteed; not every page that Google processes will be indexed." Common indexing blockers: low content quality, robots meta rules disallowing indexing, site design making indexing difficult.

**[CONFIRMED]** Technical requirements for indexability: Googlebot isn't blocked; the page returns HTTP 200; the page has indexable content (supported file type, no spam policy violation).

**[CONFIRMED]** Google indexes only publicly accessible pages; login-gated pages are not crawled.

*Source: "In-depth guide to how Google Search works" (see §1.1); Google Search Central, "Google Search Technical Requirements" — https://developers.google.com/search/docs/essentials/technical (accessed 2026-11-10).*

**[CONFIRMED]** Google's indexing infrastructure was re-architected in 2010 as **Caffeine**. Google's announcement states: the old index had several layers refreshed at different rates (main layer every couple of weeks); Caffeine "analyzes the web in small portions and update our search index on a continuous basis, globally," adding new pages/information "straight to the index." Google claimed "50 percent fresher results" and "the largest collection of web content we've offered." At announcement, Caffeine processed "hundreds of thousands of pages in parallel" every second and occupied "nearly 100 million gigabytes of storage in one database," adding "hundreds of thousands of gigabytes per day."

*Source: Google Search Central Blog, "Our new search index: Caffeine" (9 June 2010) — https://developers.google.com/search/blog/2010/06/our-new-search-index-caffeine (accessed 2026-11-10); cross-posted on the Official Google Blog.*

**[UNKNOWN]** Whether "Caffeine" is still the name of Google's current indexing system in 2026, and its current internal architecture. Google has not published an updated indexing-infrastructure announcement since 2010. The name appears in third-party SEO literature (e.g., Moz) as the current index, but that is not a primary source.

### 1.10 Change detection and recrawling

**[CONFIRMED]** Google's systems "want to recrawl documents frequently enough to pick up any changes" (staleness is a crawl-demand factor).

**[CONFIRMED]** Google recommends supporting `304 (Not Modified)` HTTP status codes so Google can reuse a cached version when a page hasn't changed.

**[CONFIRMED]** For most sites, new pages take "several days minimum to be noticed"; Google "strives to check and index pages in a reasonably timely manner. For most sites, this is three days or more." Same-day indexing is not expected except for news sites or extremely time-sensitive content.

**[CONFIRMED]** Site owners can request recrawling of individual URLs via the URL Inspection tool in Search Console (subject to quota) or submit a sitemap for many URLs.

*Source: "Optimize your crawl budget" (see §1.3); Google Search Central, "Troubleshoot Google Search crawling errors" — https://developers.google.com/search/docs/crawling-indexing/troubleshoot-crawling-errors (accessed 2026-11-10); "Ask Google to recrawl your URLs" — https://developers.google.com/search/docs/crawling-indexing/ask-google-to-recrawl (accessed 2026-11-10).*

**[CONFIRMED]** Google does not participate in IndexNow (see §7).

**[INFERRED]** Change detection is therefore driven by a combination of: recrawl scheduling based on staleness/popularity, conditional HTTP requests (If-Modified-Since / ETag → 304), sitemap `<lastmod>` signals, and manual recrawl requests. Google does not publish its change-detection algorithm.

### 1.11 Product feed ingestion (Google Merchant Center)

**[CONFIRMED]** Merchant Center supports multiple product-data input methods:
- **Adding products automatically from your online shop** ("website crawl" / automated data source / "Found by Google").
- **Connecting an ecommerce platform** (Shopify, etc.) to Google.
- **File upload** (.txt, .xml, .tsv; up to 4 GB).
- **Google Sheets** template.
- **Manual entry** one-by-one.
- **Content API for Shopping** (now sunset — see below) / **Merchant API**.
- **Scheduled fetch** of a feed hosted on the merchant's website.
- **Google Cloud Storage (GCS)**.
- **Supplemental feeds** to enrich a primary feed.

*Source: Google Merchant Center Help, "Data source (Previously known as 'Feeds')" — https://support.google.com/merchants/answer/15624855 (accessed 2026-11-10); "Upload a feed" — https://support.google.com/merchants/answer/188477 (accessed 2026-11-10); "Add products from a file to Merchant Center" — https://support.google.com/merchants/answer/12158380 (accessed 2026-11-10).*

**[CONFIRMED]** **Website crawl / automated data source** ("Add products automatically from your online shop"): Google (1) scans the website to find product pages, (2) reads structured data on those pages to extract title, price, availability, etc., (3) formats and delivers that information to Merchant Center. Requires schema.org structured data on all product landing pages, a verified and claimed website URL, and correct robots.txt (Googlebot not blocked). Google also reads the website's sitemaps "to comprehensively crawl your site." Data is refreshed regularly "depending on the traffic that your website receives from Google."

**[CONFIRMED]** To avoid duplicates, products already in another data source are not added automatically; if the same product is both auto-found and merchant-uploaded, Google keeps the merchant-uploaded version and removes the auto-added one.

**[CONFIRMED]** Opting out stops automatic addition, but basic product information may still appear in organic Google search results based on structured data.

*Source: Google Merchant Center Help, "Add products automatically from your online shop" — https://support.google.com/merchants/answer/12158480 (accessed 2026-11-10); "Use automated feeds to build your product data" — https://support.google.com/merchants/answer/7538732 (accessed 2026-11-10).*

**[CONFIRMED]** **Scheduled fetch**: Merchant Center fetches the file from a URL (http/https/sftp) on a schedule; default fetch every 24 hours, adjustable. SFTP delivery syncs each time a file with the matching name is received.

**[CONFIRMED]** **File formats**: .txt, .xml, .tsv, up to 4 GB; non-compressed. Processing takes ~30 minutes before status is visible; data-quality checks may take up to 72 hours to resolve.

*Source: "Upload a feed" (see above); "Create a feed | Shopping Automation" — https://developers.google.com/google-ads/shopping/feed-management/articles/t6 (accessed 2026-11-10).*

**[CONFIRMED]** **Merchant API / Content API sunset**: "Content API for Shopping was sunset on **August 18, 2026**." Starting **September 1, 2026**, Content API requests experience progressive errors. Merchants must migrate to **Merchant API**. The Products sub-API uses two resources: `ProductInput` (raw submitted data; write operations) and `Product` (read-only final processed product built from one primary + zero or more supplemental `ProductInput`s after feed rules and processing). Required `ProductInput` fields include `channel`, `offerId`, `contentLanguage`, `feedLabel`. Batch mode supports many products.

*Source: Google Merchant API, "Manage your products" — https://developers.google.com/merchant/api/guides/products/overview (accessed 2026-11-10; last updated 2026-10-26 UTC).*

**[CONFIRMED]** **Product data specification**: Google uses product data "to match your products to the right queries, and as a foundational input to help optimize and enhance the content of the ads being served in our AI powered formats and experiences." Attributes are defined in the Product data specification; required/optional/conditional per attribute and per target country. Common problems: incorrect `google_product_category` or `gtin`, missing variant attributes (`item_group_id`, `color`, `size`), low-quality images, conflicting data between data source and website.

*Source: Google Merchant Center Help, "Product data specification" — https://support.google.com/merchants/answer/7052112 (accessed 2026-11-10).*

### 1.12 Price and inventory updates

**[CONFIRMED]** **Automatic item updates** (formerly "automated feeds"/"automations"): Google automatically uses landing-page data to update product data for `price`, `sale_price`, `availability`, and `condition`. Enabled by default. Two mechanisms:
1. **Structured data markup** (schema.org `price`, `priceCurrency`, `availability`, `itemCondition`) read by Google's "advanced data extractors."
2. **Advanced data extractors** that "use a combination of statistical models and machine learning to detect and extract product data from your website independent of structured data markup." These work in addition to schema.org markup.

**[CONFIRMED]** Automatic updates are "designed to alleviate issues related to latency or minor mismatches" and are "not a replacement for your feeds or for regular updates." Google recommends continuing to resubmit product data regularly.

**[CONFIRMED]** If automatic item updates are off, or extractors cannot determine price/availability, the product is subject to item-level disapprovals.

**[CONFIRMED]** `strictAvailability` updates (out_of_stock → in_stock) require `allowStrictAvailabilityUpdates` to be set to `true`.

*Source: Google Merchant Center Help, "About automatic item updates" — https://support.google.com/merchants/answer/3246284 (accessed 2026-11-10); "Allow Merchant Center to update product information automatically" — https://support.google.com/merchants/answer/12157888 (accessed 2026-11-10); Google Shopping Content developers, "Enable automatic improvements" — https://developers.google.com/shopping-content/guides/automatic-improvements (accessed 2026-11-10); "Tips to help your products stay approved" — https://support.google.com/merchants/answer/14999713 (accessed 2026-11-10).*

**[CONFIRMED]** **Google StoreBot** is a separate crawler for shopping pages. It "automatically 'crawls' through web pages to gather and analyze data," going through product detail pages, cart pages, and checkout pages, "using machine learning algorithms to fill in forms with information such as shipping addresses." It catalogs: shipping price, product price, online and local availability, validity of coupons, shipping time, shipping countries, logistics, payment methods, secure address/payment, shopping framework, affiliate domains, contact information, meta information. It is used to verify Merchant Center data and keep it accurate.

**[CONFIRMED]** Structured data and Merchant Center data take precedence over what StoreBot extracts. robots.txt rules addressed to `Storebot-Google` affect all Google Shopping surfaces.

**[CONFIRMED]** StoreBot is also used for local inventory: "Google uses Google Storebot to crawl your website to deduce local inventory availability information and create inventory data directly within Merchant Center," crawling the top product/shop pairs per day.

*Source: Google Merchant Center Help, "About the Google StoreBot crawler" — https://support.google.com/merchants/answer/13294660 (accessed 2026-11-10); "In-store inventory automatically added from your online shop" — https://support.google.com/merchants/answer/14871373 (accessed 2026-11-10); Google Crawling Infrastructure, "List of Google's common crawlers" (see §1.2); Google Crawling Infrastructure, "Crawling infrastructure" — https://developers.google.com/crawling (accessed 2026-11-10).*

### 1.13 Search result generation, freshness, and ranking signals

**[CONFIRMED]** When a user enters a query, Google's machines search the index for matching pages and return results believed to be highest quality and most relevant. Relevancy is determined by "hundreds of factors," including user location, language, and device.

**[CONFIRMED]** Google publishes a guide to its ranking systems. Named systems include: BERT, Crisis information systems, Deduplication systems, Exact match domain system, Freshness systems, Link analysis systems and PageRank, Local news systems, MUM, Neural matching, Original content systems, Removal-based demotion systems, Passage ranking system, RankBrain, Reliable information systems, Reviews system, Site diversity system, Spam detection systems. Retired: Helpful content system (evolved into core ranking systems in March 2024), Hummingbird, Panda, Penguin.

**[CONFIRMED]** Ranking systems work at page level using a variety of signals; site-wide signals and classifiers also contribute.

**[CONFIRMED]** Google's "How Search Works" ranking page lists the factors systems look at: words of the query, relevance and usability of pages, expertise of sources, and user location/settings. It breaks these into: relevance of content (keyword matching is "the most basic signal"), quality of content (expertise, authoritativeness, trustworthiness), usability of content (page experience, mobile-friendly, fast loading), and context/settings.

**[CONFIRMED]** Page experience signals include Core Web Vitals, HTTPS, mobile display, ad load, intrusive interstitials. "There is no single signal."

*Source: "In-depth guide to how Google Search works" (see §1.1); "A guide to Google Search ranking systems" (see §1.6); Google, "Automatically generating and ranking results" — https://www.google.com/search/howsearchworks/how-search-works/ranking-results/ (accessed 2026-11-10); Google Search Central, "Understanding page experience in Google Search results" — https://developers.google.com/search/docs/appearance/page-experience (accessed 2026-11-10); "Creating helpful, reliable, people-first content" — https://developers.google.com/search/docs/fundamentals/creating-helpful-content (accessed 2026-11-10).*

**[CONFIRMED]** Freshness is a named ranking system. Google states: "Generally, Bing prefers fresh content" (Bing's own statement, see §2.6) — for Google, the Freshness systems exist but Google does not publish their detailed mechanics.

### 1.14 Combining first-party index with external data providers

**[CONFIRMED]** Google Search primarily serves from its own crawled index. However:
- **Google Merchant Center** combines merchant-submitted feeds, website-crawl extraction (StoreBot/Googlebot), structured data, and automatic item updates.
- **Google News** uses publisher guidelines and PubHub-style inclusion; licensed Microsoft Start content may be shown in Bing (see §2) — for Google, news ranking uses its own index plus publisher-applied inclusion.
- **Google Shopping** combines Merchant Center data with on-page structured data; "Some experiences combine data from structured data and Google Merchant Center feeds."

**[CONFIRMED]** Google states it does not accept payment to crawl more frequently or rank higher. Paid ads are separate from organic ranking.

*Source: "In-depth guide to how Google Search works" (see §1.1); "Introduction to Product structured data" (see §1.8); "Share Your Product Data With Google" — https://developers.google.com/search/docs/specialty/ecommerce/share-your-product-data-with-google (accessed 2026-11-10).*

---

## 2. Microsoft Bing

### 2.1 URL discovery and crawling

**[CONFIRMED]** Bing's crawler is called **Bingbot**. "Crawling is how Bingbot discovers new and updated pages and content to add to our search index. Bingbot uses an algorithm to decide what to crawl and how often, working to minimize its impact on websites... We prioritize relevant known pages that aren't indexed yet and ones we think have been updated to help deliver the freshest results."

**[CONFIRMED]** Bing states it crawls "billions of URLs every day."

*Source: Microsoft Support, "How Bing delivers search results" (last updated March 2025) — https://support.microsoft.com/en-us/bing/how-bing-delivers-search-results (accessed 2026-11-10).*

**[CONFIRMED]** Bing's webmaster blog elaborates the crawl-scheduling algorithm: "The algorithmic process selects URLs to be crawled by prioritizing relevant known URLs that may not be indexed yet, and URLs that have already been indexed that we are checking for updates to ensure that the content is still valid (example not a dead link) and that it has not changed. We also crawl content specifically to discovery links to new URLs that have yet to be discovered. Sitemaps and RSS/Atom feeds are examples of URLs fetched primarily to discovery new links."

**[CONFIRMED]** Bing defines and measures **crawl efficiency**: "how often we crawl and discover new and fresh content per page crawled." North star: "crawl an URL only when the content has been added (URL not crawled before), updated (fresh on-page context or useful outbound links). The more we crawl duplicated, unchanged content, the lower our Crawl Efficiency metric is."

*Source: Bing Webmaster Blog, "bingbot Series: Maximizing Crawl Efficiency" (October 2018) — https://blogs.bing.com/webmaster/2018/10/bingbot-Series-Maximizing-Crawl-Efficiency/ (accessed 2026-11-10).*

### 2.2 Index construction

**[CONFIRMED]** "As Bingbot crawls the web, it sends information to Bing about what it finds. These pages are then added to the Bing index and algorithms are used to analyze the pages so we can effectively include them in search results, including determining which sites, news articles, images, or videos will be included in the index."

*Source: "How Bing delivers search results" (see §2.1).*

**[CONFIRMED]** Bing's Webmaster Guidelines state: "Bing and Copilot search experiences rely on the same core crawling, indexing, and ranking foundation as traditional search."

*Source: Bing Webmaster Tools, "Webmaster Guidelines" — https://www.bing.com/webmasters/help/webmaster-guidelines-30fba23a (accessed 2026-11-10).*

### 2.3 Sitemap processing

**[CONFIRMED]** Bing accepts XML Sitemap, RSS 2.0, Atom 0.3 and 1.0, and plain text (one URL per line). Sitemaps can be submitted via Bing Webmaster Tools, referenced in robots.txt (`Sitemap:` line), imported from Google Search Console, or discovered organically by Bing.

**[CONFIRMED]** "Once your sitemap is referenced in your robots.txt file or submitted through Bing Webmaster Tools, Bing will attempt to fetch it **immediately upon submission**. After that, it will revisit your sitemap on a regular basis, typically **at least once per day**, to check for updates."

**[CONFIRMED]** Bing recommends sitemaps list only canonical URLs, reflect current site structure, remove deleted/redirected URLs promptly, and include freshness signals such as `lastmod`. "Sitemaps do not guarantee visibility, but they significantly improve accuracy and freshness."

**[CONFIRMED]** Bing also uses HTTP validation headers such as **ETags** to help detect content changes more reliably.

**[CONFIRMED]** Bing's blog recommends sitemaps containing all relevant URLs, refreshed once per day, and organized via sitemap indexes with fewer than 50,000 URLs and less than 50 MB per sitemap file.

*Source: Bing Webmaster Tools, "Sitemaps" — https://www.bing.com/webmasters/help/Sitemaps-3b5cf6ed (accessed 2026-11-10); Bing Webmaster Blog, "Keeping Content Discoverable with Sitemaps in AI Powered Search" (July 2025) — https://blogs.bing.com/webmaster/2025/7/Keeping-Content-Discoverable-with-Sitemaps-in-AI-Powered-Search/ (accessed 2026-11-10); "Spring cleaning: Removed Bing anonymous sitemap submission" (May 2022) — https://blogs.bing.com/webmaster/2022/5/Spring-cleaning-Removed-Bing-anonymous-sitemap-submission/ (accessed 2026-11-10); "How to Use the new Sitemap Index Coverage" (September 2023) — https://blogs.bing.com/webmaster/2023/9/How-to-Use-the-new-Sitemap-Index-Coverage-to-Improve-Your-Site-s-Index-Coverage/ (accessed 2026-11-10); "Webmaster Guidelines" (see §2.2).*

### 2.4 robots.txt and meta directives

**[CONFIRMED]** "robots.txt controls crawl access, not indexing." Use `NOINDEX` when a URL should not appear in Bing search, Copilot experiences, or grounding API results.

**[CONFIRMED]** Directive semantics documented by Bing:
- `DATA-NOSNIPPET` and `NOSNIPPET` prevent Bing from displaying captions and may limit Copilot citation quality.
- `NOARCHIVE` prevents content from being used in Copilot responses and grounding results.
- `NOCACHE` limits Copilot to using only the URL, title, and snippet, reducing citation depth and answer quality.

*Source: "Webmaster Guidelines" (see §2.2).*

**[CONFIRMED]** Bing advises against blocking Bingbot in robots.txt and against hiding critical content behind client-side rendering.

*Source: same.*

### 2.5 Canonicalization and duplicate handling

**[CONFIRMED]** "Avoid serving the same content across multiple URLs. Duplicate URLs dilute signals and reduce Bing's confidence in selecting a URL for grounding results or citations. Use canonical URLs, parameter controls, and consistent URL structures to consolidate signals."

**[CONFIRMED]** For URL moves: "Use 301 redirects for permanent URL changes. Use 302 redirects only for very short-term changes (less than 2 days). Use redirects instead of canonical tags."

**[CONFIRMED]** "Canonical tags help signal preference but do not replace fixing underlying issues."

*Source: "Webmaster Guidelines" (see §2.2).*

### 2.6 Ranking signals and freshness

**[CONFIRMED]** Bing lists its main ranking parameters "in general order of importance":

1. **Relevance** — how closely landing-page content matches user intent; includes direct term matching, anchor text of links, and semantic equivalents (synonyms/abbreviations).
2. **Quality and Credibility (QC)** — clarity of purpose, usability, presentation; authority factors: reputation (what types of sites link to it), level of discourse, level of distortion (fact vs. opinion), origination and transparency of ownership.
3. **User engagement** — click-through, time on page, query reformulation.
4. **Freshness** — "Generally, Bing prefers fresh content. A page that consistently provides up-to-date information is considered fresh."
5. **Location and Language** — user location (country/city), page hosting location, page language, location of other visitors.
6. **Page load time** — slow loads considered poor user experience.

**[CONFIRMED]** Bing relies on machine learning for web ranking: "building a model that will look at some ideal results to learn which features are most predictive of their usefulness, relying on automated signals like user interactions with the Bing website, and training data labeled by human judges and/or via AI systems with human oversight."

**[CONFIRMED]** Bing uses personal data (search history, location, language, device characteristics) to improve relevance; users can control this via Bing Settings and the Microsoft Privacy Dashboard.

**[CONFIRMED]** Bing does not allow websites to improve organic ranking through payment. Paid ads are ranked separately and labeled.

*Source: "How Bing delivers search results" (see §2.1).*

### 2.7 JavaScript rendering

**[CONFIRMED]** Bing's Webmaster Guidelines warn against "hiding critical content behind client-side rendering" and "excessive or unnecessary HTTP requests to render the content," stating: "Content that cannot be reliably rendered may not be indexed or selected for grounding results."

*Source: "Webmaster Guidelines" (see §2.2).*

**[INFERRED]** Bing renders JavaScript (the warning implies a rendering step exists), but Bing does not publish a detailed description of its rendering pipeline comparable to Google's. The exact renderer, queueing, and two-pass link extraction are **[UNKNOWN]**.

### 2.8 Structured data

**[CONFIRMED]** "Use structured data using schema.org to your product pages — this includes fields like price, availability, product name, and unique identifiers." (Bing Webmaster Blog.)

**[CONFIRMED]** "Structured data may support clearer grounding but does not guarantee visibility or grounding traffic. Markup must accurately reflect visible content." (Webmaster Guidelines.)

**[CONFIRMED]** "Misleading structured data ... may be ignored and can affect trust and eligibility for enhanced features."

*Source: Bing Webmaster Blog (feed, accessed 2026-11-10); "Webmaster Guidelines" (see §2.2).*

### 2.9 IndexNow participation

**[CONFIRMED]** Bing is a founding participant in **IndexNow** (see §7). "Bing Webmaster Tools supports IndexNow, a protocol that instantly alerts Bing and other IndexNow Search Engines when you add, update, or delete content. This eliminates the need to wait for traditional crawling."

**[CONFIRMED]** Bing recommends IndexNow URL submission, XML sitemaps, crawlable internal links, and external links as discovery signals. "Avoid batch submissions when possible. Streaming submissions provide faster updates, reduce server load, and improve indexing accuracy."

*Source: Bing Webmaster Blog (feed); "Webmaster Guidelines" (see §2.2); IndexNow.org (see §7).*

### 2.10 Bing Shopping and Microsoft Merchant Center

**[CONFIRMED]** "Bing Shopping is a search feature to help users discover products from multiple sellers on a single view by returning relevant shopping results from the web and from advertisers." It may use generative AI (Buying Guides, Expert Review Summary / User Review Summary).

**[CONFIRMED]** Microsoft operates **Microsoft Merchant Center (MMC)**. Product ads are created from products in an MMC store catalog. Setup: create an MMC store; create a product catalog; submit the catalog feed via **FTP/SFTP** or the **Content API**; get the store ID; create a Microsoft Shopping campaign.

**[CONFIRMED]** "A product feed contains a list of product attributes, such as Brand, GTIN, and MPN." The delivery engine "searches the customer's Microsoft Merchant Center store for products that matches the query" and generates a product ad from the product details.

**[CONFIRMED]** Product listings (free section of Bing Shopping tab): merchants with an MMC store with approved offers are automatically opted in. Products must comply with Microsoft Advertising policies and product specification; the `link` attribute must point to the direct product landing page URL.

**[CONFIRMED]** Microsoft Merchant Center can import a **Google Merchant Center** catalog directly ("Google Merchant Center import" is listed as a feed submission method). Microsoft reuses the Google product attribute schema/taxonomy.

**[CONFIRMED]** Feed formats: tab-delimited TXT/TSV (XML only for existing Google-formatted RSS feeds); delivery via scheduled URL fetch (once/24h), FTP/SFTP, manual upload, or Content API. Offers expire after 30 days unless refreshed.

*Source: "How Bing delivers search results" (see §2.1); Microsoft Advertising API, "Product Ads" — https://learn.microsoft.com/en-us/advertising/guides/product-ads (accessed 2026-11-10; last updated 2025-06-26); "About Product listings" — https://learn.microsoft.com/en-us/advertising/msa-help/hlp_ba_conc_bsc_productlistings (accessed 2026-11-10); "Get started with Microsoft Shopping campaigns" — https://learn.microsoft.com/en-us/advertising/msa-help/hlp_ba_conc_bsc_getstarted (accessed 2026-11-10); third-party corroboration of formats/expiry: Pricefy and Feedance integration pages (accessed 2026-11-10) — labelled as third-party, not primary.*

**[CONFIRMED]** Bing Shopping also uses structured data on product pages and IndexNow for faster updates (Bing Webmaster Blog).

**[UNKNOWN]** Whether Bing Shopping crawls merchant websites directly with a dedicated crawler analogous to Google StoreBot. No primary source documents such a crawler for Bing.

### 2.11 Combining first-party index with external providers

**[CONFIRMED]** Bing's web index is first-party (Bingbot). Bing also:
- Uses **IndexNow** to receive URL submissions shared across participating engines.
- Imports sitemaps from Google Search Console (a cross-engine convenience, not a results feed).
- Licenses **Microsoft Start** content for Bing News (labeled "powered by Microsoft Start").
- Uses third-party content providers for local business enhancement (e.g., restaurant review sites) and Bing Places for Business.
- Uses licensed and publicly available location data for Bing Maps.

**[CONFIRMED]** Bing's generative AI features run on Microsoft/OpenAI technologies (GPT, DALL-E).

*Source: "How Bing delivers search results" (see §2.1); "Webmaster Guidelines" (see §2.2); Bing Webmaster Tools "Sitemaps" (see §2.3).*

---

## 3. DuckDuckGo

### 3.1 Result sources

**[CONFIRMED]** DuckDuckGo's own help page states: "We also maintain our own crawler (DuckDuckBot) and many indexes to support our results. Of course, we have more traditional links and images in our search results too, which we **largely source from Bing**."

**[CONFIRMED]** "Most of our search result pages feature one or more Instant Answers. To deliver Instant Answers on specific topics, DuckDuckGo leverages many sources, including specialized sources like Sportradar and crowd-sourced sites like Wikipedia."

**[CONFIRMED]** "When we send a request to a partner for information used in search results, the transfer of information is proxied through our servers so it stays anonymous... no unique identifiers are passed in that process (e.g., your IP address)."

*Source: DuckDuckGo Help Pages, "Where do DuckDuckGo search results come from?" — https://duckduckgo.com/duckduckgo-help-pages/results/sources/ (accessed 2026-11-10).*

**[CONFIRMED]** DuckDuckGo also states: "We use a variety of sources for search results, but not Google."

*Source: DuckDuckGo Help Pages, "What search features does DuckDuckGo Search have?" — https://start.duckduckgo.com/duckduckgo-help-pages/results/what-search-features-does-duckduckgo-search-have (accessed 2026-11-10).*

### 3.2 DuckDuckBot crawler

**[CONFIRMED]** "DuckDuckBot is a web crawler for DuckDuckGo. DuckDuckBot's job is to constantly improve our search results and offer users the best and most secure search experience possible. The crawler's user agent will appear as `DuckDuckBot/1.1; (+http://duckduckgo.com/duckduckbot.html)`."

*Source: DuckDuckGo Help Pages (GitHub mirror of official docs), "DuckDuckBot" — https://github.com/duckduckgo/duckduckgo-help-pages/blob/master/_docs/results/duckduckbot.md (accessed 2026-11-10).*

**[CONFIRMED]** DuckDuckGo also operates **DuckAssistBot** (referenced in third-party reporting; not documented on the primary sources page fetched). **[UNKNOWN]** the exact role of DuckAssistBot from primary sources.

### 3.3 Instant Answers infrastructure

**[CONFIRMED]** DuckDuckGo's Instant Answer infrastructure was open source: the GitHub repository `duckduckgo/duckduckgo` ("DuckDuckGo Instant Answer Infrastructure") was archived by the owner on **October 15, 2022** and is now read-only.

*Source: https://github.com/duckduckgo/duckduckgo (accessed 2026-11-10).*

**[CONFIRMED]** DuckDuckHack, the community program for building Instant Answers, is "now in Maintenance Mode." "Over the past five years, more than 1,500 contributors formed an amazing open source community called DuckDuckHack that, together, produced over 1,200 instant answers for DuckDuckGo." All open source code remains on GitHub.

*Source: https://duckduckgo.github.io/duckduckhack.com/ (accessed 2026-11-10).*

**[CONFIRMED]** The DuckDuckGo Instant Answer API (`https://api.duckduckgo.com`) is a free, key-less GET endpoint returning JSON/JSON-P/XML with zero-click/instant-answer payloads (Abstract, Answer, Definition, RelatedTopics, Results, Redirect for bangs, plus Image/Heading/Type metadata). It does not return web search results.

*Source: Third-party documentation of the API (api-evangelist/duckduckgo GitHub, accessed 2026-11-10) — labelled third-party; the API endpoint itself is official DuckDuckGo infrastructure.*

### 3.4 Sitemaps, robots.txt, canonicalization, JavaScript, change detection

**[UNKNOWN]** DuckDuckGo does not publish primary documentation on its sitemap processing, robots.txt interpretation, canonicalization, JavaScript rendering, or change-detection/recrawling behaviour comparable to Google's or Bing's. Its help pages focus on sources and privacy.

**[INFERRED]** Because DuckDuckGo's web links are largely sourced from Bing, its effective URL discovery and index freshness for general web results are substantially determined by Bing's crawler and index, augmented by DuckDuckBot's own indexes for Instant Answers and specific verticals. This is supported by third-party reporting that a 2024 Bing API outage caused DuckDuckGo to stop returning web results (searchengines.net, accessed 2026-11-10 — third-party, not primary).

### 3.5 Product discovery / shopping

**[UNKNOWN]** DuckDuckGo does not publish a merchant product feed or shopping catalogue system comparable to Google Merchant Center or Microsoft Merchant Center. Its shopping-related results appear to be ad-supported and/or sourced from partners; no primary documentation was found.

---

## 4. Brave Search

### 4.1 Independent index and crawler

**[CONFIRMED]** "Brave Search is built on its own independent search index. It doesn't rely on Big Tech companies like Bing to power its search engine."

**[CONFIRMED]** "Every Web search result seen in Brave Search is now served by our own index. We've removed all search API calls to Bing, which previously represented about 7% of query results." At launch (June 2021), about 13% of queries required third-party help; the "global independence score" rose from 87% to 93% within a year, then to 100%.

**[CONFIRMED]** Brave Search's crawler "does not advertise a differentiated user agent because we must avoid discrimination from websites that allow only Google to crawl them. However, if a domain or page is not crawlable by Googlebot, then Brave Search's bot will not crawl it either."

**[CONFIRMED]** "robots.txt is not used to prevent a page from being indexed. A site owner can delist a page by using the robots noindex directive. As soon as your web page has been updated with the directive, Brave needs to re-fetch it in order to apply the changes — that is, delisting the page." Brave provides a URL submission page for re-fetching/delisting.

**[CONFIRMED]** "Brave Search's crawler is partially powered by information provided by users enrolled in the Web Discovery Project (WDP) option in Brave browser's search settings, which is an off-by-default (aka opt-in), privacy-preserving system. WDP has multiple mechanisms to prevent Brave from knowing who is contributing what (WDP is also open-source for auditing and inspection by anyone)."

*Source: Brave Search Help, "Brave Search Crawler" — https://search.brave.com/help/brave-search-crawler (accessed 2026-11-10); Brave, "Brave Search removes last remnant of Bing..." — https://brave.com/blog/search-independence/ (accessed 2026-11-10); Brave Search Help, "Search results independence" — https://search.brave.com/help/independence (accessed 2026-11-10); Brave Search FAQ — https://support.brave.app/hc/en-us/articles/4403568397837-Brave-Search-FAQ (accessed 2026-11-10); Brave, "What is Brave Search?" — https://brave.com/search/ (accessed 2026-11-10).*

### 4.2 Web Discovery Project (WDP)

**[CONFIRMED]** WDP is a privacy-preserving opt-in system where Brave browser users "contribute some anonymous data about searches and web page visits made within the Brave Browser (including pages arrived at via some, but not all, other search engines). This data helps build the Brave Search independent index." Data types: "search queries, search result clicks, the URLs of pages visited in the browser, time spent on those pages, and some metadata about the pages themselves."

**[CONFIRMED]** WDP's methodology is "heavily inspired by Cliqz's Human Web." The client (extension) code is open source in the `brave/web-discovery-project` GitHub repository. Brave "has a policy to open-source any code delivered to our users."

**[CONFIRMED]** WDP releases include a "patterns" bundle served from Brave's CDN (`https://patterns.wdp.brave.com/patterns.gz`) used "to extract useful information from search results pages on the Internet."

*Source: Brave Support, "What is the Web Discovery Project?" — https://support.brave.app/hc/en-us/articles/4409406835469-What-is-the-Web-Discovery-Project (accessed 2026-11-10); GitHub `brave/web-discovery-project` — https://github.com/brave/web-discovery-project (accessed 2026-11-10) and its `modules/web-discovery-project/sources/README.md` (accessed 2026-11-10); GitHub releases page (accessed 2026-11-10).*

### 4.3 Index scale and API

**[CONFIRMED]** Brave states its index has "over 40 billion pages" and is "the only index at this scale available through an open API." (Vendor claim; not independently verified.)

**[CONFIRMED]** The Brave Search API provides: Web Search (human-consumable ranked results with local/rich enrichments), LLM Context (pre-extracted page content for AI agents), News Search, Image Search, Videos Search, Place Search, and Answers (AI-generated answers backed by web search).

**[CONFIRMED]** "Our service crawls and indexes billions of web pages, ensuring fresh and accurate search results."

**[CONFIRMED]** "No. Every result comes from Brave's own independent index, crawled and ranked by Brave. There is no fallback to another search engine for any query." (Brave Search API page.)

*Source: Brave Search API Documentation — https://api-dashboard.search.brave.com/documentation (accessed 2026-11-10); "Web Search" service page — https://api-dashboard.search.brave.com/documentation/services/web-search (accessed 2026-11-10); Brave, "Brave Search API" — https://brave.com/search/api/ (accessed 2026-11-10); Brave Learn feed — https://brave.com/learn/index.xml (accessed 2026-11-10).*

### 4.4 Goggles (community re-ranking)

**[CONFIRMED]** "Goggles enable any individual — or community of people — to alter the ranking of Brave Search by using a set of instructions (rules and filters). Anyone can create, apply, or extend a Goggle." Goggles act as a re-ranking layer on top of the Brave Search index. The syntax specification and examples are in the `brave/goggles-quickstart` GitHub repository; Goggles can be hosted on GitHub, Gist, or GitLab and submitted via `search.brave.com/goggles/create`.

*Source: Brave Search Help, "Goggles" — https://search.brave.com/help/goggles (accessed 2026-11-10); GitHub `brave/goggles-quickstart` — https://github.com/brave/goggles-quickstart (accessed 2026-11-10).*

### 4.5 Sitemaps, canonicalization, JavaScript, structured data, change detection

**[CONFIRMED]** Brave documents URL submission for re-fetching/delisting (`search.brave.com/submit-url`).

**[CONFIRMED]** Brave's crawler will not crawl pages that Googlebot cannot crawl (a policy choice to avoid discrimination).

**[UNKNOWN]** Brave does not publish primary documentation on its sitemap processing, canonicalization/duplicate handling, JavaScript rendering pipeline, structured data extraction, or change-detection/recrawling algorithms. These are not publicly documented as of November 2026.

**[INFERRED]** Brave's index construction likely combines: first-party crawling, WDP-contributed URL/visit data, and possibly third-party data licensing historically (Bing API until 2023). The exact blend and freshness mechanisms are not disclosed.

### 4.6 Product discovery / shopping

**[UNKNOWN]** Brave Search does not publish a merchant product feed or shopping catalogue system. Its Place Search API covers geographic places/POIs, not product catalogues.

---

## 5. Other independent search engines and product-discovery platforms

### 5.1 Mojeek

**[CONFIRMED]** Mojeek is a UK-based independent search engine that runs its own crawler, index, and ranking algorithm. Its About page documents its index growth: passed 9 billion pages in 2025; 8 billion in 2024; 7 billion in 2023; 6 billion in 2022; 5 billion in 2022; 4 billion in 2021; 3 billion in 2020; 2 billion in 2018; over 1 billion in 2015. "New index starts being crawled and built from scratch" in 2011. Founder Marc Smith "Built Mojeek infrastructure, crawler, billion scale indexing and algorithms."

**[CONFIRMED]** Mojeek adds Openverse as an image search provider (2023) and has launched Search Summary and Semantic Search (2024).

*Source: Mojeek, "About Mojeek" — https://www.mojeek.com/about/ (accessed 2026-11-10); Mojeek Blog (linked from About page).*

**[UNKNOWN]** Mojeek does not publish detailed primary documentation on its crawl scheduling, canonicalization, JavaScript rendering, or structured data extraction. Its technology page (`mojeek.com/about/technology.html`) was not fetched in this research.

### 5.2 Marginalia Search

**[CONFIRMED]** Marginalia is an independent, open-source (AGPL-3) search engine built by Viktor Löfgren in Sweden, focused on text-heavy, non-commercial websites. "The search engine does its own crawling." The software is "custom built in Java" with MariaDB for ancillary metadata; code is open source at `https://git.marginalia.nu/`.

**[CONFIRMED]** Index scale: "the record is about 300 million documents [in 2024]. In terms of disk size, we're talking approximately a terabyte."

**[CONFIRMED]** Crawler identification: the crawler identifies as `search.marginalia.nu` in both the user-agent string and robots.txt token. Crawling runs from a published static IP range (193.183.0.162 to 193.183.0.174), published as a machine-readable text file.

**[CONFIRMED]** Refresh cycle: "a major cycle of eight to ten weeks, with daily RSS polling filling the gap for sites that publish feeds."

**[CONFIRMED]** Architecture (from Marginalia's own documentation): the system uses a link database to determine which websites to visit; a clean system must be seeded with domains manually. It supports batch crawls and recrawls, real-time crawling (using an RSS feed database to identify new documents, indexed in a separate small index partition), and can read WARC files directly and sideload them into the index. Each index node is a separate Docker container. The index is constructed from processed crawl data with snapshot/rollback support.

*Source: Marginalia FAQ — https://www.marginalia.nu/marginalia-search/faq/ (accessed 2026-11-10); Marginalia Search documentation — https://docs.marginalia.nu/ (accessed 2026-11-10); GitHub `MarginaliaSearch` — https://github.com/MarginaliaSearch (accessed 2026-11-10); third-party corroboration: searchengines.net/marginalia (accessed 2026-11-10).*

### 5.3 Ecosia, Qwant, and the Staan index (European Search Perspective)

**[CONFIRMED]** Ecosia and Qwant formed a 50:50 joint venture, **European Search Perspective (EUSP)**, announced November 2024 and headquartered in Paris. Its index is called **Staan** (Search Trusted API Access Network). Staan went live on **6–7 August 2025**, serving real traffic for both engines starting in France.

**[CONFIRMED]** Ecosia's own blog: "We've started delivering search results from our new European-based search index to Ecosia users!... our users in France are receiving a portion of their search results directly from EUSP's own index. We're aiming to serve 30% of French search queries by the end of the year." EUSP's index is "built for alternative search engines and AI companies that need fast, reliable access to the latest web data, while safeguarding user privacy and data security." EUSP is structured to allow outside investment.

**[CONFIRMED]** Ecosia's help center (as reported by third-party searchengines.net, August 2026) names three result providers: Microsoft Bing, Google, and EUSP.

**[CONFIRMED]** Qwant operates its own crawler, **Qwantbot**, and its own index. Third-party reporting (searchengines.net) states Qwant's own index held around 20 billion pages as of September 2023, crawled at more than a billion pages a day — **self-reported figures with no independent verification**. Qwant supplements with Bing where its own relevance is weak and for images, and since August 2025 also uses Staan.

**[CONFIRMED]** Qwant became the default search engine of the European Parliament from 4 June 2026 (third-party reporting).

*Source: Ecosia blog, "The internet just got better: our European search index goes live" — https://blog.ecosia.org/launching-our-european-search-index (accessed 2026-11-10); TechCrunch, "Qwant and Ecosia debut Staan..." (6 August 2025) — https://techcrunch.com/2025/08/06/qwant-and-ecosia-debut-staan-a-european-search-index-that-aims-to-take-on-big-tech/ (accessed 2026-11-10); Wikipedia, "Qwant" — https://en.wikipedia.org/wiki/Qwant (accessed 2026-11-10); third-party: searchengines.net (accessed 2026-11-10).*

**[UNKNOWN]** Staan's internal crawl scheduling, index architecture, and whether it operates its own crawler versus aggregating partner data. EUSP's own website/documentation was not fetched in this research.

### 5.4 Startpage, Swisscows, and other resellers

**[CONFIRMED]** Startpage has no crawler and no index; it relays queries anonymously to Google and Bing and returns their results (third-party reporting, searchengines.net, accessed 2026-11-10). Startpage was Google-only from 2009 until March 2023, when a Microsoft partnership added Bing results.

**[CONFIRMED]** Swisscows states an "exclusive partnership with Bing" on its own blog (third-party reporting).

**[CONFIRMED]** Ecosia blends Bing, Google, and EUSP/Staan.

*Source: Third-party: searchengines.net guides (accessed 2026-11-10). These are not primary vendor documentation; the underlying claims are corroborated by Wikipedia and company announcements where cited.*

### 5.5 Common Crawl (open web corpus)

**[CONFIRMED]** Common Crawl is a non-profit that "crawls the web every month and freely publishes its archives." The corpus contains "raw web page data, metadata extracts, and text extracts," stored on AWS Public Data Sets and academic cloud platforms. Access is free.

**[CONFIRMED]** Each crawl ships a columnar URL index listing every captured page with host, fetch status, content type, detected language, and a pointer into the WARC archive holding the raw response. The URL index is split into 300 parts per crawl. Example: CC-MAIN-2026-25 contains 2,098,491,742 URLs (161.7 GB Parquet).

**[CONFIRMED]** Common Crawl data is available as WARC (raw), WET (extracted text), and WAT (metadata) files, and via a columnar URL index on S3/HTTPS.

*Source: Common Crawl, "Overview" — https://commoncrawl.org/overview (accessed 2026-11-10); Hugging Face dataset card `open-index/ccrawl-urls` — https://huggingface.co/datasets/open-index/ccrawl-urls (accessed 2026-11-10); NVIDIA NeMo Curator Common Crawl docs — https://docs.nvidia.com/nemo/curator/curate-text/load-data/common-crawl (accessed 2026-11-10).*

**[INFERRED]** Common Crawl is a key external data source for independent search engines, AI training, and academic research. It is not a consumer search engine itself.

### 5.6 Yandex, Naver, Seznam, Yep (IndexNow participants)

**[CONFIRMED]** IndexNow participating endpoints include Yandex (`https://yandex.com/indexnow`), Naver (`https://searchadvisor.naver.com/indexnow`), Seznam.cz (`https://search.seznam.cz/indexnow`), and Yep (`https://indexnow.yep.com/indexnow`).

*Source: IndexNow.org FAQ — https://www.indexnow.org/faq (accessed 2026-11-10).*

**[UNKNOWN]** Detailed crawling/indexing internals for these engines are outside the scope of this report and were not researched from primary sources.

---

## 6. Google Merchant Center and Bing/Microsoft Merchant Center: comparative feed ingestion

| Dimension | Google Merchant Center | Microsoft Merchant Center |
|---|---|---|
| **Primary feed input** | File upload (.txt/.xml/.tsv ≤4 GB), Google Sheets, scheduled fetch (default 24h), SFTP, GCS, Merchant API | File upload, FTP/SFTP, scheduled fetch, Content API |
| **API** | Merchant API (Content API for Shopping sunset 2026-08-18) | Content API (`content.api.bingads.microsoft.com/shopping/v9.1/bmc/`) |
| **Website crawl** | "Website crawl" / automated data source / "Found by Google" — scans site, reads structured data, submits to MC | Not documented as a comparable feature |
| **Dedicated shopping crawler** | Google StoreBot (`Storebot-Google`) — product, cart, checkout pages; ML form-filling | Not documented |
| **Structured data** | schema.org Product/Offer (JSON-LD recommended); must be in server-returned HTML | schema.org recommended (price, availability, name, identifiers) |
| **Price/availability updates** | Automatic item updates (default on) using structured data + ML "advanced data extractors" | Not documented as an equivalent feature; merchants resubmit feeds |
| **Local inventory** | StoreBot deduces in-store availability; creates inventory data in MC | Local product inventory feeds documented |
| **Cross-engine import** | — | Can import Google Merchant Center catalog directly |
| **Data quality** | Product data specification; item-level disapprovals; 72h data-quality checks | Product specification; policy compliance; approval review |

*Sources: Google Merchant Center Help pages and Google Merchant API docs (see §1.11–1.12); Microsoft Advertising API docs (see §2.10).*

---

## 7. IndexNow protocol (cross-engine URL submission)

**[CONFIRMED]** IndexNow is an open protocol for telling participating search engines that a URL has been added, updated, or deleted. A single submission is shared across all participating engines.

**[CONFIRMED]** Mechanism:
1. Site owner generates an API key (8–128 chars; a-z, A-Z, 0-9, hyphens) and hosts it as a UTF-8 text file at `https://{host}/{key}.txt` (or another public location specified via `keyLocation`).
2. On content change, the site sends a GET (`?url=...&key=...`) or a POST with JSON `{host, key, keyLocation, urlList}` (up to 10,000 URLs per POST).
3. The receiving engine verifies the key file to confirm domain ownership.
4. Participating engines share received URLs with each other within 10 seconds of verification, using signed payloads (`X-IN-Notifier-Public-Key`, `X-Signed-Payload-Digest`) and a `/indexnow?noreping` endpoint to prevent re-propagation loops.
5. Each engine polls the partners list (`https://www.indexnow.org/searchengines.json`) at least every 24 hours and hosts a `meta.json` with public keys.
6. Partners retain logs of notified/verified URLs for at least one week.

**[CONFIRMED]** Participating endpoints: IndexNow global (`api.indexnow.org`), Amazon (`indexnow.amazonbot.amazon`), Bing (`www.bing.com/indexnow`), Naver, Seznam.cz, Yandex, Yep.

**[CONFIRMED]** "Submitting a URL does not guarantee immediate indexing. However, IndexNow increases the likelihood that important changes are discovered and crawled faster." "Every URL submitted through IndexNow counts toward your site's crawl quota."

**[CONFIRMED]** Google does **not** participate in IndexNow. (IndexNow FAQ lists participating endpoints; Google is absent. Third-party sources confirm Google does not participate.)

**[CONFIRMED]** CMS support includes WordPress, Shopify, Wix, GoDaddy, Duda, XenForo, Joomla, Drupal, PrestaShop, Shopware, OpenCart, Typo3, Umbraco, Bitrix24, MODX, Milestone. Cloudflare offers native IndexNow integration.

*Source: IndexNow.org, "Documentation for search engines" — https://www.indexnow.org/searchengines (accessed 2026-11-10); IndexNow.org FAQ — https://www.indexnow.org/faq (accessed 2026-11-10); Wikipedia, "IndexNow" — https://en.wikipedia.org/wiki/IndexNow (accessed 2026-11-10).*

---

## 8. Cross-cutting synthesis: patterns and divergences

### 8.1 URL discovery
- **Link graph is primary** for Google and Bing; sitemaps are a supplementary hint.
- **Sitemaps** are universally supported (XML/RSS/Atom/text) and fetched at least daily by Bing; Google reads them regularly.
- **Push protocols** (IndexNow) are adopted by Bing and other engines but **not by Google**.
- **Manual submission** exists in Search Console (Google) and Bing Webmaster Tools.

### 8.2 Crawl scheduling
- **Google** formalizes crawl budget = capacity limit × demand, with documented health-based adjustment.
- **Bing** formalizes crawl efficiency (new/fresh content per page crawled) and prioritizes unindexed known URLs and suspected updates.
- **Brave** and **Mojeek** do not publish scheduling internals.
- **Marginalia** documents an 8–10 week major cycle plus daily RSS polling and real-time crawling.

### 8.3 Canonicalization and duplicates
- **Google** has the most detailed documented model: centerpiece detection, clustering, canonical selection, signal strength ordering (redirect > rel=canonical > sitemap), and a named Deduplication ranking system.
- **Bing** documents canonical/redirect/parameter guidance but not an internal clustering model.
- **Brave** and **DuckDuckGo** do not publish canonicalization internals.

### 8.4 JavaScript rendering
- **Google** documents a explicit crawl → render → index pipeline with two-pass link extraction and evergreen Chromium.
- **Bing** acknowledges rendering and warns against client-side-only content, but does not document its pipeline.
- **Brave**, **DuckDuckGo**, **Mojeek** do not publish rendering documentation.

### 8.5 Structured data
- **Google** has the richest documented schema.org surface (Product snippets, Merchant listings, full search gallery) and combines on-page markup with Merchant Center feeds.
- **Bing** recommends schema.org for products and evaluates it for grounding/citations.
- **Brave** exposes rich results via its API (products, reviews, recipes, software, organizations) but does not publish extraction documentation.
- **DuckDuckGo** uses structured sources for Instant Answers (Wikipedia, Sportradar, etc.) via open-source plugins.

### 8.6 Product feeds and merchant catalogues
- **Google** has the most layered system: file/API/scheduled-fetch ingestion, website crawl, StoreBot deep crawling (cart/checkout), automatic item updates with ML extractors, and local inventory deduction.
- **Microsoft** has a conventional merchant centre (file/FTP/API) plus Google Merchant Centre import, but no documented equivalent of StoreBot or automatic item updates.
- **Bing Shopping** combines web results with advertiser catalogue results and generative AI features.
- **DuckDuckGo** and **Brave** do not publish merchant catalogue systems.

### 8.7 Freshness and ranking
- **Google** names Freshness systems and hundreds of factors; page-level plus site-wide signals.
- **Bing** lists six ranked parameters (relevance, QC, engagement, freshness, location/language, load time) and uses ML with human/AI-labeled training data.
- **Brave** emphasizes independent ranking plus community Goggles re-ranking.
- **Mojeek** and **Marginalia** use their own algorithms; Marginalia explicitly ranks "backwards" (favouring text-heavy, ad-light pages).

### 8.8 First-party vs. external data
- **Google** and **Bing** are fully first-party for web results; both augment with merchant feeds, structured data, and (Bing) licensed content.
- **DuckDuckGo** is a hybrid: Bing supplies web links/images; DuckDuckBot supplies Instant Answer indexes.
- **Brave** is first-party for web results since 2023 (Bing API removed), augmented by WDP-contributed data.
- **Mojeek** and **Marginalia** are fully first-party.
- **Ecosia/Qwant** are hybrids blending Bing/Google with the co-owned Staan index.
- **Startpage** is a pure reseller (Google + Bing).
- **Common Crawl** is an open external corpus used by researchers and as crawl-frontier seeds.

---

## 9. Key unknowns and gaps (as of November 2026)

1. **Google's current indexing infrastructure** — whether "Caffeine" remains the name/architecture; no primary update since 2010.
2. **Google's change-detection algorithm** — staleness/recrawl logic is described at a high level but not published.
3. **Bing's rendering pipeline** — acknowledged but not documented.
4. **Brave's crawl scheduling, canonicalization, rendering, and structured data extraction** — not publicly documented.
5. **DuckDuckGo's sitemap/robots/canonicalization/JavaScript/change-detection behaviour** — not publicly documented.
6. **Mojeek's and Marginalia's canonicalization and rendering internals** — not publicly documented (Marginalia's open-source code is the closest primary artifact).
7. **Staan/EUSP's internal architecture** — not publicly documented.
8. **Whether Bing Shopping operates a dedicated shopping crawler** analogous to StoreBot — no primary source found.
9. **DuckDuckGo's and Brave's merchant/shopping catalogue systems** — not published.
10. **Exact blend of first-party vs. third-party data in Brave's and DuckDuckGo's results** — not disclosed.

---

## 10. Source index (all accessed 2026-11-10)

### Google
- How Google Search works: https://developers.google.com/search/docs/fundamentals/how-search-works
- Crawling and indexing overview: https://developers.google.com/search/docs/crawling-indexing
- Google's common crawlers: https://developers.google.com/crawling/docs/crawlers-fetchers/google-common-crawlers
- Crawl budget: https://developers.google.com/crawling/docs/crawl-budget
- Sitemaps overview: https://developers.google.com/search/docs/crawling-indexing/sitemaps/overview
- Build and submit a sitemap: https://developers.google.com/search/docs/crawling-indexing/sitemaps/build-sitemap
- robots.txt intro: https://developers.google.com/search/docs/crawling-indexing/robots/intro
- robots.txt specification interpretation: https://developers.google.com/crawling/docs/robots-txt/robots-txt-spec
- Canonicalization: https://developers.google.com/search/docs/crawling-indexing/canonicalization
- Specify a canonical: https://developers.google.com/search/docs/crawling-indexing/consolidate-duplicate-urls
- JavaScript SEO basics: https://developers.google.com/search/docs/crawling-indexing/javascript/javascript-seo-basics
- Product structured data: https://developers.google.com/search/docs/appearance/structured-data/product
- Merchant listing structured data: https://developers.google.com/search/docs/appearance/structured-data/merchant-listing
- Structured data gallery: https://developers.google.com/search/docs/appearance/structured-data/search-gallery
- Ranking systems guide: https://developers.google.com/search/docs/appearance/ranking-systems-guide
- Page experience: https://developers.google.com/search/docs/appearance/page-experience
- Creating helpful content: https://developers.google.com/search/docs/fundamentals/creating-helpful-content
- Technical requirements: https://developers.google.com/search/docs/essentials/technical
- Troubleshoot crawling errors: https://developers.google.com/search/docs/crawling-indexing/troubleshoot-crawling-errors
- Ask Google to recrawl: https://developers.google.com/search/docs/crawling-indexing/ask-google-to-recrawl
- SEO Starter Guide: https://support.google.com/webmasters/answer/7451184
- SEO Guide for Web Developers: https://developers.google.com/search/docs/fundamentals/get-started-developers
- Caffeine announcement (2010): https://developers.google.com/search/blog/2010/06/our-new-search-index-caffeine
- Evergreen Googlebot (2019): https://developers.google.com/search/blog/2019/05/the-new-evergreen-googlebot
- Merchant Center — data sources: https://support.google.com/merchants/answer/15624855
- Merchant Center — upload a feed: https://support.google.com/merchants/answer/188477
- Merchant Center — add products from a file: https://support.google.com/merchants/answer/12158380
- Merchant Center — add products automatically: https://support.google.com/merchants/answer/12158480
- Merchant Center — automated feeds: https://support.google.com/merchants/answer/7538732
- Merchant Center — product data specification: https://support.google.com/merchants/answer/7052112
- Merchant Center — StoreBot: https://support.google.com/merchants/answer/13294660
- Merchant Center — in-store inventory: https://support.google.com/merchants/answer/14871373
- Merchant Center — automatic item updates: https://support.google.com/merchants/answer/3246284
- Merchant Center — automations: https://support.google.com/merchants/answer/12157888
- Merchant Center — structured data attributes: https://support.google.com/merchants/answer/6386198
- Merchant Center — set up structured data: https://support.google.com/merchants/answer/7331077
- Merchant Center — tips to stay approved: https://support.google.com/merchants/answer/14999713
- Merchant API — manage products: https://developers.google.com/merchant/api/guides/products/overview
- Shopping Content — automatic improvements: https://developers.google.com/shopping-content/guides/automatic-improvements
- Shopping Automation — create a feed: https://developers.google.com/google-ads/shopping/feed-management/articles/t6
- Share product data with Google: https://developers.google.com/search/docs/specialty/ecommerce/share-your-product-data-with-google
- Google crawling infrastructure: https://developers.google.com/crawling

### Microsoft Bing
- How Bing delivers search results: https://support.microsoft.com/en-us/bing/how-bing-delivers-search-results
- Webmaster Guidelines: https://www.bing.com/webmasters/help/webmaster-guidelines-30fba23a
- Sitemaps: https://www.bing.com/webmasters/help/Sitemaps-3b5cf6ed
- bingbot crawl efficiency (2018): https://blogs.bing.com/webmaster/2018/10/bingbot-Series-Maximizing-Crawl-Efficiency/
- Sitemaps in AI powered search (2025): https://blogs.bing.com/webmaster/2025/7/Keeping-Content-Discoverable-with-Sitemaps-in-AI-Powered-Search/
- Removed anonymous sitemap submission (2022): https://blogs.bing.com/webmaster/2022/5/Spring-cleaning-Removed-Bing-anonymous-sitemap-submission/
- Sitemap Index Coverage (2023): https://blogs.bing.com/webmaster/2023/9/How-to-Use-the-new-Sitemap-Index-Coverage-to-Improve-Your-Site-s-Index-Coverage/
- Product Ads: https://learn.microsoft.com/en-us/advertising/guides/product-ads
- About Product listings: https://learn.microsoft.com/en-us/advertising/msa-help/hlp_ba_conc_bsc_productlistings
- Get started with Microsoft Shopping campaigns: https://learn.microsoft.com/en-us/advertising/msa-help/hlp_ba_conc_bsc_getstarted

### DuckDuckGo
- Where do results come from: https://duckduckgo.com/duckduckgo-help-pages/results/sources/
- Search features: https://start.duckduckgo.com/duckduckgo-help-pages/results/what-search-features-does-duckduckgo-search-have
- DuckDuckBot: https://github.com/duckduckgo/duckduckgo-help-pages/blob/master/_docs/results/duckduckbot.md
- Instant Answer infrastructure (archived): https://github.com/duckduckgo/duckduckgo
- DuckDuckHack maintenance mode: https://duckduckgo.github.io/duckduckhack.com/

### Brave
- Brave Search Crawler: https://search.brave.com/help/brave-search-crawler
- Search results independence: https://search.brave.com/help/independence
- Independence announcement: https://brave.com/blog/search-independence/
- Brave Search FAQ: https://support.brave.app/hc/en-us/articles/4403568397837-Brave-Search-FAQ
- What is Brave Search: https://brave.com/search/
- Brave Search API: https://brave.com/search/api/
- API documentation: https://api-dashboard.search.brave.com/documentation
- Web Search service: https://api-dashboard.search.brave.com/documentation/services/web-search
- Goggles: https://search.brave.com/help/goggles
- Web Discovery Project: https://support.brave.app/hc/en-us/articles/4409406835469-What-is-the-Web-Discovery-Project
- WDP GitHub: https://github.com/brave/web-discovery-project
- WDP sources README: https://github.com/brave/web-discovery-project/blob/main/modules/web-discovery-project/sources/README.md
- Goggles quickstart: https://github.com/brave/goggles-quickstart

### Other engines
- Mojeek About: https://www.mojeek.com/about/
- Marginalia FAQ: https://www.marginalia.nu/marginalia-search/faq/
- Marginalia docs: https://docs.marginalia.nu/
- Marginalia GitHub: https://github.com/MarginaliaSearch
- Ecosia blog — European index launch: https://blog.ecosia.org/launching-our-european-search-index
- TechCrunch — Staan launch: https://techcrunch.com/2025/08/06/qwant-and-ecosia-debut-staan-a-european-search-index-that-aims-to-take-on-big-tech/
- Qwant (Wikipedia): https://en.wikipedia.org/wiki/Qwant
- Common Crawl overview: https://commoncrawl.org/overview
- Common Crawl URL index (HF): https://huggingface.co/datasets/open-index/ccrawl-urls
- NVIDIA NeMo Curator — Common Crawl: https://docs.nvidia.com/nemo/curator/curate-text/load-data/common-crawl

### Protocols and standards
- IndexNow for search engines: https://www.indexnow.org/searchengines
- IndexNow FAQ: https://www.indexnow.org/faq
- IndexNow (Wikipedia): https://en.wikipedia.org/wiki/IndexNow
- Sitemaps protocol: https://www.sitemaps.org/protocol.html
- RFC 3986 (URI syntax): https://datatracker.ietf.org/doc/html/rfc3986

### Third-party corroboration (labelled as such in text)
- searchengines.net guides (own-index vs resold results; market share Europe; Marginalia; Qwant; Ecosia; Startpage; Swisscows): https://www.searchengines.net/
- Moz beginners guide (Caffeine reference): https://moz.com/beginners-guide-to-seo/how-search-engines-operate
- Pricefy Microsoft Bing feed: https://www.pricefy.io/feed-channels/microsoft-bing
- Feedance Bing Ads product feed: https://www.feedance.com/integration/bing-ads

---

*Report prepared for IntentForge. No repository files were modified. All source URLs accessed 10 November 2026.*
