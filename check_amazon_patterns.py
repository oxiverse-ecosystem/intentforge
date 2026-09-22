import urllib.request, re

# Simulate the fetch the gateway does
url = 'https://www.amazon.com/Apple-iPhone-16-Pro-Max/dp/B0DHJ896RY'
req = urllib.request.Request(url, headers={
    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
})
with urllib.request.urlopen(req, timeout=10) as r:
    html = r.read().decode('utf-8', errors='replace')

# Test pattern 1 (class contains price-related + currency + digits)
pat1 = re.compile(r'(?i)(?:\$|€|£|¥|₹|Rs\.?|INR|USD|EUR|GBP)\s*([\d,]+\.?\d*)')
# Find all matches near price-related text
for m in pat1.finditer(html):
    start = max(0, m.start() - 100)
    end = min(len(html), m.end() + 100)
    ctx = html[start:end].replace('\n', ' ')
    if 'price' in ctx.lower() or 'a-price' in ctx:
        print(f'Match: {m.group(0)} at pos {m.start()}')
        print(f'  Context: {ctx[:200]}')
        print()
        break

# Also check: the a-offscreen span contains the actual price
for m in re.finditer(r'class\s*=\s*["\']a-offscreen["\'][^>]*>([^<]+)<', html):
    print(f'a-offscreen price: {m.group(1)}')
    break
