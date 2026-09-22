import urllib.request, re, json

def fetch_and_check(name, url):
    print(f'\n=== {name} ===')
    try:
        req = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
            'Accept': 'text/html,application/xhtml+xml',
            'Accept-Language': 'en-US,en;q=0.9',
        })
        with urllib.request.urlopen(req, timeout=5) as r:
            html = r.read().decode('utf-8', errors='replace')
        print(f'Fetched {len(html)} bytes')
        
        # Check for common price patterns
        patterns = {
            'data-price': r'data-price\s*=\s*["\']([\d,.]+)["\']',
            'data-amount': r'data-amount\s*=\s*["\']([\d,.]+)["\']',
            'price-class': r'class\s*=\s*["\'][^"\']*(?:price|Price|PRICE|current-price|sale-price|selling-price|offer-price)[^"\']*["\'][^>]*>(?:\s*<[^>]*>)*\s*(?:[\$€£¥₹]|Rs\.?|INR|USD|EUR|GBP)?\s*([\d,]+\.?\d*)',
            'itemprop-price': r'itemprop\s*=\s*["\']price["\'][^>]*>([\d,.]+)',
            'json-ld-price': r'"price"\s*:\s*"?(\d[\d.]*)"?',
            'a-price-whole': r'class\s*=\s*["\']a-price-whole["\'][^>]*>([\d,]+)',
            'span-dollar': r'<span[^>]*>\s*\$([\d,]+\.\d{2})\s*</span>',
            'og-price': r'<meta\s+(?:itemprop|property)\s*=\s*["\']product:price:amount["\'][^>]*\s*content\s*=\s*["\']([\d.]+)["\']',
        }
        for p_name, pattern in patterns.items():
            matches = re.findall(pattern, html)
            if matches:
                print(f'  {p_name}: {matches[:3]}')
    except Exception as e:
        print(f'ERROR: {e}')

fetch_and_check('BestBuy', 'https://www.bestbuy.com/site/searchpage.jsp?id=pcat17071&st=iphone+16+pro+max')
fetch_and_check('Amazon', 'https://www.amazon.com/Apple-iPhone-16-Pro-Max/dp/B0DHJ896RY')
fetch_and_check('AT&T', 'https://www.att.com/buy/phones/apple-iphone-16-pro-max.html')
