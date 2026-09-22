import urllib.request, re

url = 'https://www.amazon.com/Apple-iPhone-16-Pro-Max/dp/B0DHJ896RY'
req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'})
with urllib.request.urlopen(req, timeout=10) as r:
    html = r.read().decode('utf-8', errors='replace')
print(f'Fetched {len(html)} bytes')

# data-price
print('data-price matches:', re.findall(r'data-price\s*=\s*["\']([\d,.]+)["\']', html)[:5])
# a-offscreen
print('a-offscreen matches:', re.findall(r'class\s*=\s*["\']a-offscreen["\'][^>]*>([^<]+)', html)[:5])
# price block
print('priceblock matches:', re.findall(r'class\s*=\s*["\']priceBlock[^"\']*["\'][^>]*>([^<]+)', html)[:5])
# span with dollar
span_matches = re.findall(r'<span[^>]*>\s*\$([\d,]+\.\d{2})\s*</span>', html)[:10]
print('span $ matches:', span_matches)
