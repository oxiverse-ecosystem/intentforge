import urllib.request, re

url = 'https://www.amazon.com/Apple-iPhone-16-Pro-Max/dp/B0DHJ896RY'
req = urllib.request.Request(url, headers={
    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
    'Accept': 'text/html,application/xhtml+xml',
    'Accept-Language': 'en-US,en;q=0.9',
})
with urllib.request.urlopen(req, timeout=15) as r:
    html = r.read().decode('utf-8', errors='replace')
print(f'Fetched {len(html)} bytes')

# Find all context around a-price-whole
for m in re.finditer(r'a-price-whole', html):
    start = max(0, m.start() - 200)
    end = min(len(html), m.end() + 200)
    snippet = html[start:end].replace('\n', ' ').replace('\r', ' ')
    print(f'\n--- a-price-whole context ---')
    print(f'{snippet[:400]}')
    break  # Just first occurrence

# Check data-price and data-amount attributes
for attr in ['data-price', 'data-amount', 'data-sale-price']:
    for m in re.finditer(rf'{attr}\s*=\s*["\']([\d,.]+)["\']', html):
        print(f'{attr}={m.group(1)}')
        break

# Check pattern 4 structure: class="...price..."> $<digits>
# Amazon might be: <span class="a-price">$<span class="a-price-whole">1,199</span>.<span class="a-price-decimal">00</span></span>
for m in re.finditer(r'class\s*=\s*["\'][^"\']*a-price[^"\']*["\'][^>]*>([^<]+)<', html):
    print(f'a-price text: {m.group(1).strip()[:50]}')
    break
