import urllib.request

url = 'https://www.amazon.com/Apple-iPhone-16-Pro-Max/dp/B0DHJ896RY'
req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'})
with urllib.request.urlopen(req, timeout=10) as r:
    html = r.read().decode('utf-8', errors='replace')
print(f'Fetched {len(html)} bytes')
print('First 500 chars:', html[:500])
print('Last 500 chars:', html[-500:])
