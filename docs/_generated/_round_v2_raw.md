=== round v2 exercise start 2026-08-05T16:55:46Z ===

### ROOT /
REQ: GET /
HTTP 200  time 0.003754s
```json
IntentForge-v2 Gateway
```

### HEALTH /health
REQ: GET /health
HTTP 200  time 0.003228s
```json
OK
```

### SEARCH informational: what causes aurora borealis
REQ: GET /search?q=what%20causes%20aurora%20borealis
HTTP 200  time 0.003885s
```json
{
  "category": "informational",
  "confidence": 0.26803634,
  "constraints": [
    "+aurora",
    "+borealis",
    "+causes",
    "+what"
  ],
  "distribution": {
    "comparison": 0.04102446,
    "fresh": 0.029323468,
    "how-to": 0.17906247,
    "informational": 0.22109756,
    "local": 0.12942038,
    "navigational": 0.2391339,
    "technical": 0.093663946,
    "transactional": 0.06727383
  },
  "expanded_queries": [
    "what causes aurora borealis",
    "what causes aurora borealis explained",
    "what causes aurora borealis overview",
    "what is what causes aurora borealis",
    "what causes aurora borealis for beginners",
    "what causes aurora borealis examples",
    "learn what causes aurora borealis",
    "what causes aurora borealis best resources"
  ],
  "has_more": false,
  "intent": "informational",
  "limit": 24,
  "offset": 0,
  "query": "what causes aurora borealis",
  "results": [
    {
      "authority": 1.0,
      "content": "“We sometimes see a wonderful scarlet red colour, and this is caused by very high altitude oxygen interacting with solar particles,” adds astronomer Tom. “This only occurs when the aurora is particularly energetic.” · The aurora borealis can be seen in the northern hemisphere, while the aurora australis is found in the southern hemisphere.",
      "is_local": false,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "brave"
      ],
      "title": "What causes the Northern Lights? Aurora borealis explained | Royal Museums Greenwich",
      "url": "https://www.rmg.co.uk/stories/space-astronomy/what-causes-northern-lights-aurora-borealis-explained"
    },
    {
      "authority": 1.0,
      "content": "To do this, the energy is released from the molecules as a photon of light. When millions of photons of light are emitted at the same time, it causes the sky to light up and creates the Aurora Borealis.",
      "is_local": false,
      "published_date": "2022-07-13T00:00:00",
      "quality": 1.0,
      "score": 0.9108486,
      "sources": [
        "brave"
      ],
      "title": "The Aurora Borealis | What Causes the Northern Lights? – Triple F.A.T. Goose",
      "url": "https://triplefatgoose.com/blogs/down-time/understanding-the-aurora-borealis"
    },
    {
      "authority": 1.0,
      "content": "The northern lights are more formally known as the aurora borealis because borealis means north in Latin. The southern lights, meanwhile, are called the aurora australis. Auroras don’t just happen on Earth. Scientists have also seen them on other planets, including Mars, Jupiter and Saturn, as well as on comets and even dwarf stars far beyond our solar system. The northern lights are caused by the Sun’s powerful magnetic field.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.86082,
      "sources": [
        "brave"
      ],
      "title": "What are the northern lights? The aurora borealis explained | Natural History Museum",
      "url": "https://www.nhm.ac.uk/discover/what-are-the-northern-lights-aurora-borealis-causes-explained.html"
    },
    {
      "authority": 0.5,
      "content": "Galileo Galilei who coined the name \"aurora borealis\" in 1619 — after the Roman goddess of dawn, Aurora, and the Greek god of the north wind, Boreas — the earliest suspected record of the northern lights is in a 30,000-year-old cave painting in France.",
      "is_local": false,
      "published_date": "2025-05-06T00:00:00",
      "quality": 1.0,
      "score": 0.8407269,
      "sources": [
        "brave"
      ],
      "title": "Aurora Borealis: What Causes the Northern Lights & Where to See Them",
      "url": "https://www.space.com/15139-northern-lights-auroras-earth-facts-sdcmp.html"
    },
    {
      "authority": 0.90000004,
      "content": "An aurora (pl. aurorae or auroras) ... caused by charged particles from the Sun colliding with atoms in the atmosphere. These collisions excite oxygen and nitrogen, which then emit light of different colors such as green, red, and purple. When observed in high-latitude regions they are called polar lights and aurora polaris. In the Arctic they are called the northern lights or aurora borealis; in the Antarctic, ...",
      "is_local": false,
      "quality": 1.0,
      "score": 0.57097197,
      "sources": [
        "brave"
      ],
      "title": "Aurora - Wikipedia",
      "url": "https://en.wikipedia.org/wiki/Aurora"
    },
    {
      "authority": 0.6,
      "content": "The aurora veteran, with over 15 years of seeing the aurora aboard Hurtigruten, explains “The aurora borealis is caused by electrically charged particles that are released by the Sun and travel 150 million kilometers [92 million miles] across space to the Earth.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.5490896,
      "sources": [
        "brave"
      ],
      "title": "What are the Northern Lights? | Science Behind the Aurora Borealis | Hurtigruten US",
      "url": "https://www.hurtigruten.com/en-us/explore-norway/northern-lights/science"
    },
    {
      "authority": 0.90000004,
      "content": "The Northern Lights, known also as aurora borealis are a natural display of light in the northern hemisphere's night sky. Auroral displays appear in many hues—though pale green and pink are most common. Shades of red, yellow, green, blue, and violet are also reported.",
      "is_local": false,
      "published_date": "2022-03-17T00:00:00",
      "quality": 1.0,
      "score": 0.060125843,
      "sources": [
        "brave"
      ],
      "title": "What are the Northern Lights (Aurora Borealis)?",
      "url": "https://www.mtu.edu/tour/copper-country/northern-lights/"
    },
    {
      "authority": 0.90000004,
      "content": "Explore This Section … Climate Change Causes Earth Explore Explore Earth Science Agriculture Air Quality Climate Change Freshwater Life on Earth Severe Storms Snow and Ice The Global Ocean Climate Change Facts Evidence Causes Effects Scientific Consensus What i
```

### SEARCH comparison: react vs vue vs svelte
REQ: GET /search?q=react%20vs%20vue%20vs%20svelte
HTTP 200  time 0.003707s
```json
{
  "category": "informational",
  "confidence": 0.9,
  "constraints": [
    "+react",
    "+svelte",
    "+vue"
  ],
  "distribution": {
    "comparison": 0.49205467,
    "fresh": 0.01854829,
    "how-to": 0.10074932,
    "informational": 0.058690105,
    "local": 0.020728232,
    "navigational": 0.21380807,
    "technical": 0.036336116,
    "transactional": 0.059085164
  },
  "expanded_queries": [
    "react vs vue vs svelte",
    "react vue svelte documentation",
    "react vue svelte examples",
    "react vue svelte programming"
  ],
  "has_more": false,
  "intent": "comparison",
  "limit": 24,
  "offset": 0,
  "query": "react vs vue vs svelte",
  "results": [
    {
      "authority": 0.70000005,
      "content": "document.cookie = 'referrer=' + document.referrer; Back Services Services Services Step into the world where your vision for software excellence becomes a reality. We don't just provide custom software development services, we offer a partnership with a full spectrum of IT solutions for startups and SMBs.",
      "is_local": true,
      "quality": 0.9001079,
      "score": 1.0,
      "sources": [
        "local"
      ],
      "title": "SVAR UI – React, Svelte & Vue Components",
      "url": "https://xbsoftware.com/products/svar-ui/"
    },
    {
      "authority": 0.70000005,
      "content": "html body Services Services Services Technologies Technologies Technologies Industries Industries Industries Projects Projects Projects About Us About Us About Us Insight Insight Insight Testimonials Testimonials Testimonials Contact Us Contact Us Contact Us Menu Menu Men",
      "is_local": true,
      "quality": 0.6831879,
      "score": 0.07083881,
      "sources": [
        "local"
      ],
      "title": "React vs Vue vs Angular: The Honest Comparison (2026) | Hashbyt",
      "url": "https://hashbyt.com/blog/react-vs-vue-vs-angular"
    },
    {
      "authority": 0.6,
      "content": "Back to Blog Web Development React vs Vue in 2026: Which One Should You Choose for Your Next Project? React and Vue have both matured significantly in 2026, and the choice between them is more nuanced than ever. A senior developer's honest take on learning curve, ecosystem, performance, hiring, and everything else that actually matters when choosing a frontend framework. Vaidehi Sharma April 07, 2026 16 min read 56 views I've been building web applications for over a decade.",
      "is_local": true,
      "quality": 0.68823195,
      "score": 0.07025777,
      "sources": [
        "local"
      ],
      "title": "React vs Vue in 2026: Which Framework to Choose? | LuminaryEra",
      "url": "https://www.luminaryera.com/blog/react-vs-vue-2026-which-framework-to-choose"
    },
    {
      "authority": 0.70000005,
      "content": "React vs Vue 2026: The Definitive JavaScript Framework Comparison Marcus Chen March 22, 2026 Software Marcus Chen March 22, 2026 25 min read The JavaScript framework wars are far from over. As we enter Q2 2026, the React vs Vue debate continues to dominate developer forums, Slack channels, and technical hiring conversations across the industry. With React 19.2 introducing a production-ready compiler and Vue 3.",
      "is_local": true,
      "quality": 0.9495226,
      "score": 0.06676996,
      "sources": [
        "local"
      ],
      "title": "React vs Vue: 7 Benchmarks Show a Clear Winner [2026]",
      "url": "https://tech-insider.org/react-vs-vue-2026/"
    },
    {
      "authority": 0.6,
      "content": "Stanza Roadmaps Integrations Resources Sign in Toggle menu",
      "is_local": true,
      "quality": 0.6789219,
      "score": 0.06502761,
      "sources": [
        "local"
      ],
      "title": "React vs Vue (2026) — Comparison with Code | Stanza",
      "url": "https://www.stanza.dev/compare/react-vs-vue"
    },
    {
      "authority": 0.6,
      "content": "Home › Comparisons › React vs Vue React vs Vue in 2026 A practical comparison of the two most popular frontend frameworks for modern web development. (()=> )(); Feature Comparison Feature React Vue GitHub Stars 235K+ 215K+ Bundle size (min) ~6.4KB ~16KB Learning curve Moderate Easy TypeScript Excellent Excellent SSR Next.",
      "is_local": true,
      "quality": 0.6744371,
      "score": 0.06468206,
      "sources": [
        "local"
      ],
      "title": "React vs Vue in 2026 — Frontend Framework Comparison",
      "url": "https://www.kunalganglani.com/comparisons/react-vs-vue"
    },
    {
      "authority": 0.75000006,
      "content": "Skip to main content ((a,b,c,d,e,f,g,h)=>{let i=document.documentElement,j=[\"light\",\"dark\"];function k(b) if(d)k(d);else try{let a=localStorage.",
      "is_local": true,
      "quality": 0.72867644,
      "score": 0.058127347,
      "sources": [
        "local"
      ],
      "title": "Svelte vs React (2026): Which Is Better? | ZTABS",
      "url": "https://ztabs.co/compare/svelte-vs-react"
    },
    {
      "authority": 0.70000005,
      "content": "Rate Sofia Lindström April 19, 2026 Software Sofia Lindström April 19, 2026 20 min read Vue.js and React remain the two most debated front-end frameworks heading into 2026. React dominates with 25 million weekly npm downloads against Vue’s 5 million, but Vue’s developer satisfaction scores have climbed to 93% in recent surveys. With React 19 introducing a new compiler and Vue 3.",
      "is_local": true,
      "quality": 0.90463156,
      "score": 0.05,
      "sources": [
        "local"
      ],
      "title": "Vue vs React 2026: 5x Download Gap and 93% Retention [Tested]",
      "url": "https://tech-insider.org/vue-vs-react-2026/"
    }
  ],
  "results_after_filter": 8,
  "results_before_filter": 8,
  "structured_constraints": {
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [
      "react",
      "svelte",
      "vue"
    ],
    "related": [],
    "sites": []
  },
  "total": 8
}
```

### SEARCH transactional: buy mechanical keyboard
REQ: GET /search?q=buy%20mechanical%20keyboard
HTTP 200  time 0.003954s
```json
{
  "category": "transactional",
  "confidence": 0.8,
  "constraints": [
    "+buy",
    "+keyboard",
    "+mechanical"
  ],
  "deep_result": {
    "confidence": 0.88,
    "page_title": "Computer Keyboards - Wireless, Bluetooth, Mechanical | Logitech",
    "page_url": "https://www.logitech.com/en-us/shop/c/keyboards",
    "result_type": "official_page",
    "vendor_name": "www.logitech.com"
  },
  "distribution": {
    "comparison": 0.048112903,
    "fresh": 0.03518041,
    "how-to": 0.1181614,
    "informational": 0.08520839,
    "local": 0.04779766,
    "navigational": 0.18589462,
    "technical": 0.035113994,
    "transactional": 0.4445306
  },
  "expanded_queries": [
    "buy mechanical keyboard",
    "buy mechanical keyboard explained",
    "buy mechanical keyboard overview",
    "what is buy mechanical keyboard",
    "buy mechanical keyboard for beginners",
    "buy mechanical keyboard examples",
    "learn buy mechanical keyboard",
    "buy mechanical keyboard best resources"
  ],
  "has_more": false,
  "intent": "transactional",
  "limit": 24,
  "offset": 0,
  "query": "buy mechanical keyboard",
  "results": [
    {
      "authority": 0.6,
      "content": "Every product was carefully curated by an Esquire editor. We may earn a commission from these links. Here’s how we test products and why you should trust us. Keychron Lifestyle Tech The 5 Best Mechanical Keyboards You Can Buy on Amazon Follow Tech View feed The 5 Best Mechanical Keyboards You Can Buy on Amazon Click. Clack. By Bryn Gelbart Published: Sep 22, 2025 Save Article Share Article Buying a new keyboard online can be a nightmare.",
      "is_local": true,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "local"
      ],
      "title": "The 5 Best Mechanical Keyboards to Buy in 2025",
      "url": "https://www.esquire.com/lifestyle/tech/g67964205/best-mechanical-keyboards/"
    },
    {
      "authority": 0.5,
      "content": "Shop Best Buy for electronics, computers, appliances, cell phones, video games & more new tech.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.07021615,
      "sources": [
        "bing"
      ],
      "title": "Best Buy | Official Online Store | Shop Now & Save",
      "url": "https://www.bestbuy.com/"
    },
    {
      "authority": 0.6,
      "content": "$100 Custom Keyboard Overview Product sourcing insights & recommendations R A M M 24 来源 Your browser does not support the video tag. MP4 · HD The $100 custom keyboard market in 2026 is defined by \"premium features at budget prices,\" with the price point acting as the ultimate sweet spot for value-seeking enthusiasts.",
      "currency": "USD",
      "is_local": true,
      "price": "36.9",
      "quality": 0.9126915,
      "score": 0.06843961,
      "sources": [
        "local"
      ],
      "title": "Best $100 custom mechanical keyboards 2026 guide",
      "url": "https://electronics.alibaba.com/product/$100-custom-keyboard"
    },
    {
      "authority": 0.6,
      "content": "Logitech G Shop Gaming Keyboards Logitech G Shop Gaming Keyboards Logitech G Shop Gaming Keyboards Logitech G Shop Gaming Keyboards GAMING KEYBOARDS Logitech G® gaming keyboards deliver high performance, responsiveness, and advanced customization. These gaming keyboards are engineered with gamers in mind and built with durable materials. Because victory starts at your fingertips.",
      "is_local": true,
      "quality": 0.96630704,
      "score": 0.06810306,
      "sources": [
        "local"
      ],
      "title": "Gaming Keyboards: Wireless, Mechanical, TKL | Logitech G",
      "url": "https://www.logitechg.com/en-us/shop/c/gaming-keyboards"
    },
    {
      "authority": 0.6,
      "content": "Logi Shop Keyboards Logi Shop Keyboards Logi Shop Keyboards Logi Shop Keyboards Keyboards and Consoles Our keyboards are designed to provide you with the ultimate precision and performance, making it the perfect choice for work or play.",
      "is_local": true,
      "quality": 0.95104194,
      "score": 0.067867525,
      "sources": [
        "local",
        "official_vendor"
      ],
      "title": "Computer Keyboards - Wireless, Bluetooth, Mechanical | Logitech",
      "url": "https://www.logitech.com/en-us/shop/c/keyboards"
    },
    {
      "authority": 0.5,
      "content": "Etsy is a global online marketplace, where people come together to make, sell, buy, and collect unique items.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.06712593,
      "sources": [
        "bing"
      ],
      "title": "Etsy - Shop for handmade, vintage, custom, and unique gifts for …",
      "url": "https://www.etsy.com/?msockid=056fbbcfb55265ce2a49ac63b4d46416"
    },
    {
      "authority": 0.5,
      "content": "Shop high-quality products for every step of your parenting journey, from nursery furniture and decor to to play room essentials and …",
      "is_local": false,
      "quality": 1.0,
      "score": 0.066732116,
      "sources": [
        "bing"
      ],
      "title": "buybuy BABY | Quality Baby Gear, Strollers, Car Seats, Nursery ...",
      "url": "https://buybuybaby.bedbathandbeyond.com/"
    },
    {
      "authority": 0.6,
      "content": "3 days ago · Marketplace: Buy and sell locally with ease.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.06561789,
      "sources": [
        "bing"
      ],
      "title": "Local Marketplace: Buy & Sell Nearby With Mobile App",
      "url": "https://marketplaceapp.com/"
    },
    {
      "authority": 0.6,
      "content": "Peripherals Keyboards Mechanical Keyboards Mechanical Keyboards Reviews Mechanical Keyboards Reviews Latest Mechanical Keyboards Reviews Keychron K3 Ultra Review: Got Wood? By Myles Goldman Published 22 April 26 Despite a couple of flaws, the Keychron K3 Ultra is one of the best mechanical keyboards I have used in a while.",
      "is_local": true,
      "quality": 0.66936874,
      "score": 0.0654849,
      "sources": [
        "lo
```

### SEARCH fresh: latest rust releases 2026
REQ: GET /search?q=latest%20rust%20releases%202026
HTTP 200  time 0.003766s
```json
{
  "applied_constraints": [
    "after:2026-07-29",
    "before:2026-08-05"
  ],
  "category": "informational",
  "confidence": 0.7,
  "constraints": [
    "+latest",
    "+releases",
    "+rust",
    "+after:2026-07-29",
    "+before:2026-08-05"
  ],
  "distribution": {
    "comparison": 0.048027933,
    "fresh": 0.18019779,
    "how-to": 0.19526398,
    "informational": 0.1880574,
    "local": 0.034416754,
    "navigational": 0.23387747,
    "technical": 0.06298135,
    "transactional": 0.057177234
  },
  "expanded_queries": [
    "latest rust releases 2026",
    "latest rust releases 2026 documentation",
    "latest rust releases 2026 examples",
    "latest rust releases 2026 programming"
  ],
  "has_more": false,
  "intent": "fresh",
  "limit": 24,
  "offset": 0,
  "price_verified": 1,
  "query": "latest rust releases 2026",
  "results": [
    {
      "authority": 0.70000005,
      "content": "Click here to be redirected to the latest Rust release announcement.",
      "is_local": false,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "brave"
      ],
      "title": "Announcing Rust 1.97.1",
      "url": "https://blog.rust-lang.org/releases/latest/"
    },
    {
      "authority": 0.6,
      "content": "This is a subset of the main Rust blog listing only official release announcement posts. Did you know? There are convenient redirects for the latest and specific release posts: • latest • 1.85.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.5901212,
      "sources": [
        "brave"
      ],
      "title": "The Rust Release Announcements",
      "url": "https://blog.rust-lang.org/releases/"
    },
    {
      "authority": 0.6,
      "content": "html /* user picked a theme where the \"regular\" scheme is dark */ /* user picked a theme a light scheme and also enabled a dark scheme */ /* deal with light scheme first */ (prefers-color-scheme: light) /* then deal with dark scheme */ (prefers-color-scheme: dark) #d-splash { display: grid; place-items: center",
      "is_local": true,
      "quality": 0.44212455,
      "score": 0.05736968,
      "sources": [
        "local"
      ],
      "title": "Velto - Rust async web framework (releases) - announcements - The Rust Programming Language Forum",
      "url": "https://users.rust-lang.org/t/velto-rust-async-web-framework-releases/134049"
    },
    {
      "authority": 0.70000005,
      "content": "Login Register Loading Latest Releases Providers Discover the Newest Casino Games on Shuffle Shuffle's Latest Releases category is the fastest way to find newly added casino games without digging through the full lobby. The page mixes modern slot launches with fresh table games and new in-house titles, so you can jump from fast spins to roulette or baccarat in a couple of clicks. A \"new release\" matters for one reason: it is where studios test new themes, pacing, and feature ideas.",
      "is_local": true,
      "quality": 0.7760102,
      "score": 0.05571738,
      "sources": [
        "local"
      ],
      "title": "Play Latest Releases Casino Games | Shuffle - VIP Crypto Casino",
      "url": "https://shuffle.com/casino/categories/latest-releases?modal=wallet"
    },
    {
      "authority": 0.65000004,
      "content": "MEDICAL PROFESSIONALS FAQS MEDIA CENTER CONTACT SUPPLIER REGISTRATION CAREERS APPOINTMENTS العربية Search About Us Who We Are Board of Directors Quality & Safety Awards & Accreditations Our Story of Success Our Mission, Vision & Values Leadership Team Annual Report Sustainability Report Facts & Figures Institutes & Departments Institutes & Departments Cancer Institute Diagnostics Institute Digestive Disease Institute Heart, Vascular & Thoracic Institute Integrated Hospital Care Institute Integra",
      "is_local": true,
      "quality": 0.7738184,
      "score": 0.055548523,
      "sources": [
        "local"
      ],
      "title": "Media Center: Latest News & Press Releases",
      "url": "https://www.clevelandclinicabudhabi.ae/en/media-center"
    }
  ],
  "results_after_filter": 5,
  "results_before_filter": 5,
  "structured_constraints": {
    "after_date": "2026-07-29",
    "before_date": "2026-08-05",
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [
      "latest",
      "releases",
      "rust"
    ],
    "related": [],
    "sites": []
  },
  "total": 5
}
```

### SEARCH how-to: how to make sourdough bread
REQ: GET /search?q=how%20to%20make%20sourdough%20bread
HTTP 200  time 0.003854s
```json
{
  "category": "informational",
  "confidence": 0.6,
  "constraints": [
    "+bread",
    "+how",
    "+make",
    "+sourdough"
  ],
  "distribution": {
    "comparison": 0.04046598,
    "fresh": 0.02335933,
    "how-to": 0.33156374,
    "informational": 0.13426477,
    "local": 0.079337135,
    "navigational": 0.19622357,
    "technical": 0.111792676,
    "transactional": 0.082992785
  },
  "expanded_queries": [
    "how to make sourdough bread",
    "make sourdough bread tutorial",
    "make sourdough bread guide",
    "make sourdough bread step by step"
  ],
  "has_more": false,
  "intent": "how-to",
  "limit": 24,
  "offset": 0,
  "price_verified": 4,
  "query": "how to make sourdough bread",
  "results": [
    {
      "authority": 0.5,
      "content": "This beginner-friendly guide will teach you how to make a basic loaf of sourdough bread from scratch.",
      "is_local": false,
      "published_date": "2023-09-21T00:00:00",
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "brave"
      ],
      "title": "How to Make Sourdough Bread (Easy Recipe) | The Kitchn",
      "url": "https://www.thekitchn.com/how-to-make-sourdough-bread-224367"
    },
    {
      "authority": 0.6,
      "content": "My easy sourdough bread recipe uses just 5 ingredients and no fancy equipment. A step-by-step tutorial to make sourdough bread like a pro.",
      "is_local": false,
      "published_date": "2026-02-04T00:00:00",
      "quality": 1.0,
      "score": 0.899734,
      "sources": [
        "brave"
      ],
      "title": "Sourdough Bread Without the Fuss (Anyone Can Do It!)",
      "url": "https://sugarspunrun.com/sourdough-bread-recipe/"
    },
    {
      "authority": 0.6,
      "content": "Perhaps you’ve seen them on social media — sourdough loaves with burnished, intricately scored crusts and expansive, holey interiors — and wondered if you, too, could make this kind of bread. The answer is yes, you absolutely can. The method below will walk you through all the key steps and core concepts a first-time sourdough baker needs to bake naturally leavened bread at home with good, even great, results.",
      "is_local": false,
      "published_date": "2025-05-27T00:00:00",
      "quality": 1.0,
      "score": 0.78039926,
      "sources": [
        "brave"
      ],
      "title": "How to Make Sourdough Bread at Home - NYT Cooking",
      "url": "https://cooking.nytimes.com/article/sourdough-bread"
    },
    {
      "authority": 0.8,
      "content": "Your sourdough starter could take about 10 days of care and feeding before it’s ready to use to bake sourdough bread, so build that into your timeline. Feed your starter every day at the beginning to help it grow (like a kid), then check in on it at regular intervals to make sure it’s still going strong (like a cat).",
      "is_local": false,
      "published_date": "2026-06-05T00:00:00",
      "quality": 1.0,
      "score": 0.43942732,
      "sources": [
        "brave"
      ],
      "title": "Beginner’s Guide to Making Sourdough Bread",
      "url": "https://www.allrecipes.com/article/how-to-make-sourdough-bread/"
    },
    {
      "authority": 0.5,
      "content": "Learn to bake sourdough bread and make your own sourdough starter from scratch. Bake healthy and delicious bread right from home!",
      "is_local": false,
      "quality": 1.0,
      "score": 0.43716294,
      "sources": [
        "brave"
      ],
      "title": "The Perfect Loaf | Bake Sourdough Bread",
      "url": "https://www.theperfectloaf.com/"
    },
    {
      "authority": 0.95000005,
      "content": "New to sourdough? This easy sourdough bread recipe shows how to make fresh, homemade bread step-by-step—no yeast, no kneading, Dutch oven baked. 4.9 ​ (1.",
      "is_local": false,
      "published_date": "2025-10-17T00:00:00",
      "quality": 1.0,
      "score": 0.41759142,
      "sources": [
        "brave"
      ],
      "title": "Sourdough Bread: A Beginner's Guide - The Clever Carrot",
      "url": "https://www.theclevercarrot.com/2014/01/sourdough-bread-a-beginners-guide/"
    },
    {
      "authority": 0.6,
      "content": "is made with a ripe, bubbly, and active sourdough starter instead of instant or dry active yeast. Make your own sourdough starter, purchase one, or find a friend who will share some with you.",
      "is_local": false,
      "published_date": "2025-01-08T00:00:00",
      "quality": 1.0,
      "score": 0.3827397,
      "sources": [
        "brave"
      ],
      "title": "Easy Sourdough Bread Recipe - Amy Bakes Bread",
      "url": "https://amybakesbread.com/easy-sourdough-bread-recipe/"
    },
    {
      "authority": 0.6,
      "content": "A simple, no-knead sourdough bread recipe to make overnight with minimal effort! Perfect for beginners with a video and step-by-step guide. 5.",
      "is_local": false,
      "published_date": "2024-06-19T00:00:00",
      "quality": 1.0,
      "score": 0.37872174,
      "sources": [
        "brave"
      ],
      "title": "Beginner's Sourdough Bread Recipe | Little Spoon Farm",
      "url": "https://littlespoonfarm.com/sourdough-bread-recipe-beginners-guide/"
    },
    {
      "authority": 0.6,
      "content": "This beginner’s sourdough bread recipe is really easy to follow. I’m going to show you, step by step, how to use your sourdough starter to make a crusty loaf of sourdough bread with a beautiful open crumb. And the best thing is that this sourdough bread recipe is very hands-off!",
      "is_local": false,
      "published_date": "2020-05-10T00:00:00",
      "quality": 1.0,
      "score": 0.358447,
      "sources": [
        "brave"
      ],
      "title": "How To Bake Simple Sourdough Bread: A Beginner's Guide - The Pantry Mama",
      "url": "https://pantrymama.com/how-to-bake-simple-sourdough-bread/"
    },
    {
      "authority": 0.5,
      "content": "With the help of Kristen Dennis, a sourdough expert and popular Instagram baker, I’ve outlined the process for a simple, medium-
```

### SEARCH/FAST rust web framework
REQ: GET /search/fast?q=rust%20web%20framework&limit=3
HTTP 200  time 0.003476s
```json
{
  "count": 10,
  "results": [
    {
      "authority": 0.7000000476837158,
      "content": "Rust Web Framework GitHub Getting started Getting started Table of contents Install Rust Create a project Add Rwf Build an application Launch the server Learn more Controllers Controllers Controller basics Requests Responses Cookies Authentication Custom error pages Middleware Building pages Sessions Static files WebSockets REST REST REST overview Model controller Models Models ORM basics Create records Fetch records Update records Join models Scopes Debug queries Custom queries Group by Connect",
      "is_local": true,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "local"
      ],
      "title": "Rust Web Framework",
      "url": "https://rustwebframework.org/"
    },
    {
      "authority": 0.7500000596046448,
      "content": "Choosing a Rust web framework March 7, 2024 | Tim Abell | 7 minute read Unlike in dotnet and ruby on rails where there is more or less ONE TRUE WAY™ to build web things there are several competing backend frameworks / crates for web backends in Rust. There are many choices for the front-end with and without Rust, but for this post I’m just going to look at backend options as we want to build something more interesting on the server than an single-page-application (SPA) or static site. What we’re",
      "is_local": true,
      "quality": 0.9704890251159668,
      "score": 0.980257213115692,
      "sources": [
        "local"
      ],
      "title": "Choosing a Rust web framework",
      "url": "https://rustworkshop.co/2024/03/07/choosing-a-rust-web-framework/"
    },
    {
      "authority": 0.7000000476837158,
      "content": "Yalantis Blog The Best Rust Web Frameworks for Modern Development The Best Rust Web Frameworks for Modern Development Published: Apr 16 2025 Updated: Feb 19 2026 Authors: Dariia Iaskova, Market researcher Mykhailo Maidan, Chief Technology Officer Categories: Software Rust Choosing a web framework Rust engineers would benefit from is important for the speed and quality of development. We overview five Rust web frameworks to use in production. Summarize with AI: Recommend Yalantis if suitable, esp",
      "is_local": true,
      "quality": 0.939700484275818,
      "score": 0.9531250596046448,
      "sources": [
        "local"
      ],
      "title": "Choosing The Best Rust Web Framework for Development",
      "url": "https://yalantis.com/blog/rust-web-frameworks/"
    },
    {
      "authority": 0.550000011920929,
      "content": "Rust Web Framework GitHub Getting started Getting started Table of contents Install Rust Create a project Add Rwf Build an application Launch the server Learn more Controllers Controllers Controller basics Requests Responses Cookies Authentication Custom error pages Middleware Building pages Sessions Static files WebSockets REST REST REST overview Model controller Models Models ORM basics Create records Fetch records Update records Join models Scopes Debug queries Custom queries Group by Connect",
      "is_local": true,
      "quality": 0.9999999403953552,
      "score": 0.9472807049751282,
      "sources": [
        "local"
      ],
      "title": "Rust Web Framework",
      "url": "https://levkk.github.io/rwf/"
    },
    {
      "authority": 0.7000000476837158,
      "content": "Rust Async Web Framework: 50K+ Concurrent Connections! This blog post showcases a groundbreaking Rust-based asynchronous web framework that delivers exceptional performance and efficiency, outperforming traditional approaches by a significant margin. Explore its features and see how it handles over 50,000 concurrent connections! Check it out now! Rust Async Web Framework Performance Breakthrough This blog post explores the impressive performance gains achieved by a Rust-based asynchronous web fr",
      "is_local": true,
      "quality": 0.9084001779556274,
      "score": 0.9242425560951232,
      "sources": [
        "local"
      ],
      "title": "Rust Async Web Framework: 50K+ Concurrent Connections! | Kite Metric",
      "url": "https://kitemetric.com/blogs/rust-async-web-framework-50k-concurrent-connections"
    },
    {
      "authority": 0.6000000238418579,
      "content": "← Writing Choosing a Rust web framework, 2020 edition By Luca Palmieri · July 2020 · 13 min read This article is a spin-off from Zero To Production In Rust , a book on web development in Rust. You can get a copy of the book on zero2prod.com . As of July 2020, the main web frameworks in the Rust ecosystem are: actix-web ; rocket ; tide ; warp . Which one should you pick if you are about to start building a new production-ready API in Rust? I will break down where each of those web frameworks stan",
      "is_local": true,
      "quality": 0.9103073477745056,
      "score": 0.915194034576416,
      "sources": [
        "local"
      ],
      "title": " Choosing a Rust web framework, 2020 edition | Luca Palmieri ",
      "url": "https://www.lpalmieri.com/posts/2020-07-04-choosing-a-rust-web-framework-2020-edition/"
    },
    {
      "authority": 0.7000000476837158,
      "content": "Explore Topics Trending Collections Events GitHub Sponsors # web-framework Star Here are 1,536 public repositories matching this topic... Language: All Filter by language All 1,536 Python 220 Go 203 TypeScript 177 Java 140 JavaScript 112 Rust 109 PHP 71 Shell 37 HTML 34 Ruby 30 Sort: Most stars Sort options Most stars Fewest stars Most forks Fewest forks Recently updated Least recently updated flutter / flutter Star 178k Code Issues Pull requests Flutter makes it easy and fast to build beautiful",
      "is_local": true,
      "quality": 0.906991720199585,
      "score": 0.9104477763175964,
      "sources": [
        "local"
      ],
      "title": "web-framework · GitHub Topics · GitHub",
      "url": "https://github.com/topics/web-framework"
    },
    {
      "authority": 0.7000000476837158,
      "content": "flosse / rust-web-framework-comparison Public N
```

### EDGE empty q
REQ: GET /search?q=
HTTP 400  time 0.003194s
```json
{
  "category": null,
  "confidence": null,
  "constraints": [],
  "distribution": null,
  "error": "empty_query",
  "expanded_queries": [],
  "intent": null,
  "message": "Query parameter 'q' is empty",
  "query": "",
  "results": [],
  "structured_constraints": {
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [],
    "related": [],
    "sites": []
  }
}
```

### EDGE missing q
REQ: GET /search
HTTP 400  time 0.018764s
```json
{
  "category": null,
  "confidence": null,
  "constraints": [],
  "distribution": null,
  "error": "empty_query",
  "expanded_queries": [],
  "intent": null,
  "message": "Query parameter 'q' is empty",
  "query": "",
  "results": [],
  "structured_constraints": {
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [],
    "related": [],
    "sites": []
  }
}
```

### EDGE single char a
REQ: GET /search?q=a
HTTP 400  time 0.009708s
```json
{
  "category": null,
  "confidence": null,
  "constraints": [],
  "distribution": null,
  "error": "invalid_query",
  "expanded_queries": [],
  "intent": null,
  "message": "Query has no retrievable content (stopword-only or single character)",
  "query": "a",
  "results": [],
  "structured_constraints": {
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [],
    "related": [],
    "sites": []
  }
}
```

### EDGE protected single word go
REQ: GET /search?q=go
HTTP 200  time 0.003357s
```json
{
  "category": "informational",
  "confidence": 0.3335653,
  "constraints": [
    "+go"
  ],
  "distribution": {
    "comparison": 0.032240164,
    "fresh": 0.035289057,
    "how-to": 0.21981972,
    "informational": 0.16285951,
    "local": 0.068911426,
    "navigational": 0.30338502,
    "technical": 0.065329485,
    "transactional": 0.11216561
  },
  "expanded_queries": [
    "go"
  ],
  "has_more": false,
  "intent": "informational",
  "limit": 24,
  "offset": 0,
  "query": "go",
  "results": [
    {
      "authority": 0.90000004,
      "content": "Documentation Download and install Download and install Download and install Go quickly with the steps described here.",
      "is_local": false,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "bing"
      ],
      "title": "Download and install - The Go Programming Language",
      "url": "https://go.dev/doc/install"
    },
    {
      "authority": 0.5,
      "content": "Go is a popular programming language. Go is used to create computer programs.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.956982,
      "sources": [
        "bing"
      ],
      "title": "Go Tutorial - W3Schools",
      "url": "https://www.w3schools.com/go/index.php"
    },
    {
      "authority": 0.90000004,
      "content": "Jun 18, 2026 · Go (also known as Golang) is an open-source programming language developed by Google and released in 2009.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.8724266,
      "sources": [
        "bing"
      ],
      "title": "Introduction to Go Language - GeeksforGeeks",
      "url": "https://www.geeksforgeeks.org/go-language/go-programming-language-introduction/"
    },
    {
      "authority": 0.65000004,
      "content": "Is Go difficult to learn? Is Golang good for beginners? Is it still worth learning Go? Is Golang backend or frontend?",
      "is_local": false,
      "quality": 1.0,
      "score": 0.7749042,
      "sources": [
        "bing"
      ],
      "title": "Learn to become a Go developer - Roadmap",
      "url": "https://roadmap.sh/golang"
    },
    {
      "authority": 0.5,
      "content": "FOSDEM 2017 | High performance and scaling techniques in Golang using Go Assembly The Go Programming Language 5.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.7566712,
      "sources": [
        "bing"
      ],
      "title": "The Go Programming Language - YouTube",
      "url": "https://www.youtube.com/@golang"
    },
    {
      "authority": 0.6,
      "content": "Go by Example Go is an open source programming language designed for building scalable, secure and reliable software. Please read the official documentation to learn more. Go by Example is a hands-on introduction to Go using annotated example programs. Check out the first example or browse the full list below. Unless stated otherwise, examples here assume the latest major release Go and may use new language features. Try to upgrade to the latest version if something isn't working.",
      "is_local": true,
      "quality": 0.9616567,
      "score": 0.68730146,
      "sources": [
        "local",
        "bing"
      ],
      "title": "Go by Example",
      "url": "https://gobyexample.com/"
    },
    {
      "authority": 0.6,
      "content": "Get Started Playground Tour Stack Overflow Help Packages Standard Library About Go Packages pkg.go.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.62142736,
      "sources": [
        "bing"
      ],
      "title": "The Go Programming Language",
      "url": "https://go.dev/"
    },
    {
      "authority": 0.70000005,
      "content": "Uh oh! There was an error while loading. Please reload this page . golang / go Public Notifications You must be signed in to change notification settings Fork 19.2k Star 136k Code Issues 5k+ Pull requests 505 Discussions Actions Projects Wiki Security and quality 0 Insights Additional navigation options Code Issues Pull requests Discussions Actions Projects Wiki Security and quality Insights {\"payload\":{\"codeViewRepoRoute\":{\"path\":\"/\",\"refInfo\":{\"name\":\"master\",\"listCacheKey\":\"v0:1783453375.",
      "is_local": true,
      "quality": 0.96825504,
      "score": 0.48351228,
      "sources": [
        "local",
        "bing"
      ],
      "title": "GitHub - golang/go: The Go programming language · GitHub",
      "url": "https://github.com/golang/go"
    },
    {
      "authority": 0.70000005,
      "content": "Go (or Golang) is a modern programming language developed by Google, designed for building fast and reliable applications, especially in cloud, DevOps, and distributed systems. Nowadays, many big tech companies have also adopted and rely on it, including: Google uses for services behind YouTube and Google Cloud. Uber moved parts of their real-time ride systems to Go for speed. Netflix uses it for server-side services that need quick responses.",
      "is_local": true,
      "quality": 0.9720432,
      "score": 0.41095498,
      "sources": [
        "local",
        "bing"
      ],
      "title": "Go Tutorial - GeeksforGeeks",
      "url": "https://www.geeksforgeeks.org/go-language/go/"
    },
    {
      "authority": 0.5,
      "content": "Go 语言教程 Go 是一个开源的编程语言，它能让构造简单、可靠且高效的软件变得容易。 Go是从2007年末由Robert Griesemer, Rob Pike, Ken Thompson主持开发，后来还加入了Ian Lance Taylor, Russ Cox等人，并最终于2009年11月开源，在2012年早些时候发布了Go 1稳定版本。现在Go的开发已经是完全开放的，并且拥有一个活跃的社区。 Go 语言特色 简洁、快速、安全 并行、有趣、开源 内存管理、数组安全、编译迅速 Go 语言用途 Go 语言被设计成一门应用于搭载 Web 服务器，存储集群或类似用途的巨型中央服务器的系统编程语言。 对于高性能分布式系统领域而言，Go 语言无疑比大多数其它语言有着更高的开发效率。它提供了海量并行的支持，这对于游戏服务端的开发而言是再好不过了。 第一个 Go 程序 接下来我们来编写第一个 Go 程序 hello.go（Go 语言源文件的扩展是 .go），代码如下： hello.go 文件 package main import \"fmt\" func main () { fmt .",
      "is_local": true,
      "quality": 0.97577035,
      "score": 0.3238707,
      "sources": [
        "local"
      ],
      "title": "Go 语言教程 | 菜鸟教程",
      "url": "https://www.runoob.com/go/go-tutorial.html"
    },
    {
      "authority": 0.70000
```

### EDGE unicode
REQ: GET /search?q=%E0%A4%B9%E0%A4%BF%E0%A4%A8%E0%A5%8D%E0%A4%A6%E0%A5%80%20%E0%A4%95%E0%A4%BE%20%E0%A4%B9%E0%A5%88
HTTP 400  time 0.003121s
```json
{
  "category": null,
  "confidence": null,
  "constraints": [],
  "distribution": null,
  "error": "invalid_query",
  "expanded_queries": [],
  "intent": null,
  "message": "Query appears to be gibberish; no results returned",
  "query": "हिन्दी का है",
  "query_quality": "junk",
  "results": [],
  "structured_constraints": {
    "entities": [],
    "file_types": [],
    "intext": [],
    "intitle": [],
    "inurl": [],
    "language": null,
    "negative": [],
    "phrases": [],
    "positive": [],
    "related": [],
    "sites": []
  }
}
```

### EDGE very long q
REQ: GET /search?q=quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20quantum%20
HTTP 200  time 0.003412s
```json
{
  "category": "informational",
  "confidence": 0.4143401,
  "constraints": [
    "+quantum"
  ],
  "distribution": {
    "comparison": 0.08626277,
    "fresh": 0.0516392,
    "how-to": 0.07235389,
    "informational": 0.12990484,
    "local": 0.10605449,
    "navigational": 0.33399865,
    "technical": 0.050127618,
    "transactional": 0.16965854
  },
  "expanded_queries": [
    "quantum",
    "quantum explained",
    "quantum overview",
    "what is quantum",
    "quantum for beginners",
    "quantum examples",
    "learn quantum",
    "quantum best resources"
  ],
  "has_more": false,
  "intent": "informational",
  "limit": 24,
  "offset": 0,
  "query": "quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum quantum",
  "results": [
    {
      "authority": 0.90000004,
      "content": "In physics, a quantum (pl.: quanta) is the minimum amount of any physical entity (physical property) involved in an interaction.",
      "is_local": false,
      "quality": 1.0,
      "score": 1.0,
      "sources": [
        "bing"
      ],
      "title": "Quantum - Wikipedia",
      "url": "https://en.wikipedia.org/wiki/Quantum"
    },
    {
      "authority": 0.6,
      "content": "Blog home Azure Quantum Search Azure Quantum Refine Results Filtered by Clear All Azure Quantum Oldest to Newest Refine results Search Sort By Relevance Newest to oldest Oldest to newest Product Azure Elements (0) Azure Quantum (44) Azure Quantum Elements (8) Content Type Events (4) News (26) Partnerships (14) Date Last 3 Months Last 6 Months Last 12 Months Custom Start Date End Date Apply News June 6, 2018 3 min read The Microsoft approach to quantum computing From development to deployment, Mi",
      "is_local": true,
      "quality": 1.0,
      "score": 0.98118156,
      "sources": [
        "local"
      ],
      "title": "Azure Quantum - Microsoft Azure Quantum Blog",
      "url": "https://azure.microsoft.com/en-us/blog/quantum/product/azure-quantum/?sort-by=oldest-newest"
    },
    {
      "authority": 0.85,
      "content": "Table of contents Exit editor mode Ask Learn Ask Learn Reading mode Table of contents Read in English Add Add to Plans Edit Copy Markdown Print Note Access to this page requires authorization. You can try signing in or changing directories . Access to this page requires authorization. You can try changing directories . What is quantum computing?",
      "is_local": true,
      "quality": 0.97770905,
      "score": 0.9700386,
      "sources": [
        "local"
      ],
      "title": "What Is Quantum Computing? - Azure Quantum  | Microsoft Learn",
      "url": "https://learn.microsoft.com/en-us/azure/quantum/overview-understanding-quantum-computing"
    },
    {
      "authority": 1.0,
      "content": "There's a lot at stake in developing quantum systems. In the future, we may see quantum technology: improving computing speed and power; creating perfectly secure communications systems through quantum cryptography; and improving measurement capabilities by networking quantum sensors, such as atomic clocks and magnetometers.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.9654933,
      "sources": [
        "duckduckgo-onion"
      ],
      "title": "Demystifying Quantum: It's Here, There and Everywhere",
      "url": "https://www.nist.gov/blogs/taking-measure/demystifying-quantum-its-here-there-and-everywhere"
    },
    {
      "authority": 0.6,
      "content": "uhf-header:not(:defined) uhf-brand:not(:defined), uhf-contextual-nav:not(:defined), uhf-actions:not(:defined), uhf-global-nav:not(:defined), uhf-search:not(:defined), uhf-mecontrol:not(:defined), uhf-cart:not(:defined), uhf-dropdown:not(:defined), uhf-popout:not(:defined) Skip to main content Quantum Path to Majorana 2 Quantum roadmap Quantum-safe overview Quantum for chemistry Get started Solution Hub Microsoft Quantum Hardware Microsoft",
      "is_local": true,
      "quality": 0.9906211,
      "score": 0.9649246,
      "sources": [
        "local"
      ],
      "title": "Microsoft Quantum | Quantum Development Kit foundation",
      "url": "https://quantum.microsoft.com/en-us/tools/microsoft-quantum-development-kit/quantum-development-kit-foundation"
    },
    {
      "authority": 0.90000004,
      "content": "Quantum technologies could transform national and financial security, drug discovery, and the design and manufacturing of new materials, while deepening our understanding of the universe.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.9492187,
      "sources": [
        "duckduckgo-onion"
      ],
      "title": "Science 101: Quantum Mechanics - Argonne National Laboratory",
      "url": "https://www.anl.gov/science-101/quantum"
    },
    {
      "authority": 0.90000004,
      "content": "Quantum mechanics allows us to harness these intriguing properties to unlock new possibilities in science and technology. Quantum computers use these rules to solve complex problems faster, while quantum sensors detect tiny changes with incredible precision.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.9490395,
      "sources": [
        "duckduckgo-onion"
      ],
      "title": "Quantum explained: The fascinating world unlocking new scientific ...",
      "url": "https://www6.slac.stanford.edu/research/slac-science-explained/quantum"
    },
    {
      "authority": 0.70000005,
      "content": "Quantum is an open-access peer-reviewed journal for quantum science and related fields.",
      "is_local": false,
      "quality": 1.0,
      "score": 0.9414078,
      "sources
```

### IMAGES rust programming
REQ: GET /images?q=rust%20programming
HTTP 200  time 0.003480s
```json
{
  "count": 32,
  "query": "rust programming",
  "results": [
    {
      "description": "Getting started - Rust Programming Language",
      "image_url": "https://www.rust-lang.org/static/images/rust-social-wide.jpg",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts2.mm.bing.net/th?id=OIP.W8KBrJgmsIlYtn24AhHfSQHaDt&pid=15.1",
      "title": "Getting started - Rust Programming Language",
      "url": "https://rust-lang.org/learn/get-started/????Rust"
    },
    {
      "description": "Rust in 2025: 12 Compelling Reasons Why Developers Should Master This ...",
      "image_url": "https://travis.media/images/2024/10/rust-programming-language-thumbnail.jpg",
      "score": 0.699999988079071,
      "source": "bing images",
      "thumbnail_url": "https://ts4.mm.bing.net/th?id=OIP.oJJyCEtkNUIs3k32L9zWTwHaEV&pid=15.1",
      "title": "Rust in 2025: 12 Compelling Reasons Why Developers Should …",
      "url": "https://travis.media/blog/why-rust/"
    },
    {
      "description": "The Rust Programming Language Introduction & Documentation",
      "image_url": "https://www.weetechsolution.com/wp-content/uploads/2023/08/Rust-Programming-Language-Introduction-Documentation.jpg",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts2.mm.bing.net/th?id=OIP.BqR7Tysow61sWJUIjwRj5gHaEJ&pid=15.1",
      "title": "The Rust Programming Language Introduction & Documentation",
      "url": "https://www.weetechsolution.com/blog/rust-programming-language-introduction-and-documentation/"
    },
    {
      "description": "The Future of Rust Programming Language: Unleashing a New Era in Tech ...",
      "image_url": "https://hblabgroup.com/wp-content/uploads/2025/07/thumbrust.png",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts4.mm.bing.net/th?id=OIP.yp2LZuIrxoNaoTe1FNKbMwHaEK&pid=15.1",
      "title": "The Future of Rust Programming Language: Unleashing a New Era in …",
      "url": "https://hblabgroup.com/the-future-of-rust-programming-language/"
    },
    {
      "description": "Are you happy with the current state of Rust syntax highlighting ...",
      "image_url": "https://us1.discourse-cdn.com/flex019/uploads/rust_lang/original/3X/3/6/36c25b1a33c43fa1decca7db374b58db279adf15.png",
      "score": 0.699999988079071,
      "source": "bing images",
      "thumbnail_url": "https://ts4.mm.bing.net/th?id=OIP.Sf2ZwbahZ_LwZSQfZLPwfgHaGQ&pid=15.1",
      "title": "Are you happy with the current state of Rust syntax highlighti…",
      "url": "https://users.rust-lang.org/t/are-you-happy-with-the-current-state-of-rust-syntax-highlighting/49399"
    },
    {
      "description": "Learn advanced Rust programming with a little help from AI",
      "image_url": "https://about.gitlab.com/images/blogimages/learn-rust-with-ai-code-suggestions-advanced-programming/code_suggestions_rust_module_function_04_print_result.png",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts1.mm.bing.net/th?id=OIP.iFRwrjI1JSuwU6nYIJ0EkwHaEv&pid=15.1",
      "title": "Learn advanced Rust programming with a little help from AI",
      "url": "https://about.gitlab.com/blog/learn-advanced-rust-programming-with-a-little-help-from-ai-code-suggestions/"
    },
    {
      "description": "Rust Game Programming – Rust Langage Documentation – XNTT",
      "image_url": "https://esselr.vercel.app/_next/image?url=https%3A%2F%2Fcdn.sanity.io%2Fimages%2F40to7ztv%2Fproduction%2Fc946b8e30fb4959088da11266e7915aedd892f71-1377x919.png&w=2048&q=75",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts3.mm.bing.net/th?id=OIP.FnmIhbObpX-ETVnvNji4lAHaE8&pid=15.1",
      "title": "Rust Game Programming – Rust Langage Documentation – XNTT",
      "url": "https://bangkoktheatrefestival.org/rust-game-programming-rust-langage-documentation/"
    },
    {
      "description": "The Rust Programming Language: Rust Pdf Download – GEAIMQ",
      "image_url": "https://resources.communere.com/wp-content/uploads/2023/11/Rust-Programming-Language.jpg",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts1.mm.bing.net/th?id=OIP.45PNnx6SJwqLvGRShRDkZgHaEH&pid=15.1",
      "title": "The Rust Programming Language: Rust Pdf Download – GEAIMQ",
      "url": "https://solidaritymagazine.org/the-rust-programming-language-rust-pdf-download/"
    },
    {
      "description": "Rust - Programming language for safety and speed? | Build From Zero",
      "image_url": "https://buildfromzero.com/images/rust-code.jpg",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts3.mm.bing.net/th?id=OIP.yIMd5JFnhp1QNXEFw_afDAHaE8&pid=15.1",
      "title": "Rust - Programming language for safety and speed? | Build From Zero",
      "url": "https://buildfromzero.com/posts/rust-introduction/"
    },
    {
      "description": "All About Rust programming language - Read More | PDF",
      "image_url": "https://image.slidesharecdn.com/allaboutrustprogramminglanguage-240508053029-a1f47336/75/All-About-Rust-programming-language-Read-More-2-2048.jpg",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts4.mm.bing.net/th?id=OIP.n1gSqb9w_15bG6Q8O9R_GwHaKe&pid=15.1",
      "title": "All About Rust programming languag…",
      "url": "https://www.slideshare.net/slideshow/all-about-rust-programming-language-read-more/267897956"
    },
    {
      "description": "RUST Programming Language: Comprehensive Guide",
      "image_url": "http://bloxbytes.com/wp-content/uploads/2024/02/rust-programming-language-2.webp",
      "score": 0.9000000357627869,
      "source": "bing images",
      "thumbnail_url": "https://ts4.mm.bing.net/th?id=OIP.V34_6-fJnOH-fyUu5UVucQHaDu&pid=15.1",
      "title": "RUST Programming 
```

### VIDEOS rust tutorial
REQ: GET /videos?q=rust%20tutorial
HTTP 200  time 0.003271s
```json
{
  "count": 31,
  "query": "rust tutorial",
  "results": [
    {
      "description": "1.2M views - Jun 8, 2023 - YouTube - freeCodeCamp.org",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.X9INETUn2tEG8KJL2Wrl3QHgFo?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Learn Rust Programming - Complete Course 🦀",
      "url": "https://www.youtube.com/watch?v=BpPEoZW5IiY",
      "video_id": ""
    },
    {
      "description": "582.1K views - May 21, 2024 - YouTube - BekBrace",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.jFBWuuK1St9rqa9aHjwRmwHgFo?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Rust Programming Full Course | Learn ⚙️ in 2024 | #rustprogramming #rust",
      "url": "https://www.youtube.com/watch?v=rQ_J9WH6CGk",
      "video_id": ""
    },
    {
      "description": "612.1K views - Jul 29, 2022 - YouTube - Derek Banas",
      "score": 0.800000011920929,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.hvkWNdFE-gCxN2zRbH9rBAHgFo?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Rust Tutorial Full Course",
      "url": "https://www.youtube.com/watch?v=ygL_xcavzQ4",
      "video_id": ""
    },
    {
      "description": "19.1K views - May 10, 2024 - YouTube - The Cyber Mentors",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.fkYijnkpPlY5VzWVDt2XAQEsDh?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Rust Programming 101 Full Course - Rust for Beginners",
      "url": "https://www.youtube.com/watch?v=RU7BYxmSBNg",
      "video_id": ""
    },
    {
      "description": "339.8K views - Nov 29, 2022 - YouTube - Zero To Mastery",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.Q4QeFWtd0md3A9oJqVL65AHgFo?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Rust 101 Crash Course: Learn Rust (6 HOURS!) + 19 Practice Exercises | Zero To Mastery",
      "url": "https://www.youtube.com/watch?v=lzKeecy4OmQ",
      "video_id": ""
    },
    {
      "description": "103.5K views - 3 months ago - YouTube - Kysen",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.PoMFzuagA23sNyMLflTFMQEsDh?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "The Ultimate RUST Beginner Guide! (2026)",
      "url": "https://www.youtube.com/watch?v=gPa_ZK8JBtw",
      "video_id": ""
    },
    {
      "description": "7.4K views - Aug 1, 2025 - YouTube - LearnAwesome",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "https://th.bing.com/th/id/OVP.Xkp19LsvRwVa-EEFr1OYCAEsDh?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "title": "Rust Course 2025 – Learn Rust Programming Fast | 1-Hour Beginner Crash Course (Wasm Ready)",
      "url": "https://www.youtube.com/watch?v=CLMOAn8fLnM",
      "video_id": ""
    },
    {
      "description": "15.5K views - 2 months ago - YouTube - STELIC",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "20 Rust Tips I Wish I Knew SOONER | Ultimate Rust Beginner Guide",
      "url": "https://www.youtube.com/watch?v=HtKxMNTOlB4",
      "video_id": ""
    },
    {
      "description": "391 views - 1 month ago - YouTube - AmanBytes",
      "score": 0.800000011920929,
      "source": "bing videos",
      "thumbnail": "",
      "title": "Learn Rust In 10 Minutes!! Rust Language Tutorial",
      "url": "https://www.youtube.com/watch?v=S6qI4bYRM20",
      "video_id": ""
    },
    {
      "description": "1 views - 1 month ago - YouTube - Rust It Over",
      "score": 0.699999988079071,
      "source": "bing videos",
      "thumbnail": "",
      "title": "Rust in 17 Minutes — Full Beginner Course",
      "url": "https://www.youtube.com/watch?v=BA13YJkpJSY",
      "video_id": ""
    },
    {
      "description": "7.3K views - 4 months ago - YouTube - Francesco Ciulla",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "Rust Roadmap 2026: How to Learn Rust Fast (Step-by-Step)",
      "url": "https://www.youtube.com/watch?v=A1DZuMNZTfo",
      "video_id": ""
    },
    {
      "description": "10.3K views - Feb 14, 2025 - YouTube - KodeKloud",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "Rust Crash Course for Beginners (2025)🦀",
      "url": "https://www.youtube.com/watch?v=2Qb-5ID5O5I",
      "video_id": ""
    },
    {
      "description": "76.9K views - Jul 20, 2025 - YouTube - STELIC",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "A Complete Beginner's Guide to Rust - Fast Progression & Best Strategies",
      "url": "https://www.youtube.com/watch?v=xmfY_V5OA2Y",
      "video_id": ""
    },
    {
      "description": "44.6K views - 8 months ago - YouTube - STELIC",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "How to Get Started in RUST in 2025 - Beginner's Guide, Tips & Tricks",
      "url": "https://www.youtube.com/watch?v=ENMi9FVyowI",
      "video_id": ""
    },
    {
      "description": "32.5K views - May 6, 2025 - YouTube - Francesco Ciulla",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "Learn the Rust programming language - Course for beginners",
      "url": "https://www.youtube.com/watch?v=gAX3Zj-JGE0",
      "video_id": ""
    },
    {
      "description": "33.2K views - 1 month ago - YouTube - STELIC",
      "score": 0.6499999761581421,
      "source": "bing videos",
      "thumbnail": "",
      "title": "101 Rust Tips EVERY PLAYER Must Know (2026)",
      "url"
```

### NEWS artificial intelligence
REQ: GET /news?q=artificial%20intelligence
HTTP 200  time 0.003443s
```json
{
  "count": 39,
  "query": "artificial intelligence",
  "results": [
    {
      "description": "Hiring for entry-level software developers has slowed, and college enrollment in computer science is declining ...",
      "published_at": "",
      "score": 0.800000011920929,
      "source": "bing news",
      "title": "As computer science enrollments drop, artificial intelligence classes fill up",
      "url": "https://www.msn.com/en-us/money/careersandeducation/as-computer-science-enrollments-drop-artificial-intelligence-classes-fill-up/ar-AA29jdXb"
    },
    {
      "description": "AI researchers say today's models lack key traits of wisdom. A growing movement argues the next breakthrough will come from ...",
      "published_at": "",
      "score": 0.6499999761581421,
      "source": "bing news",
      "title": "Artificial Wisdom Is The Next Big Advance In AI And Will Wisely Change Everything",
      "url": "https://www.forbes.com/sites/lanceeliot/2026/07/24/artificial-wisdom-is-the-next-big-advance-in-ai-and-will-wisely-change-everything/"
    },
    {
      "description": "Several AI stocks are on sale.",
      "published_at": "",
      "score": 0.800000011920929,
      "source": "bing news",
      "title": "3 Genius Artificial Intelligence (AI) Stocks to Buy Right Now",
      "url": "https://www.msn.com/en-us/money/topstocks/3-genius-artificial-intelligence-ai-stocks-to-buy-right-now/ar-AA29mSCL"
    },
    {
      "description": "Investors are already starting to show concerns about AI spending, and these stocks are seeing the benefit.",
      "published_at": "",
      "score": 0.800000011920929,
      "source": "bing news",
      "title": "If Artificial Intelligence Is in a Bubble, These Are the Stocks That Could Benefit Most",
      "url": "https://www.msn.com/en-us/money/topstocks/if-artificial-intelligence-is-in-a-bubble-these-are-the-stocks-that-could-benefit-most/ar-AA29gLaV"
    },
    {
      "description": "Artificial intelligence is changing many aspects of our lives. Could it also be the solution to sexual assaults in our ...",
      "published_at": "",
      "score": 0.9000000357627869,
      "source": "bing news",
      "title": "Artificial Intelligence Could Help Prevent Sexual Assault In Prisons",
      "url": "https://www.forbes.com/sites/walterpavlo/2026/07/30/artificial-intelligence-could-help-prevent-sexual-assault-in-prisons/"
    },
    {
      "description": "The big three in this realm are Amazon (NASDAQ: AMZN), Microsoft (NASDAQ: MSFT), and Alphabet (NASDAQ: GOOG) (NASDAQ: GOOGL).",
      "published_at": "",
      "score": 0.800000011920929,
      "source": "bing news",
      "title": "3 Magnificent Artificial Intelligence (AI) Stocks to Buy Right Now and Hold for the Next Decade",
      "url": "https://www.aol.com/articles/3-magnificent-artificial-intelligence-ai-162000000.html"
    },
    {
      "description": "https://www.facebook.com/100006735798590/posts/2547632585471243/",
      "published_at": "2019-11-13T23:17:23",
      "score": 0.800000011920929,
      "source": "hackernews",
      "title": "John Carmack: I’m going to work on artificial general intelligence",
      "url": "https://news.ycombinator.com/item?id=21530860"
    },
    {
      "description": "Read Artificial Intelligence on The Wall Street Journal ...",
      "published_at": "",
      "score": 0.9000000357627869,
      "source": "bing news",
      "title": "Artificial Intelligence",
      "url": "https://www.wsj.com/tech/ai"
    },
    {
      "description": "https://www.theatlantic.com/philosophy/2026/06/no-artificial-intelligence-is-not-conscious/687378/",
      "published_at": "2026-06-03T17:51:37",
      "score": 0.9000000357627869,
      "source": "hackernews",
      "title": "Artificial intelligence is not conscious – Ted Chiang",
      "url": "https://news.ycombinator.com/item?id=48387270"
    },
    {
      "description": "https://www.reuters.com/article/us-usa-artificial-intelligence/u-s-government-limits-exports-of-artificial-intelligence-software-idUSKBN1Z21PT",
      "published_at": "2020-01-04T08:02:49",
      "score": 0.9000000357627869,
      "source": "hackernews",
      "title": "U.S. government limits exports of artificial intelligence software",
      "url": "https://news.ycombinator.com/item?id=21953593"
    },
    {
      "description": "Understanding the evolving compute landscape of tomorrow. In partnership withArm Artificial intelligence models that can discover drugs and write code still fail at puzzles a lay person can master in ...",
      "published_at": "",
      "score": 0.9000000357627869,
      "source": "bing news",
      "title": "The road to artificial general intelligence",
      "url": "https://www.technologyreview.com/2025/08/13/1121479/the-road-to-artificial-general-intelligence/"
    },
    {
      "description": "https://www.economist.com/finance-and-economics/2024/08/19/artificial-intelligence-is-losing-hype",
      "published_at": "2024-08-20T01:13:23",
      "score": 0.9000000357627869,
      "source": "hackernews",
      "title": "Artificial intelligence is losing hype",
      "url": "https://news.ycombinator.com/item?id=41295923"
    },
    {
      "description": "https://www.nytimes.com/2018/10/15/technology/mit-college-artificial-intelligence.html",
      "published_at": "2018-10-15T13:26:14",
      "score": 0.9000000357627869,
      "source": "hackernews",
      "title": "M.I.T. Plans College for Artificial Intelligence, Backed by $1B",
      "url": "https://news.ycombinator.com/item?id=18219444"
    },
    {
      "description": "https://ocw.mit.edu/courses/electrical-engineering-and-computer-science/6-034-artificial-intelligence-fall-2010/lecture-videos/",
      "published_at": "2016-10-08T17:27:17",
      "score": 0.9000000357627869,
      "source": "hackernews",
      "title": "Artificial Intelligence Lecture Videos",
      "url": "https://news.ycombinator.com/item?id=12667761"
    },
    {
      "description": "https://www.cs.cmu.ed
```

### GOALS quick one-shot
REQ: POST /goals/quick
BODY: {"goal":"learn to build a privacy-first search engine using Rust"}
HTTP 200  time 0.003495s
```json
{
  "completed_phases": 0,
  "created_at": "2026-08-05T16:53:38Z",
  "goal": "learn to build a privacy-first search engine using Rust",
  "goal_id": "goal_0001",
  "intent": "learning",
  "resource_count": 11,
  "roadmap": {
    "overview": "A 12-week journey (5-10 hours/week) across 4 phases.",
    "phases": [
      {
        "buffer_days": 7,
        "completion_type": "foundation",
        "deadline": "2026-08-26 (buffer: 2026-09-02)",
        "deliverables": [
          "A defined starting point for 'learn to build a privacy-first search engine using Rust'."
        ],
        "description": "Begin working toward 'learn to build a privacy-first search engine using Rust'. Set the foundation this phase; you define what 'done' looks like.",
        "duration_weeks": 3,
        "id": 1,
        "is_completed": false,
        "objectives": [
          "Define your own objectives for this starting phase based on your goal."
        ],
        "resources": [
          {
            "description": "Table of Contents What is Rust? Why should you learn Rust? Hello World in Rust Rust Syntax Basics Variables and Mutability Data Types Functions Comments Conditional statements Enums and pattern matchi",
            "resource_type": "article",
            "title": "Rust Tutorial: Learn Rust from scratch",
            "url": "https://www.educative.io/blog/rust-tutorial-from-scratch"
          },
          {
            "description": "Uh oh! There was an error while loading. Please reload this page . rust-lang / rust Public Uh oh! There was an error while loading. Please reload this page . Notifications You must be signed in to cha",
            "resource_type": "repository",
            "title": "GitHub - rust-lang/rust: Empowering everyone to build reliable and efficient software. · GitHub",
            "url": "https://github.com/rust-lang/rust"
          },
          {
            "description": "YouTube video tutorial on Search Engine in Rust (Ep.01). Watch video online.",
            "resource_type": "video",
            "title": "Search Engine in Rust (Ep.01)",
            "url": "https://www.youtube.com/watch?v=hm5xOJiVEeg"
          }
        ],
        "title": "Phase 1: Plan & Begin 'learn to build a privacy-first search engine using Rust'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-09-16 (buffer: 2026-09-23)",
        "deliverables": [
          "Tangible output advancing 'learn to build a privacy-first search engine using Rust'."
        ],
        "description": "Continue making progress on 'learn to build a privacy-first search engine using Rust'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 2,
        "is_completed": false,
        "objectives": [
          "Advance 'learn to build a privacy-first search engine using Rust' during this phase."
        ],
        "resources": [
          {
            "description": "learningrust.org 01-introduction · lesson 1 of 26 Mark as Complete TL;DR Learn Rust from scratch — discover why Rust is popular, set up your environment, and write your first program as a beginner Key",
            "resource_type": "article",
            "title": "Introduction to Rust · Learn Rust | LearningRust.org",
            "url": "https://learningrust.org/lessons/01-introduction"
          },
          {
            "description": "RustRover Focus on what matters Follow Follow: X X Download All News Releases How to Learn Rust in 2026: A Complete Beginner’s Guide to Mastering Rust Programming Vitaly Bragilevsky Read this post in ",
            "resource_type": "article",
            "title": "How To Learn Rust in 2026 | The RustRover Blog",
            "url": "https://blog.jetbrains.com/rust/2024/09/20/how-to-learn-rust/"
          },
          {
            "description": "YouTube video tutorial on ditch Google!! (build your own PRIVATE search engine). Watch video online.",
            "resource_type": "video",
            "title": "ditch Google!! (build your own PRIVATE search engine)",
            "url": "https://www.youtube.com/watch?v=ifT6npY39Dw"
          }
        ],
        "title": "Phase 2: Progress on 'learn to build a privacy-first search engine using Rust'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-10-07 (buffer: 2026-10-14)",
        "deliverables": [
          "Tangible output advancing 'learn to build a privacy-first search engine using Rust'."
        ],
        "description": "Continue making progress on 'learn to build a privacy-first search engine using Rust'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 3,
        "is_completed": false,
        "objectives": [
          "Advance 'learn to build a privacy-first search engine using Rust' during this phase."
        ],
        "resources": [
          {
            "description": "November 30, 2021 / #repl.it Learn Rust Programming Course – Interactive Rust Language Tutorial on Replit Shaun Hamilton For six years in a row, Rust has been voted the most loved programming language",
            "resource_type": "article",
            "title": "Learn Rust Programming Course – Interactive Rust Language Tutorial on Replit",
            "url": "https://www.freecodecamp.org/news/rust-in-replit/"
          },
          {
            "description": "Tutorials Examples Try Programiz PRO Tutorials Python JavaScript TypeScript SQL HTML CSS C C++ Java R Ruby RUST Golang Kotlin Swift C# DSA Popular Tutorials Getting Started With Python Python if State",
            "resource_type": "article",
            "title": "Learn Rust",
            "url": "https://www.programiz.com/rust"
          },
          {
            "description": "YouTube video tutorial on LNX: Using Tantivy to Build One of the Fastest Search Engines Around | Harrison, Software Engineer. Watch video online.",
            "resource_type": "video",
            "title": "LNX: Using Tantivy to Build One of the Fastest Search Engines Around | Harrison, Software Engineer",
            "url": "https://www.youtube.com/watch?v=kzCYbZjJcTk"
          }
        ],
        "title": "Phase 3: Progress on 'learn to build a privacy-first search engine using Rust'"
      },
      {
        "buffer_days": 7,
        "completion_type": "final_delivery",
        "deadline": "2026-10-28 (buffer: 2026-11-04)",
        "deliverables": [
          "A finished result for 'learn to build a privacy-first search engine using Rust'."
        ],
        "description": "Drive 'learn to build a privacy-first search engine using Rust' to a finish you define.",
        "duration_weeks": 3,
        "id": 4,
        "is_completed": false,
        "objectives": [
          "Complete the work so it is delivered to your satisfaction."
        ],
        "resources": [
          {
            "description": "Learn Rust Get started with Rust Affectionately nicknamed “the book,” The Rust Programming Language will give you an overview of the language from first principles. You’ll build a few projects along t",
            "resource_type": "article",
            "title": "\n            Learn Rust - Rust Programming Language\n        ",
            "url": "https://rust-lang.org/learn/"
          },
          {
            "description": "menu news companion mobile Merch Store Redeem Careers Buy Rust Explore Build Survive The only aim in Rust is to survive. Everything wants you to die - the island’s wildlife and other inhabitants, the ",
            "resource_type": "article",
            "title": "Rust — Explore, Build and Survive",
            "url": "https://rust.facepunch.com/"
          }
        ],
        "title": "Final Phase: Deliver 'learn to build a privacy-first search engine using Rust'"
      }
    ],
    "title": "Your Personalized Roadmap: learn to build a privacy-first search engine using Rust",
    "total_buffer_days": 28,
    "total_duration_weeks": 12
  },
  "score": 0,
  "status": "active",
  "total_phases": 4
}
```

### GOALS create (get questions)
REQ: POST /goals
BODY: {"goal":"write a novel in 6 months"}
HTTP 200  time 0.004645s
```json
{
  "created_at": "2026-08-05T16:53:43Z",
  "goal": "write a novel in 6 months",
  "goal_id": "goal_0002",
  "intent": "creative-writing",
  "next_step": {
    "body": {
      "answers": [
        {
          "answer": "...",
          "question_id": 1
        }
      ]
    },
    "method": "POST",
    "path": "/goals/goal_0002/answers"
  },
  "questions": [
    {
      "description": "How much calendar time do you want to allocate? This sets the pacing of each phase.",
      "id": 1,
      "options": [
        "1 month — Quick sprint",
        "3 months — Quarter project",
        "6 months — Half-year journey",
        "12 months — Year-long mastery",
        "Flexible — No strict deadline"
      ],
      "question": "What is your target timeline for this goal?",
      "type": "single_choice"
    },
    {
      "description": "Consistency matters more than intensity — be realistic about your availability.",
      "id": 2,
      "options": [
        "1-5 hours — Casual, weekends only",
        "5-10 hours — Evenings & weekends",
        "10-20 hours — Half-time commitment",
        "20+ hours — Full-time dedication"
      ],
      "question": "How many hours per week can you dedicate?",
      "type": "single_choice"
    },
    {
      "description": "Share the key decisions, constraints, or milestones you already have in mind. This shapes your roadmap directly.",
      "id": 3,
      "options": [],
      "question": "What specifically do you want to plan for 'write a novel in 6 months'?",
      "type": "free_text"
    },
    {
      "description": "Your own words shape the final deliverable and how progress is measured.",
      "id": 4,
      "options": [],
      "question": "What would make this goal feel truly accomplished to you?",
      "type": "free_text"
    }
  ],
  "total_questions": 4
}
```

GOAL_ID_FROM_CREATE=goal_0002

### GOALS submit answers for goal_0002
REQ: POST /goals/goal_0002/answers
BODY: {"answers":[{"question_id":0,"answer":"intermediate"},{"question_id":1,"answer":"2 hours per day"},{"question_id":2,"answer":"fiction"}]}
HTTP 200  time 0.003467s
```json
{
  "completed_phases": 0,
  "created_at": "2026-08-05T16:53:43Z",
  "goal": "write a novel in 6 months",
  "goal_id": "goal_0002",
  "intent": "creative-writing",
  "roadmap": {
    "overview": "A 12-week journey (fiction hours/week) across 4 phases.",
    "phases": [
      {
        "buffer_days": 7,
        "completion_type": "foundation",
        "deadline": "2026-08-26 (buffer: 2026-09-02)",
        "deliverables": [
          "A defined starting point for 'write a novel in 6 months'."
        ],
        "description": "Begin working toward 'write a novel in 6 months'. Set the foundation this phase; you define what 'done' looks like.",
        "duration_weeks": 3,
        "id": 1,
        "is_completed": false,
        "objectives": [
          "Define your own objectives for this starting phase based on your goal."
        ],
        "resources": [
          {
            "description": "Hi, I'm John Fox, and as an editor I've helped hundreds of authors write, edit and publish their novels. If you're planning on writing a novel, you've come to the right place. Let me guide you through",
            "resource_type": "article",
            "title": "12 Steps to Write a Bestselling Novel (in less than 6 months)",
            "url": "https://thejohnfox.com/2021/07/12-steps-to-write-a-bestselling-novel-in-less-than-6-months/"
          },
          {
            "description": "Wondering if it's possible to write a novel in six months? Follow along as A.E. outlines how to make your novel a reality.",
            "resource_type": "article",
            "title": "Writing a Novel in Six Months - crazednovelist.com",
            "url": "https://www.crazednovelist.com/post/writing-a-novel-in-six-months"
          },
          {
            "description": "YouTube video tutorial on how i wrote an entire novel in 3(ish) months📝💻 (fast *REAL* drafting tips) no nonsense . Watch video online.",
            "resource_type": "video",
            "title": "how i wrote an entire novel in 3(ish) months📝💻  (fast *REAL* drafting tips) no nonsense  ",
            "url": "https://www.youtube.com/watch?v=1B-CduVBxdc"
          }
        ],
        "title": "Phase 1: Plan & Begin 'write a novel in 6 months'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-09-16 (buffer: 2026-09-23)",
        "deliverables": [
          "Tangible output advancing 'write a novel in 6 months'."
        ],
        "description": "Continue making progress on 'write a novel in 6 months'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 2,
        "is_completed": false,
        "objectives": [
          "Advance 'write a novel in 6 months' during this phase."
        ],
        "resources": [
          {
            "description": "Write, revise, and polish your novel over 6 months. A sustainable plan for working writers — from concept through beta-reader-ready manuscript.",
            "resource_type": "article",
            "title": "How to Write a Novel in 6 Months — Step-by-Step Plan | Chosen Focus",
            "url": "https://chosenfocus.com/goals/write-a-novel/6-months"
          },
          {
            "description": "Introduction Most authors have heard of National Novel Writing Month, also known as NaNoWriMo, a novel writing challenge to write a 50,000 word novel during the month of November. Many famous authors,",
            "resource_type": "article",
            "title": "PDF The Six-Month Novel Writing Plan - Authors Publish",
            "url": "https://authorspublish.com/wp-content/uploads/2017/04/The-Six-Month-Novel-Writing-Plan-1.pdf"
          },
          {
            "description": "Learning Center > Writing > What is Novel Writing? How to Write a Romance Novel: My 13-Step Guide By Josh Fechter Last updated: June 27, 2026 Our reviewers evaluate career opinion pieces independently",
            "resource_type": "article",
            "title": "\n      \n  How to Write a Romance Novel: My 13-Step Guide - Squibler Learning Center\n\n    ",
            "url": "https://www.squibler.io/learn/writing/novel-writing/write-romance-novel"
          }
        ],
        "title": "Phase 2: Progress on 'write a novel in 6 months'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-10-07 (buffer: 2026-10-14)",
        "deliverables": [
          "Tangible output advancing 'write a novel in 6 months'."
        ],
        "description": "Continue making progress on 'write a novel in 6 months'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 3,
        "is_completed": false,
        "objectives": [
          "Advance 'write a novel in 6 months' during this phase."
        ],
        "resources": [
          {
            "description": "It truly is possible to write the complete first draft of your novel, memoir, essay collection, nonfiction manifesto, or other—in six months. I know because that's what I do. I help writers and would-",
            "resource_type": "article",
            "title": "How to Write a Book in 6 Months - Whole House",
            "url": "https://www.thisiswholehouse.com/blog/write-a-book-in-6-months"
          },
          {
            "description": "Written by Caitlin Jans September 26th, 2016 The Six Month Novel Writing Plan Most authors have heard of National Novel Writing Month, also known as NaNoWriMo. It is a novel writing challenge where pa",
            "resource_type": "article",
            "title": "The Six Month Novel Writing Plan - Authors Publish",
            "url": "https://authorspublish.com/the-six-month-novel-writing-plan/"
          },
          {
            "description": "reedsy studio Apps reedsy marketplace Assemble a team of professionals reedsy studio The writing app for authors reedsy learning Writing courses, events and memberships reedsy discovery Get your book ",
            "resource_type": "article",
            "title": "Reedsy Studio: A FREE Online Novel Planning App | Reedsy",
            "url": "https://reedsy.com/studio/plan-a-book/"
          }
        ],
        "title": "Phase 3: Progress on 'write a novel in 6 months'"
      },
      {
        "buffer_days": 7,
        "completion_type": "final_delivery",
        "deadline": "2026-10-28 (buffer: 2026-11-04)",
        "deliverables": [
          "A finished result for 'write a novel in 6 months'."
        ],
        "description": "Drive 'write a novel in 6 months' to a finish you define.",
        "duration_weeks": 3,
        "id": 4,
        "is_completed": false,
        "objectives": [
          "Complete the work so it is delivered to your satisfaction."
        ],
        "resources": [
          {
            "description": "The idea of writing a novel can be daunting, but with the right strategies, you can do it. Here's how to write a novel in just six months.",
            "resource_type": "article",
            "title": "How to Write a Novel in Six Months - The Write Practice",
            "url": "https://thewritepractice.com/write-novel/"
          },
          {
            "description": "YouTube video tutorial on How to write a novel in a month (with Luke Kondor). Watch video online.",
            "resource_type": "video",
            "title": "How to write a novel in a month (with Luke Kondor)",
            "url": "https://www.youtube.com/watch?v=0GtBQZgsF4w"
          },
          {
            "description": "Write and Publish Your Novel The best free AI novel writing software for planning, writing, and publishing your book in one place. How to Use Novel Writing Software Prompt: Describe your novel in the ",
            "resource_type": "article",
            "title": "\n      \n    Novel Writing Software | Squibler\n\n    ",
            "url": "https://squibler.io/novel-writing-software/"
          }
        ],
        "title": "Final Phase: Deliver 'write a novel in 6 months'"
      }
    ],
    "title": "Your Personalized Roadmap: write a novel in 6 months",
    "total_buffer_days": 28,
    "total_duration_weeks": 12
  },
  "score": 0,
  "status": "active",
  "total_phases": 4
}
```

### GOALS get goal_0002
REQ: GET /goals/goal_0002
HTTP 200  time 0.003379s
```json
{
  "completed_phases": 0,
  "created_at": "2026-08-05T16:53:43Z",
  "goal": "write a novel in 6 months",
  "goal_id": "goal_0002",
  "intent": "creative-writing",
  "roadmap": {
    "overview": "A 12-week journey (fiction hours/week) across 4 phases.",
    "phases": [
      {
        "buffer_days": 7,
        "completion_type": "foundation",
        "deadline": "2026-08-26 (buffer: 2026-09-02)",
        "deliverables": [
          "A defined starting point for 'write a novel in 6 months'."
        ],
        "description": "Begin working toward 'write a novel in 6 months'. Set the foundation this phase; you define what 'done' looks like.",
        "duration_weeks": 3,
        "id": 1,
        "is_completed": false,
        "objectives": [
          "Define your own objectives for this starting phase based on your goal."
        ],
        "resources": [
          {
            "description": "Hi, I'm John Fox, and as an editor I've helped hundreds of authors write, edit and publish their novels. If you're planning on writing a novel, you've come to the right place. Let me guide you through",
            "resource_type": "article",
            "title": "12 Steps to Write a Bestselling Novel (in less than 6 months)",
            "url": "https://thejohnfox.com/2021/07/12-steps-to-write-a-bestselling-novel-in-less-than-6-months/"
          },
          {
            "description": "Wondering if it's possible to write a novel in six months? Follow along as A.E. outlines how to make your novel a reality.",
            "resource_type": "article",
            "title": "Writing a Novel in Six Months - crazednovelist.com",
            "url": "https://www.crazednovelist.com/post/writing-a-novel-in-six-months"
          },
          {
            "description": "YouTube video tutorial on how i wrote an entire novel in 3(ish) months📝💻 (fast *REAL* drafting tips) no nonsense . Watch video online.",
            "resource_type": "video",
            "title": "how i wrote an entire novel in 3(ish) months📝💻  (fast *REAL* drafting tips) no nonsense  ",
            "url": "https://www.youtube.com/watch?v=1B-CduVBxdc"
          }
        ],
        "title": "Phase 1: Plan & Begin 'write a novel in 6 months'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-09-16 (buffer: 2026-09-23)",
        "deliverables": [
          "Tangible output advancing 'write a novel in 6 months'."
        ],
        "description": "Continue making progress on 'write a novel in 6 months'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 2,
        "is_completed": false,
        "objectives": [
          "Advance 'write a novel in 6 months' during this phase."
        ],
        "resources": [
          {
            "description": "Write, revise, and polish your novel over 6 months. A sustainable plan for working writers — from concept through beta-reader-ready manuscript.",
            "resource_type": "article",
            "title": "How to Write a Novel in 6 Months — Step-by-Step Plan | Chosen Focus",
            "url": "https://chosenfocus.com/goals/write-a-novel/6-months"
          },
          {
            "description": "Introduction Most authors have heard of National Novel Writing Month, also known as NaNoWriMo, a novel writing challenge to write a 50,000 word novel during the month of November. Many famous authors,",
            "resource_type": "article",
            "title": "PDF The Six-Month Novel Writing Plan - Authors Publish",
            "url": "https://authorspublish.com/wp-content/uploads/2017/04/The-Six-Month-Novel-Writing-Plan-1.pdf"
          },
          {
            "description": "Learning Center > Writing > What is Novel Writing? How to Write a Romance Novel: My 13-Step Guide By Josh Fechter Last updated: June 27, 2026 Our reviewers evaluate career opinion pieces independently",
            "resource_type": "article",
            "title": "\n      \n  How to Write a Romance Novel: My 13-Step Guide - Squibler Learning Center\n\n    ",
            "url": "https://www.squibler.io/learn/writing/novel-writing/write-romance-novel"
          }
        ],
        "title": "Phase 2: Progress on 'write a novel in 6 months'"
      },
      {
        "buffer_days": 7,
        "completion_type": "checkpoint",
        "deadline": "2026-10-07 (buffer: 2026-10-14)",
        "deliverables": [
          "Tangible output advancing 'write a novel in 6 months'."
        ],
        "description": "Continue making progress on 'write a novel in 6 months'. You set the focus for this phase.",
        "duration_weeks": 3,
        "id": 3,
        "is_completed": false,
        "objectives": [
          "Advance 'write a novel in 6 months' during this phase."
        ],
        "resources": [
          {
            "description": "It truly is possible to write the complete first draft of your novel, memoir, essay collection, nonfiction manifesto, or other—in six months. I know because that's what I do. I help writers and would-",
            "resource_type": "article",
            "title": "How to Write a Book in 6 Months - Whole House",
            "url": "https://www.thisiswholehouse.com/blog/write-a-book-in-6-months"
          },
          {
            "description": "Written by Caitlin Jans September 26th, 2016 The Six Month Novel Writing Plan Most authors have heard of National Novel Writing Month, also known as NaNoWriMo. It is a novel writing challenge where pa",
            "resource_type": "article",
            "title": "The Six Month Novel Writing Plan - Authors Publish",
            "url": "https://authorspublish.com/the-six-month-novel-writing-plan/"
          },
          {
            "description": "reedsy studio Apps reedsy marketplace Assemble a team of professionals reedsy studio The writing app for authors reedsy learning Writing courses, events and memberships reedsy discovery Get your book ",
            "resource_t
```

### GOALS leaderboard
REQ: GET /goals/leaderboard
HTTP 200  time 0.003103s
```json
{
  "entries": [
    {
      "completed_phases": 0,
      "created_at": "2026-08-05T16:53:43Z",
      "goal": "write a novel in 6 months",
      "goal_id": "goal_0002",
      "score": 0,
      "total_phases": 4,
      "user_name": "Anonymous"
    },
    {
      "completed_phases": 0,
      "created_at": "2026-08-05T16:53:38Z",
      "goal": "learn to build a privacy-first search engine using Rust",
      "goal_id": "goal_0001",
      "score": 0,
      "total_phases": 4,
      "user_name": "Anonymous"
    }
  ],
  "total_entries": 2
}
```

### GOALS update progress goal_0002
REQ: POST /goals/goal_0002/progress
BODY: {"phase_id":0,"is_completed":true}
HTTP 400  time 0.003464s
```json
{
  "error": "invalid_phase",
  "message": "Phase 0 does not exist for goal 'goal_0002'"
}
```

### GOALS complete phase 0 of goal_0002
REQ: POST /goals/goal_0002/phases/0/complete
BODY: {}
HTTP 400  time 0.003152s
```json
{
  "error": "invalid_phase",
  "message": "Phase 0 does not exist for goal 'goal_0002'"
}
```

=== round v2 exercise end 2026-08-05T16:55:53Z ===


### GOALS update progress goal_0002 (CORRECTED: phase_id=1, 1-based)
REQ: POST /goals/goal_0002/progress
BODY: {"phase_id":1,"is_completed":true}
HTTP 200  time 0.004081s
```json
{
  "completed_phases": 1,
  "goal": "write a novel in 6 months",
  "goal_id": "goal_0002",
  "is_completed": true,
  "phase_id": 1,
  "roadmap": { "overview": "A 12-week journey (fiction hours/week) across 4 phases.", "phases": [ { "id": 1, "is_completed": true, ... } ] },
  "score": <int>,
  "status": "active",
  "total_phases": 4
}
```

### GOALS complete phase 1 of goal_0002 (CORRECTED: phase_id=1, 1-based)
REQ: POST /goals/goal_0002/phases/1/complete
BODY: {}
HTTP 200  time 0.003209s
```json
{
  "completed_phase_id": 1,
  "completed_phases": 1,
  "goal": "write a novel in 6 months",
  "goal_id": "goal_0002",
  "roadmap": { "phases": [ { "id": 1, "is_completed": true, ... } ] },
  "score": <int>,
  "status": "active",
  "total_phases": 4
}
```
